#[cfg(test)]
mod cli {
    use std::{
        collections::{HashMap, HashSet},
        error::Error,
        fs::{self, File},
        io::Write,
        path::{Path, PathBuf},
        time::Duration,
    };

    use anyhow::anyhow;
    use assert_cmd::Command;
    use assert_json_diff::assert_json_include;
    use http::{Method, StatusCode};
    use lychee_lib::{InputSource, ResponseBody};
    use predicates::{
        ord::eq,
        prelude::{PredicateBooleanExt, predicate},
        str::{contains, is_empty},
    };
    use pretty_assertions::assert_eq;
    use regex::Regex;
    use serde::Serialize;
    use serde_json::Value;
    use tempfile::NamedTempFile;
    use uuid::Uuid;
    use wiremock::{
        Mock, ResponseTemplate,
        matchers::{basic_auth, method},
    };

    type Result<T> = std::result::Result<T, Box<dyn Error>>;

    // The lychee cache file name is used for some tests.
    // Since it is currently static and can't be overwritten, declare it as a
    // constant.
    const LYCHEE_CACHE_FILE: &str = ".lycheecache";

    /// Helper macro to create a mock server which returns a custom status code.
    macro_rules! mock_server {
        ($status:expr $(, $func:tt ($($arg:expr),*))*) => {{
            let mock_server = wiremock::MockServer::start().await;
            let response_template = wiremock::ResponseTemplate::new(http::StatusCode::from($status));
            let template = response_template$(.$func($($arg),*))*;
            wiremock::Mock::given(wiremock::matchers::method("GET")).respond_with(template).mount(&mock_server).await;
            mock_server
        }};
    }

    /// Helper macro to create a mock server which returns a 200 OK and a custom response body.
    macro_rules! mock_response {
        ($body:expr) => {{
            let mock_server = wiremock::MockServer::start().await;
            let template = wiremock::ResponseTemplate::new(200).set_body_string($body);
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .respond_with(template)
                .mount(&mock_server)
                .await;
            mock_server
        }};
    }

    /// Gets the "main" binary name (e.g. `lychee`)
    fn main_command() -> Command {
        Command::cargo_bin(env!("CARGO_PKG_NAME")).expect("Couldn't get cargo package name")
    }

    /// Helper function to get the root path of the project.
    fn root_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf()
    }

    /// Helper function to get the path to the fixtures directory.
    fn fixtures_path() -> PathBuf {
        root_path().join("fixtures")
    }

    #[derive(Default, Serialize)]
    struct MockResponseStats {
        detailed_stats: bool,
        total: usize,
        successful: usize,
        unknown: usize,
        unsupported: usize,
        timeouts: usize,
        redirects: usize,
        excludes: usize,
        errors: usize,
        cached: usize,
        success_map: HashMap<InputSource, HashSet<ResponseBody>>,
        error_map: HashMap<InputSource, HashSet<ResponseBody>>,
        suggestion_map: HashMap<InputSource, HashSet<ResponseBody>>,
        excluded_map: HashMap<InputSource, HashSet<ResponseBody>>,
    }

    /// Helper macro to test the output of the JSON format.
    macro_rules! test_json_output {
        ($test_file:expr, $expected:expr $(, $arg:expr)*) => {{
            let mut cmd = main_command();
            let test_path = fixtures_path().join($test_file);
            let outfile = format!("{}.json", uuid::Uuid::new_v4());

            let result = cmd$(.arg($arg))*.arg("--output").arg(&outfile).arg("--format").arg("json").arg(test_path).assert();

            let output = std::fs::read_to_string(&outfile)?;
            std::fs::remove_file(outfile)?;

            let actual: Value = serde_json::from_str(&output)?;
            let expected: Value = serde_json::to_value(&$expected)?;

            result.success();
            assert_json_include!(actual: actual, expected: expected);
            Ok(())
        }};
    }

    /// Test that the default report output format (compact) and mode (color)
    /// prints the failed URLs as well as their status codes on error. Make
    /// sure that the status code only occurs once.
    #[test]
    fn test_compact_output_format_contains_status() -> Result<()> {
        let test_path = fixtures_path().join("TEST_INVALID_URLS.html");

        let mut cmd = main_command();
        cmd.arg("--format")
            .arg("compact")
            .arg("--mode")
            .arg("color")
            .arg(test_path)
            .env("FORCE_COLOR", "1")
            .assert()
            .failure()
            .code(2);

        let output = cmd.output()?;

        // Check that the output contains the status code (once) and the URL
        let output_str = String::from_utf8_lossy(&output.stdout);

        // The expected output is as follows:
        // "Find details below."
        // [EMPTY LINE]
        // [path/to/file]:
        //      [400] https://httpbin.org/status/404
        //      [500] https://httpbin.org/status/500
        //      [502] https://httpbin.org/status/502
        // (the order of the URLs may vary)

        // Check that the output contains the file path
        assert!(output_str.contains("TEST_INVALID_URLS.html"));

        let re = Regex::new(r"\s{5}\[\d{3}\] https://httpbin\.org/status/\d{3}").unwrap();
        let matches: Vec<&str> = re.find_iter(&output_str).map(|m| m.as_str()).collect();

        // Check that the status code occurs only once
        assert_eq!(matches.len(), 3);

        Ok(())
    }

    /// Test JSON output format
    #[tokio::test]
    async fn test_json_output() -> Result<()> {
        // Server that returns a bunch of 200 OK responses
        let mock_server_ok = mock_server!(StatusCode::OK);
        let mut cmd = main_command();
        cmd.arg("--format")
            .arg("json")
            .arg("-vv")
            .arg("--no-progress")
            .arg("-")
            .write_stdin(mock_server_ok.uri())
            .assert()
            .success();
        let output = cmd.output().unwrap();
        let output_json = serde_json::from_slice::<Value>(&output.stdout)?;

        // Check that the output is valid JSON
        assert!(output_json.is_object());
        // Check that the output contains the expected keys
        assert!(output_json.get("detailed_stats").is_some());
        assert!(output_json.get("success_map").is_some());
        assert!(output_json.get("error_map").is_some());
        assert!(output_json.get("excluded_map").is_some());

        // Check the success map
        let success_map = output_json["success_map"].as_object().unwrap();
        assert_eq!(success_map.len(), 1);

        // Get the actual URL from the mock server for comparison
        let mock_url = mock_server_ok.uri();

        // Create the expected success map structure
        let expected_success_map = serde_json::json!({
            "stdin": [
                {
                    "status": {
                        "code": 200,
                        "text": "200 OK"
                    },
                    "url": format!("{mock_url}/"),
                }
            ]
        });

        // Compare the actual success map with the expected one
        assert_eq!(
            success_map,
            expected_success_map.as_object().unwrap(),
            "Success map doesn't match expected structure"
        );

        Ok(())
    }

    /// JSON-formatted output should always be valid JSON.
    /// Additional hints and error messages should be printed to `stderr`.
    /// See https://github.com/lycheeverse/lychee/issues/1355
    #[test]
    fn test_valid_json_output_to_stdout_on_error() -> Result<()> {
        let test_path = fixtures_path().join("TEST_GITHUB_404.md");

        let mut cmd = main_command();
        cmd.arg("--format")
            .arg("json")
            .arg(test_path)
            .assert()
            .failure()
            .code(2);

        let output = cmd.output()?;

        // Check that the output is valid JSON
        assert!(serde_json::from_slice::<Value>(&output.stdout).is_ok());
        Ok(())
    }

    #[test]
    fn test_detailed_json_output_on_error() -> Result<()> {
        let test_path = fixtures_path().join("TEST_DETAILED_JSON_OUTPUT_ERROR.md");

        let mut cmd = main_command();
        cmd.arg("--format")
            .arg("json")
            .arg(&test_path)
            .assert()
            .failure()
            .code(2);

        let output = cmd.output()?;

        // Check that the output is valid JSON
        assert!(serde_json::from_slice::<Value>(&output.stdout).is_ok());

        // Parse site error status from the error_map
        let output_json = serde_json::from_slice::<Value>(&output.stdout).unwrap();
        let site_error_status =
            &output_json["error_map"][&test_path.to_str().unwrap()][0]["status"];

        assert_eq!(
            "error sending request for url (https://expired.badssl.com/) Maybe a certificate error?",
            site_error_status["details"]
        );
        Ok(())
    }

    #[test]
    fn test_exclude_all_private() -> Result<()> {
        test_json_output!(
            "TEST_ALL_PRIVATE.md",
            MockResponseStats {
                total: 7,
                excludes: 7,
                ..MockResponseStats::default()
            },
            "--exclude-all-private"
        )
    }

    #[test]
    fn test_local_directories() -> Result<()> {
        test_json_output!(
            "TEST_LOCAL_DIRECTORIES.md",
            MockResponseStats {
                total: 4,
                successful: 4,
                ..MockResponseStats::default()
            }
        )
    }

    #[test]
    fn test_email() -> Result<()> {
        test_json_output!(
            "TEST_EMAIL.md",
            MockResponseStats {
                total: 5,
                excludes: 0,
                successful: 5,
                ..MockResponseStats::default()
            },
            "--include-mail"
        )
    }

    #[test]
    fn test_exclude_email_by_default() -> Result<()> {
        test_json_output!(
            "TEST_EMAIL.md",
            MockResponseStats {
                total: 5,
                excludes: 3,
                successful: 2,
                ..MockResponseStats::default()
            }
        )
    }

    #[test]
    fn test_email_html_with_subject() -> Result<()> {
        let mut cmd = main_command();
        let input = fixtures_path().join("TEST_EMAIL_QUERY_PARAMS.html");

        cmd.arg("--dump")
            .arg(input)
            .arg("--include-mail")
            .assert()
            .success()
            .stdout(contains("hello@example.org?subject=%5BHello%5D"));

        Ok(())
    }

    #[test]
    fn test_email_markdown_with_subject() -> Result<()> {
        let mut cmd = main_command();
        let input = fixtures_path().join("TEST_EMAIL_QUERY_PARAMS.md");

        cmd.arg("--dump")
            .arg(input)
            .arg("--include-mail")
            .assert()
            .success()
            .stdout(contains("hello@example.org?subject=%5BHello%5D"));

        Ok(())
    }

    #[test]
    fn test_stylesheet_misinterpreted_as_email() -> Result<()> {
        test_json_output!(
            "TEST_STYLESHEET_LINK.md",
            MockResponseStats {
                total: 0,
                ..MockResponseStats::default()
            }
        )
    }

    /// Test that a GitHub link can be checked without specifying the token.
    #[test]
    fn test_check_github_no_token() -> Result<()> {
        test_json_output!(
            "TEST_GITHUB.md",
            MockResponseStats {
                total: 1,
                successful: 1,
                ..MockResponseStats::default()
            }
        )
    }

    /// Test unsupported URI schemes
    #[test]
    fn test_unsupported_uri_schemes_are_ignored() {
        let mut cmd = main_command();
        let test_schemes_path = fixtures_path().join("TEST_SCHEMES.txt");

        // Exclude file link because it doesn't exist on the filesystem.
        // (File URIs are absolute paths, which we don't have.)
        // Nevertheless, the `file` scheme should be recognized.
        cmd.arg(test_schemes_path)
            .arg("--exclude")
            .arg("file://")
            .env_clear()
            .assert()
            .success()
            .stdout(contains("3 Total"))
            .stdout(contains("1 OK"))
            .stdout(contains("1 Excluded"));
    }

    #[test]
    fn test_resolve_paths() {
        let mut cmd = main_command();
        let dir = fixtures_path().join("resolve_paths");

        cmd.arg("--offline")
            .arg("--base-url")
            .arg(&dir)
            .arg(dir.join("index.html"))
            .env_clear()
            .assert()
            .success()
            .stdout(contains("3 Total"))
            .stdout(contains("3 OK"));
    }

    #[test]
    fn test_resolve_paths_from_root_dir() {
        let mut cmd = main_command();
        let dir = fixtures_path().join("resolve_paths_from_root_dir");

        cmd.arg("--offline")
            .arg("--include-fragments")
            .arg("--root-dir")
            .arg(&dir)
            .arg(dir.join("nested").join("index.html"))
            .env_clear()
            .assert()
            .failure()
            .stdout(contains("7 Total"))
            .stdout(contains("5 OK"))
            .stdout(contains("2 Errors"));
    }

    #[test]
    fn test_resolve_paths_from_root_dir_and_base_url() {
        let mut cmd = main_command();
        let dir = fixtures_path();

        cmd.arg("--offline")
            .arg("--root-dir")
            .arg("/resolve_paths")
            .arg("--base-url")
            .arg(&dir)
            .arg(dir.join("resolve_paths").join("index.html"))
            .env_clear()
            .assert()
            .success()
            .stdout(contains("3 Total"))
            .stdout(contains("3 OK"));
    }

    #[test]
    fn test_youtube_quirk() {
        let url = "https://www.youtube.com/watch?v=NlKuICiT470&list=PLbWDhxwM_45mPVToqaIZNbZeIzFchsKKQ&index=7";

        main_command()
            .write_stdin(url)
            .arg("--verbose")
            .arg("--no-progress")
            .arg("-")
            .assert()
            .success()
            .stdout(contains("1 Total"))
            .stdout(contains("1 OK"));
    }

    #[test]
    fn test_crates_io_quirk() {
        let url = "https://crates.io/crates/lychee";

        main_command()
            .write_stdin(url)
            .arg("--verbose")
            .arg("--no-progress")
            .arg("-")
            .assert()
            .success()
            .stdout(contains("1 Total"))
            .stdout(contains("1 OK"));
    }

    #[test]
    // Exclude Twitter links because they require login to view tweets.
    // https://techcrunch.com/2023/06/30/twitter-now-requires-an-account-to-view-tweets/
    // https://github.com/zedeus/nitter/issues/919
    fn test_ignored_hosts() {
        let url = "https://twitter.com/zarfeblong/status/1339742840142872577";

        main_command()
            .write_stdin(url)
            .arg("--verbose")
            .arg("--no-progress")
            .arg("-")
            .assert()
            .success()
            .stdout(contains("1 Total"))
            .stdout(contains("1 Excluded"));
    }

    #[tokio::test]
    async fn test_failure_404_link() -> Result<()> {
        let mock_server = mock_server!(StatusCode::NOT_FOUND);
        let dir = tempfile::tempdir()?;
        let file_path = dir.path().join("test.txt");
        let mut file = File::create(&file_path)?;
        writeln!(file, "{}", mock_server.uri())?;

        let mut cmd = main_command();
        cmd.arg(file_path)
            .write_stdin(mock_server.uri())
            .assert()
            .failure()
            .code(2);

        Ok(())
    }

    #[test]
    fn test_schemes() {
        let mut cmd = main_command();
        let test_schemes_path = fixtures_path().join("TEST_SCHEMES.md");

        cmd.arg(test_schemes_path)
            .arg("--scheme")
            .arg("https")
            .arg("--scheme")
            .arg("http")
            .env_clear()
            .assert()
            .success()
            .stdout(contains("3 Total"))
            .stdout(contains("2 OK"))
            .stdout(contains("1 Excluded"));
    }

    #[test]
    fn test_caching_single_file() {
        let mut cmd = main_command();
        // Repetitions in one file shall all be checked and counted only once.
        let test_schemes_path_1 = fixtures_path().join("TEST_REPETITION_1.txt");

        cmd.arg(&test_schemes_path_1)
            .env_clear()
            .assert()
            .success()
            .stdout(contains("1 Total"))
            .stdout(contains("1 OK"));
    }

    #[test]
    // Test that two identical requests don't get executed twice.
    fn test_caching_across_files() -> Result<()> {
        // Repetitions across multiple files shall all be checked only once.
        let repeated_uris = fixtures_path().join("TEST_REPETITION_*.txt");

        test_json_output!(
            repeated_uris,
            MockResponseStats {
                total: 2,
                cached: 1,
                successful: 2,
                excludes: 0,
                ..MockResponseStats::default()
            },
            // Two requests to the same URI may be executed in parallel. As a
            // result, the response might not be cached and the test would be
            // flaky. Therefore limit the concurrency to one request at a time.
            "--max-concurrency",
            "1"
        )
    }

    #[test]
    fn test_failure_github_404_no_token() {
        let mut cmd = main_command();
        let test_github_404_path = fixtures_path().join("TEST_GITHUB_404.md");

        cmd.arg(test_github_404_path)
            .arg("--no-progress")
            .env_clear()
            .assert()
            .failure()
            .code(2)
            .stdout(contains(
                r#"[404] https://github.com/mre/idiomatic-rust-doesnt-exist-man | Rejected status code (this depends on your "accept" configuration): Not Found"#
            ))
            .stderr(contains(
                "There were issues with GitHub URLs. You could try setting a GitHub token and running lychee again.",
            ));
    }

    #[tokio::test]
    async fn test_stdin_input() {
        let mut cmd = main_command();
        let mock_server = mock_server!(StatusCode::OK);

        cmd.arg("-")
            .write_stdin(mock_server.uri())
            .assert()
            .success();
    }

    #[tokio::test]
    async fn test_stdin_input_failure() {
        let mut cmd = main_command();
        let mock_server = mock_server!(StatusCode::INTERNAL_SERVER_ERROR);

        cmd.arg("-")
            .write_stdin(mock_server.uri())
            .assert()
            .failure()
            .code(2);
    }

    #[tokio::test]
    async fn test_stdin_input_multiple() {
        let mut cmd = main_command();
        let mock_server_a = mock_server!(StatusCode::OK);
        let mock_server_b = mock_server!(StatusCode::OK);

        // this behavior (treating multiple `-` as separate inputs) is the same as most CLI tools
        // that accept `-` as stdin, e.g. `cat`, `bat`, `grep` etc.
        cmd.arg("-")
            .arg("-")
            .write_stdin(mock_server_a.uri())
            .write_stdin(mock_server_b.uri())
            .assert()
            .success();
    }

    #[test]
    fn test_missing_file_ok_if_skip_missing() {
        let mut cmd = main_command();
        let filename = format!("non-existing-file-{}", uuid::Uuid::new_v4());

        cmd.arg(&filename).arg("--skip-missing").assert().success();
    }

    #[test]
    fn test_skips_hidden_files_by_default() {
        main_command()
            .arg(fixtures_path().join("hidden/"))
            .assert()
            .success()
            .stdout(contains("0 Total"));
    }

    #[test]
    fn test_include_hidden_file() {
        main_command()
            .arg(fixtures_path().join("hidden/"))
            .arg("--hidden")
            .assert()
            .success()
            .stdout(contains("1 Total"));
    }

    #[test]
    fn test_skips_ignored_files_by_default() {
        main_command()
            .arg(fixtures_path().join("ignore/"))
            .assert()
            .success()
            .stdout(contains("0 Total"));
    }

    #[test]
    fn test_include_ignored_file() {
        main_command()
            .arg(fixtures_path().join("ignore/"))
            .arg("--no-ignore")
            .assert()
            .success()
            .stdout(contains("1 Total"));
    }

    #[tokio::test]
    async fn test_glob() -> Result<()> {
        // using Result to be able to use `?`
        let mut cmd = main_command();

        let dir = tempfile::tempdir()?;
        let mock_server_a = mock_server!(StatusCode::OK);
        let mock_server_b = mock_server!(StatusCode::OK);
        let mut file_a = File::create(dir.path().join("a.md"))?;
        let mut file_b = File::create(dir.path().join("b.md"))?;

        writeln!(file_a, "{}", mock_server_a.uri().as_str())?;
        writeln!(file_b, "{}", mock_server_b.uri().as_str())?;

        cmd.arg(dir.path().join("*.md"))
            .arg("--verbose")
            .assert()
            .success()
            .stdout(contains("2 Total"));

        Ok(())
    }

    #[cfg(target_os = "linux")] // MacOS and Windows have case-insensitive filesystems
    #[tokio::test]
    async fn test_glob_ignore_case() -> Result<()> {
        let mut cmd = main_command();

        let dir = tempfile::tempdir()?;
        let mock_server_a = mock_server!(StatusCode::OK);
        let mock_server_b = mock_server!(StatusCode::OK);
        let mut file_a = File::create(dir.path().join("README.md"))?;
        let mut file_b = File::create(dir.path().join("readme.md"))?;

        writeln!(file_a, "{}", mock_server_a.uri().as_str())?;
        writeln!(file_b, "{}", mock_server_b.uri().as_str())?;

        cmd.arg(dir.path().join("[r]eadme.md"))
            .arg("--verbose")
            .arg("--glob-ignore-case")
            .assert()
            .success()
            .stdout(contains("2 Total"));

        Ok(())
    }

    #[tokio::test]
    async fn test_glob_recursive() -> Result<()> {
        let mut cmd = main_command();

        let dir = tempfile::tempdir()?;
        let subdir_level_1 = tempfile::tempdir_in(&dir)?;
        let subdir_level_2 = tempfile::tempdir_in(&subdir_level_1)?;

        let mock_server = mock_server!(StatusCode::OK);
        let mut file = File::create(subdir_level_2.path().join("test.md"))?;

        writeln!(file, "{}", mock_server.uri().as_str())?;

        // ** should be a recursive glob
        cmd.arg(dir.path().join("**/*.md"))
            .arg("--verbose")
            .assert()
            .success()
            .stdout(contains("1 Total"));

        Ok(())
    }

    /// Test formatted file output
    #[test]
    fn test_formatted_file_output() -> Result<()> {
        test_json_output!(
            "TEST.md",
            MockResponseStats {
                total: 12,
                successful: 10,
                excludes: 2,
                ..MockResponseStats::default()
            }
        )
    }

    /// Test writing output of `--dump` command to file
    #[test]
    fn test_dump_to_file() -> Result<()> {
        let mut cmd = main_command();
        let test_path = fixtures_path().join("TEST.md");
        let outfile = format!("{}", Uuid::new_v4());

        cmd.arg("--output")
            .arg(&outfile)
            .arg("--dump")
            .arg("--include-mail")
            .arg(test_path)
            .assert()
            .success();

        let output = fs::read_to_string(&outfile)?;

        // We expect 11 links in the test file
        // Running the command from the command line will print 9 links,
        // because the actual `--dump` command filters out the two
        // http(s)://example.com links
        assert_eq!(output.lines().count(), 12);
        fs::remove_file(outfile)?;
        Ok(())
    }

    /// Test excludes
    #[test]
    fn test_exclude_wildcard() -> Result<()> {
        let mut cmd = main_command();
        let test_path = fixtures_path().join("TEST.md");

        cmd.arg(test_path)
            .arg("--exclude")
            .arg(".*")
            .assert()
            .success()
            .stdout(contains("12 Excluded"));

        Ok(())
    }

    #[test]
    fn test_exclude_multiple_urls() -> Result<()> {
        let mut cmd = main_command();
        let test_path = fixtures_path().join("TEST.md");

        cmd.arg(test_path)
            .arg("--exclude")
            .arg("https://en.wikipedia.org/*")
            .arg("--exclude")
            .arg("https://ldra.com/")
            .assert()
            .success()
            .stdout(contains("4 Excluded"));

        Ok(())
    }

    #[tokio::test]
    async fn test_empty_config() -> Result<()> {
        let mock_server = mock_server!(StatusCode::OK);
        let config = fixtures_path().join("configs").join("empty.toml");
        let mut cmd = main_command();
        cmd.arg("--config")
            .arg(config)
            .arg("-")
            .write_stdin(mock_server.uri())
            .env_clear()
            .assert()
            .success()
            .stdout(contains("1 Total"))
            .stdout(contains("1 OK"));

        Ok(())
    }

    #[test]
    fn test_invalid_default_config() -> Result<()> {
        let test_path = fixtures_path().join("configs");
        let mut cmd = main_command();
        cmd.current_dir(test_path)
            .arg(".")
            .assert()
            .failure()
            .stderr(contains("Cannot load default configuration file"));

        Ok(())
    }

    #[tokio::test]
    async fn test_include_mail_config() -> Result<()> {
        let test_mail_address = "mailto:hello-test@testingabc.io";

        let mut config = NamedTempFile::new()?;
        writeln!(config, "include_mail = false")?;

        let mut cmd = main_command();
        cmd.arg("--config")
            .arg(config.path().to_str().unwrap())
            .arg("-")
            .write_stdin(test_mail_address)
            .env_clear()
            .assert()
            .success()
            .stdout(contains("1 Total"))
            .stdout(contains("1 Excluded"));

        let mut config = NamedTempFile::new()?;
        writeln!(config, "include_mail = true")?;

        let mut cmd = main_command();
        cmd.arg("--config")
            .arg(config.path().to_str().unwrap())
            .arg("-")
            .write_stdin(test_mail_address)
            .env_clear()
            .assert()
            .failure()
            .stdout(contains("1 Total"))
            .stdout(contains("1 Error"));

        Ok(())
    }

    #[tokio::test]
    async fn test_cache_config() -> Result<()> {
        let mock_server = mock_server!(StatusCode::OK);
        let config = fixtures_path().join("configs").join("cache.toml");
        let mut cmd = main_command();
        cmd.arg("--config")
            .arg(config)
            .arg("-")
            .write_stdin(mock_server.uri())
            .env_clear()
            .assert()
            .success()
            .stdout(contains("1 Total"))
            .stdout(contains("1 OK"));

        Ok(())
    }

    #[tokio::test]
    async fn test_invalid_config() {
        let config = fixtures_path().join("configs").join("invalid.toml");
        let mut cmd = main_command();
        cmd.arg("--config")
            .arg(config)
            .arg("-")
            .env_clear()
            .assert()
            .failure()
            .stderr(predicate::str::contains("Cannot load configuration file"))
            .stderr(predicate::str::contains("Failed to parse"))
            .stderr(predicate::str::contains("TOML parse error"))
            .stderr(predicate::str::contains("expected newline"));
    }

    #[tokio::test]
    async fn test_missing_config_error() {
        let mock_server = mock_server!(StatusCode::OK);
        let mut cmd = main_command();
        cmd.arg("--config")
            .arg("config.does.not.exist.toml")
            .arg("-")
            .write_stdin(mock_server.uri())
            .env_clear()
            .assert()
            .failure();
    }

    #[tokio::test]
    async fn test_config_example() {
        let mock_server = mock_server!(StatusCode::OK);
        let config = root_path().join("lychee.example.toml");
        let mut cmd = main_command();
        cmd.arg("--config")
            .arg(config)
            .arg("-")
            .write_stdin(mock_server.uri())
            .env_clear()
            .assert()
            .success();
    }

    #[tokio::test]
    async fn test_config_smoketest() {
        let mock_server = mock_server!(StatusCode::OK);
        let config = fixtures_path().join("configs").join("smoketest.toml");
        let mut cmd = main_command();
        cmd.arg("--config")
            .arg(config)
            .arg("-")
            .write_stdin(mock_server.uri())
            .env_clear()
            .assert()
            .success();
    }

    #[tokio::test]
    async fn test_config_accept() {
        let mock_server = mock_server!(StatusCode::OK);
        let config = fixtures_path().join("configs").join("accept.toml");
        let mut cmd = main_command();
        cmd.arg("--config")
            .arg(config)
            .arg("-")
            .write_stdin(mock_server.uri())
            .env_clear()
            .assert()
            .success();
    }

    #[test]
    fn test_lycheeignore_file() -> Result<()> {
        let mut cmd = main_command();
        let test_path = fixtures_path().join("lycheeignore");

        let cmd = cmd
            .current_dir(test_path)
            .arg("--dump")
            .arg("TEST.md")
            .assert()
            .stdout(contains("https://example.com"))
            .stdout(contains("https://example.com/bar"))
            .stdout(contains("https://example.net"));

        let output = cmd.get_output();
        let output = std::str::from_utf8(&output.stdout).unwrap();
        assert_eq!(output.lines().count(), 3);

        Ok(())
    }

    #[test]
    fn test_lycheeignore_and_exclude_file() -> Result<()> {
        let mut cmd = main_command();
        let test_path = fixtures_path().join("lycheeignore");
        let excludes_path = test_path.join("normal-exclude-file");

        cmd.current_dir(test_path)
            .arg("TEST.md")
            .arg("--exclude-file")
            .arg(excludes_path)
            .assert()
            .success()
            .stdout(contains("8 Total"))
            .stdout(contains("6 Excluded"));

        Ok(())
    }

    #[tokio::test]
    async fn test_lycheecache_file() -> Result<()> {
        let base_path = fixtures_path().join("cache");
        let cache_file = base_path.join(LYCHEE_CACHE_FILE);

        // Ensure clean state
        if cache_file.exists() {
            println!("Removing cache file before test: {cache_file:?}");
            fs::remove_file(&cache_file)?;
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // Setup mock servers
        let mock_server_ok = mock_server!(StatusCode::OK);
        let mock_server_err = mock_server!(StatusCode::NOT_FOUND);
        let mock_server_exclude = mock_server!(StatusCode::OK);

        // Create test file
        let dir = tempfile::tempdir()?;
        let file_path = dir.path().join("c.md");
        let mut file = File::create(&file_path)?;
        writeln!(file, "{}", mock_server_ok.uri().as_str())?;
        writeln!(file, "{}", mock_server_err.uri().as_str())?;
        writeln!(file, "{}", mock_server_exclude.uri().as_str())?;
        file.sync_all()?;

        // Create and run command
        let mut cmd = main_command();
        cmd.current_dir(&base_path)
            .arg(&file_path)
            .arg("--verbose")
            .arg("--no-progress")
            .arg("--cache")
            .arg("--exclude")
            .arg(mock_server_exclude.uri());

        // Note: Don't check output.status.success() since we expect
        // a non-zero exit code (2) when lychee finds broken links
        let _output = cmd.output()?;

        // Wait for cache file to be written
        for _ in 0..20 {
            if cache_file.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Check cache contents
        let data = fs::read_to_string(&cache_file)?;
        println!("Cache file contents: {data}");

        assert!(
            data.contains(&format!("{}/,200", mock_server_ok.uri())),
            "Missing OK entry in cache"
        );
        assert!(
            data.contains(&format!("{}/,404", mock_server_err.uri())),
            "Missing error entry in cache"
        );

        // Run again to verify cache behavior
        cmd.assert()
            .stderr(contains(format!(
                "[200] {}/ | OK (cached)\n",
                mock_server_ok.uri()
            )))
            .stderr(contains(format!(
                "[404] {}/ | Error (cached)\n",
                mock_server_err.uri()
            )));

        // Clean up
        fs::remove_file(&cache_file).map_err(|e| {
            anyhow::anyhow!(
                "Failed to remove cache file: {:?}, error: {}",
                cache_file,
                e
            )
        })?;

        Ok(())
    }

    #[tokio::test]
    async fn test_lycheecache_exclude_custom_status_codes() -> Result<()> {
        let base_path = fixtures_path().join("cache");
        let cache_file = base_path.join(LYCHEE_CACHE_FILE);

        // Unconditionally remove cache file if it exists
        let _ = fs::remove_file(&cache_file);

        let mock_server_ok = mock_server!(StatusCode::OK);
        let mock_server_no_content = mock_server!(StatusCode::NO_CONTENT);
        let mock_server_too_many_requests = mock_server!(StatusCode::TOO_MANY_REQUESTS);

        let dir = tempfile::tempdir()?;
        let mut file = File::create(dir.path().join("c.md"))?;

        writeln!(file, "{}", mock_server_ok.uri().as_str())?;
        writeln!(file, "{}", mock_server_no_content.uri().as_str())?;
        writeln!(file, "{}", mock_server_too_many_requests.uri().as_str())?;

        let mut cmd = main_command();
        let test_cmd = cmd
            .current_dir(&base_path)
            .arg(dir.path().join("c.md"))
            .arg("--verbose")
            .arg("--no-progress")
            .arg("--cache")
            .arg("--cache-exclude-status")
            .arg("204,429");

        assert!(
            !cache_file.exists(),
            "cache file should not exist before this test"
        );

        // Run first without cache to generate the cache file
        test_cmd
            .assert()
            .stderr(contains(format!("[200] {}/\n", mock_server_ok.uri())))
            .stderr(contains(format!(
                "[204] {}/ | 204 No Content: No Content\n",
                mock_server_no_content.uri()
            )))
            .stderr(contains(format!(
                "[429] {}/ | Rejected status code (this depends on your \"accept\" configuration): Too Many Requests\n",
                mock_server_too_many_requests.uri()
            )));

        // Check content of cache file
        let data = fs::read_to_string(&cache_file)?;

        if data.is_empty() {
            println!("Cache file is empty!");
        }

        assert!(data.contains(&format!("{}/,200", mock_server_ok.uri())));
        assert!(!data.contains(&format!("{}/,204", mock_server_no_content.uri())));
        assert!(!data.contains(&format!("{}/,429", mock_server_too_many_requests.uri())));

        // Unconditionally remove the cache file
        let _ = fs::remove_file(&cache_file);
        Ok(())
    }

    #[tokio::test]
    async fn test_lycheecache_accept_custom_status_codes() -> Result<()> {
        let base_path = fixtures_path().join("cache_accept_custom_status_codes");
        let cache_file = base_path.join(LYCHEE_CACHE_FILE);

        // Unconditionally remove cache file if it exists
        let _ = fs::remove_file(&cache_file);

        let mock_server_ok = mock_server!(StatusCode::OK);
        let mock_server_teapot = mock_server!(StatusCode::IM_A_TEAPOT);
        let mock_server_server_error = mock_server!(StatusCode::INTERNAL_SERVER_ERROR);

        let dir = tempfile::tempdir()?;
        let mut file = File::create(dir.path().join("c.md"))?;

        writeln!(file, "{}", mock_server_ok.uri().as_str())?;
        writeln!(file, "{}", mock_server_teapot.uri().as_str())?;
        writeln!(file, "{}", mock_server_server_error.uri().as_str())?;

        let mut cmd = main_command();
        let test_cmd = cmd
            .current_dir(&base_path)
            .arg(dir.path().join("c.md"))
            .arg("--verbose")
            .arg("--cache");

        assert!(
            !cache_file.exists(),
            "cache file should not exist before this test"
        );

        // run first without cache to generate the cache file
        // ignore exit code
        test_cmd
            .assert()
            .failure()
            .code(2)
            .stdout(contains(format!(
                r#"[418] {}/ | Rejected status code (this depends on your "accept" configuration): I'm a teapot"#,
                mock_server_teapot.uri()
            )))
            .stdout(contains(format!(
                r#"[500] {}/ | Rejected status code (this depends on your "accept" configuration): Internal Server Error"#,
                mock_server_server_error.uri()
            )));

        // check content of cache file
        let data = fs::read_to_string(&cache_file)?;
        assert!(data.contains(&format!("{}/,200", mock_server_ok.uri())));
        assert!(data.contains(&format!("{}/,418", mock_server_teapot.uri())));
        assert!(data.contains(&format!("{}/,500", mock_server_server_error.uri())));

        // run again to verify cache behavior
        // this time accept 418 and 500 as valid status codes
        test_cmd
            .arg("--no-progress")
            .arg("--accept")
            .arg("418,500")
            .assert()
            .success()
            .stderr(contains(format!(
                "[418] {}/ | OK (cached)",
                mock_server_teapot.uri()
            )))
            .stderr(contains(format!(
                "[500] {}/ | OK (cached)",
                mock_server_server_error.uri()
            )));

        // clear the cache file
        fs::remove_file(&cache_file)?;

        Ok(())
    }

    #[tokio::test]
    async fn test_accept_overrides_defaults_not_additive() -> Result<()> {
        let mock_server_200 = mock_server!(StatusCode::OK);

        let mut cmd = main_command();
        cmd.arg("--accept")
            .arg("404") // ONLY accept 404 - should reject 200 as we overwrite the default
            .arg("-")
            .write_stdin(mock_server_200.uri())
            .assert()
            .failure()
            .code(2)
            .stdout(contains(format!(
                r#"[200] {}/ | Rejected status code (this depends on your "accept" configuration): OK"#,
                mock_server_200.uri()
            )));

        Ok(())
    }

    #[tokio::test]
    async fn test_skip_cache_unsupported() -> Result<()> {
        let base_path = fixtures_path().join("cache");
        let cache_file = base_path.join(LYCHEE_CACHE_FILE);

        // Unconditionally remove cache file if it exists
        let _ = fs::remove_file(&cache_file);

        let unsupported_url = "slack://user".to_string();
        let excluded_url = "https://example.com/";

        // run first without cache to generate the cache file
        main_command()
            .current_dir(&base_path)
            .write_stdin(format!("{unsupported_url}\n{excluded_url}"))
            .arg("--cache")
            .arg("--verbose")
            .arg("--no-progress")
            .arg("--exclude")
            .arg(excluded_url)
            .arg("--")
            .arg("-")
            .assert()
            .stderr(contains(format!(
                "[IGNORED] {unsupported_url} | Unsupported: Error creating request client"
            )))
            .stderr(contains(format!("[EXCLUDED] {excluded_url}\n")));

        // The cache file should be empty, because the only checked URL is
        // unsupported and we don't want to cache that. It might be supported in
        // future versions.
        let buf = fs::read(&cache_file).unwrap();
        assert!(buf.is_empty());

        // clear the cache file
        fs::remove_file(&cache_file)?;

        Ok(())
    }

    /// Unknown status codes should be skipped and not cached by default
    /// The reason is that we don't know if they are valid or not
    /// and even if they are invalid, we don't know if they will be valid in the
    /// future.
    ///
    /// Since we cannot test this with our mock server (because hyper panics on
    /// invalid status codes) we use LinkedIn as a test target.
    ///
    /// Unfortunately, LinkedIn does not always return 999, so this is a flaky
    /// test. We only check that the cache file doesn't contain any invalid
    /// status codes.
    #[tokio::test]
    async fn test_skip_cache_unknown_status_code() -> Result<()> {
        let base_path = fixtures_path().join("cache");
        let cache_file = base_path.join(LYCHEE_CACHE_FILE);

        // Unconditionally remove cache file if it exists
        let _ = fs::remove_file(&cache_file);

        // https://linkedin.com returns 999 for unknown status codes
        // use this as a test target
        let unknown_url = "https://www.linkedin.com/company/corrode";

        // run first without cache to generate the cache file
        main_command()
            .current_dir(&base_path)
            .write_stdin(unknown_url.to_string())
            .arg("--cache")
            .arg("--verbose")
            .arg("--no-progress")
            .arg("--")
            .arg("-")
            .assert()
            // LinkedIn does not always return 999, so we cannot check for that
            // .stderr(contains(format!("[999] {unknown_url} | Unknown status")))
            ;

        // If the status code was 999, the cache file should be empty
        // because we do not want to cache unknown status codes
        let buf = fs::read(&cache_file).unwrap();
        if !buf.is_empty() {
            let data = String::from_utf8(buf)?;
            // The cache file should not contain any invalid status codes
            // In that case, we expect a single entry with status code 200
            assert!(!data.contains("999"));
            assert!(data.contains("200"));
        }

        // clear the cache file
        fs::remove_file(&cache_file)?;

        Ok(())
    }

    #[test]
    fn test_verbatim_skipped_by_default() -> Result<()> {
        let mut cmd = main_command();
        let input = fixtures_path().join("TEST_CODE_BLOCKS.md");

        cmd.arg(input)
            .arg("--dump")
            .assert()
            .success()
            .stdout(is_empty());

        Ok(())
    }

    #[test]
    fn test_include_verbatim() -> Result<()> {
        let mut cmd = main_command();
        let input = fixtures_path().join("TEST_CODE_BLOCKS.md");

        cmd.arg("--include-verbatim")
            .arg(input)
            .arg("--dump")
            .assert()
            .success()
            .stdout(contains("http://127.0.0.1/block"))
            .stdout(contains("http://127.0.0.1/inline"))
            .stdout(contains("http://127.0.0.1/bash"));

        Ok(())
    }
    #[tokio::test]
    async fn test_verbatim_skipped_by_default_via_file() -> Result<()> {
        let file = fixtures_path().join("TEST_VERBATIM.html");

        main_command()
            .arg("--dump")
            .arg(file)
            .assert()
            .success()
            .stdout(is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_verbatim_skipped_by_default_via_remote_url() -> Result<()> {
        let mut cmd = main_command();
        let file = fixtures_path().join("TEST_VERBATIM.html");
        let body = fs::read_to_string(file)?;
        let mock_server = mock_response!(body);

        cmd.arg("--dump")
            .arg(mock_server.uri())
            .assert()
            .success()
            .stdout(is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_include_verbatim_via_remote_url() -> Result<()> {
        let mut cmd = main_command();
        let file = fixtures_path().join("TEST_VERBATIM.html");
        let body = fs::read_to_string(file)?;
        let mock_server = mock_response!(body);

        cmd.arg("--include-verbatim")
            .arg("--dump")
            .arg(mock_server.uri())
            .assert()
            .success()
            .stdout(contains("http://www.example.com/pre"))
            .stdout(contains("http://www.example.com/code"))
            .stdout(contains("http://www.example.com/samp"))
            .stdout(contains("http://www.example.com/kbd"))
            .stdout(contains("http://www.example.com/var"))
            .stdout(contains("http://www.example.com/script"));
        Ok(())
    }

    #[test]
    fn test_require_https() -> Result<()> {
        let mut cmd = main_command();
        let test_path = fixtures_path().join("TEST_HTTP.html");
        cmd.arg(&test_path).assert().success();

        let mut cmd = main_command();
        cmd.arg("--require-https").arg(test_path).assert().failure();

        Ok(())
    }

    /// If `base-dir` is not set, don't throw an error in case we encounter
    /// an absolute local link (e.g. `/about`) within a file.
    /// Instead, simply ignore the link.
    #[test]
    fn test_ignore_absolute_local_links_without_base() -> Result<()> {
        let mut cmd = main_command();

        let offline_dir = fixtures_path().join("offline");

        cmd.arg("--offline")
            .arg(offline_dir.join("index.html"))
            .env_clear()
            .assert()
            .success()
            .stdout(contains("0 Total"));

        Ok(())
    }

    #[test]
    fn test_inputs_without_scheme() -> Result<()> {
        let test_path = fixtures_path().join("TEST_HTTP.html");
        let mut cmd = main_command();

        cmd.arg("--dump")
            .arg("example.com")
            .arg(&test_path)
            .arg("https://example.org")
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn test_print_excluded_links_in_verbose_mode() -> Result<()> {
        let test_path = fixtures_path().join("TEST_DUMP_EXCLUDE.txt");
        let mut cmd = main_command();

        cmd.arg("--dump")
            .arg("--verbose")
            .arg("--exclude")
            .arg("example.com")
            .arg("--")
            .arg(&test_path)
            .assert()
            .success()
            .stdout(contains(format!(
                "https://example.com/ ({}) [excluded]",
                test_path.display()
            )))
            .stdout(contains(format!(
                "https://example.org/ ({})",
                test_path.display()
            )))
            .stdout(contains(format!(
                "https://example.com/foo/bar ({}) [excluded]",
                test_path.display()
            )));
        Ok(())
    }

    #[test]
    fn test_remap_uri() -> Result<()> {
        let mut cmd = main_command();

        cmd.arg("--dump")
            .arg("--remap")
            .arg("https://example.com http://127.0.0.1:8080")
            .arg("--remap")
            .arg("https://example.org https://staging.example.com")
            .arg("--")
            .arg("-")
            .write_stdin("https://example.com\nhttps://example.org\nhttps://example.net\n")
            .env_clear()
            .assert()
            .success()
            .stdout(contains("http://127.0.0.1:8080/"))
            .stdout(contains("https://staging.example.com/"))
            .stdout(contains("https://example.net/"));

        Ok(())
    }

    #[test]
    #[ignore = "Skipping test until https://github.com/robinst/linkify/pull/58 is merged"]
    fn test_remap_path() -> Result<()> {
        let mut cmd = main_command();

        cmd.arg("--dump")
            .arg("--remap")
            .arg("../../issues https://github.com/usnistgov/OSCAL/issues")
            .arg("--")
            .arg("-")
            .write_stdin("../../issues\n")
            .env_clear()
            .assert()
            .success()
            .stdout(contains("https://github.com/usnistgov/OSCAL/issues"));

        Ok(())
    }

    #[test]
    fn test_remap_capture() -> Result<()> {
        let mut cmd = main_command();

        cmd.arg("--dump")
            .arg("--remap")
            .arg("https://example.com/(.*) http://example.org/$1")
            .arg("--")
            .arg("-")
            .write_stdin("https://example.com/foo\n")
            .env_clear()
            .assert()
            .success()
            .stdout(contains("http://example.org/foo"));

        Ok(())
    }

    #[test]
    fn test_remap_named_capture() -> Result<()> {
        let mut cmd = main_command();

        cmd.arg("--dump")
            .arg("--remap")
            .arg("https://github.com/(?P<org>.*)/(?P<repo>.*) https://gitlab.com/$org/$repo")
            .arg("--")
            .arg("-")
            .write_stdin("https://github.com/lycheeverse/lychee\n")
            .env_clear()
            .assert()
            .success()
            .stdout(contains("https://gitlab.com/lycheeverse/lychee"));

        Ok(())
    }

    #[test]
    fn test_excluded_paths_regex() -> Result<()> {
        let test_path = fixtures_path().join("exclude-path");
        let excluded_path_1 = "\\/excluded?\\/"; // exclude paths containing a directory "exclude" and "excluded"
        let excluded_path_2 = "(\\.mdx|\\.txt)$"; // exclude .mdx and .txt files
        let mut cmd = main_command();

        cmd.arg("--exclude-path")
            .arg(excluded_path_1)
            .arg("--exclude-path")
            .arg(excluded_path_2)
            .arg("--dump")
            .arg("--")
            .arg(&test_path)
            .assert()
            .success()
            .stdout(eq(
                "https://test.md/to-be-included-inner\nhttps://test.md/to-be-included-outer\n",
            ));

        Ok(())
    }

    #[test]
    fn test_handle_relative_paths_as_input() -> Result<()> {
        let test_path = fixtures_path();
        let mut cmd = main_command();

        cmd.current_dir(&test_path)
            .arg("--verbose")
            .arg("--exclude")
            .arg("example.*")
            .arg("--")
            .arg("./TEST_DUMP_EXCLUDE.txt")
            .assert()
            .success()
            .stdout(contains("3 Total"))
            .stdout(contains("3 Excluded"));

        Ok(())
    }

    #[test]
    fn test_handle_nonexistent_relative_paths_as_input() -> Result<()> {
        let test_path = fixtures_path();
        let mut cmd = main_command();

        cmd.current_dir(&test_path)
            .arg("--verbose")
            .arg("--exclude")
            .arg("example.*")
            .arg("--")
            .arg("./NOT-A-REAL-TEST-FIXTURE.md")
            .assert()
            .failure()
            .stderr(contains("Invalid file path: ./NOT-A-REAL-TEST-FIXTURE.md"));

        Ok(())
    }

    #[test]
    fn test_prevent_too_many_redirects() -> Result<()> {
        let mut cmd = main_command();
        let url = "https://http.codes/308";

        cmd.write_stdin(url)
            .arg("--max-redirects")
            .arg("0")
            .arg("-")
            .assert()
            .failure();

        Ok(())
    }

    #[test]
    #[ignore = "Skipping test because it is flaky"]
    fn test_suggests_url_alternatives() -> Result<()> {
        let re = Regex::new(r"http://web\.archive\.org/web/.*google\.com/jobs\.html").unwrap();

        for _ in 0..3 {
            // This can be flaky. Try up to 3 times
            let mut cmd = main_command();
            let input = fixtures_path().join("INTERNET_ARCHIVE.md");

            cmd.arg("--no-progress").arg("--suggest").arg(input);

            // Run he command and check if the output contains the expected
            // suggestions
            let assert = cmd.assert();
            let output = assert.get_output();

            // We're looking for a suggestion that
            // - starts with http://web.archive.org/web/
            // - ends with google.com/jobs.html
            if re.is_match(&String::from_utf8_lossy(&output.stdout)) {
                // Test passed
                return Ok(());
            } else {
                // Wait for a second before retrying
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }

        // If we reached here, it means the test did not pass after multiple attempts
        Err("Did not get the expected command output after multiple attempts.".into())
    }

    #[tokio::test]
    async fn test_basic_auth() -> Result<()> {
        let username = "username";
        let password = "password123";

        let mock_server = wiremock::MockServer::start().await;

        Mock::given(method("GET"))
            .and(basic_auth(username, password))
            .respond_with(ResponseTemplate::new(200)) // Authenticated requests are accepted
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .respond_with(|_: &_| panic!("Received unauthenticated request"))
            .mount(&mock_server)
            .await;

        // Configure the command to use the BasicAuthExtractor
        main_command()
            .arg("--verbose")
            .arg("--basic-auth")
            .arg(format!("{} {username}:{password}", mock_server.uri()))
            .arg("-")
            .write_stdin(mock_server.uri())
            .assert()
            .success()
            .stdout(contains("1 Total"))
            .stdout(contains("1 OK"));

        // Websites as direct arguments must also use authentication
        main_command()
            .arg(mock_server.uri())
            .arg("--verbose")
            .arg("--basic-auth")
            .arg(format!("{} {username}:{password}", mock_server.uri()))
            .assert()
            .success()
            .stdout(contains("0 Total")); // Mock server returns no body, so there are no URLs to check

        Ok(())
    }

    #[tokio::test]
    async fn test_multi_basic_auth() -> Result<()> {
        let username1 = "username";
        let password1 = "password123";
        let mock_server1 = wiremock::MockServer::start().await;
        Mock::given(basic_auth(username1, password1))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server1)
            .await;

        let username2 = "admin_user";
        let password2 = "admin_pw";
        let mock_server2 = wiremock::MockServer::start().await;

        Mock::given(basic_auth(username2, password2))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server2)
            .await;

        // Configure the command to use the BasicAuthExtractor
        main_command()
            .arg("--verbose")
            .arg("--basic-auth")
            .arg(format!("{} {username1}:{password1}", mock_server1.uri()))
            .arg("--basic-auth")
            .arg(format!("{} {username2}:{password2}", mock_server2.uri()))
            .arg("-")
            .write_stdin(format!("{}\n{}", mock_server1.uri(), mock_server2.uri()))
            .assert()
            .success()
            .stdout(contains("2 Total"))
            .stdout(contains("2 OK"));

        Ok(())
    }

    #[tokio::test]
    async fn test_cookie_jar() -> Result<()> {
        // Create a random cookie jar file
        let cookie_jar = NamedTempFile::new()?;
        let mut cmd = main_command();
        cmd.arg("--cookie-jar")
            .arg(cookie_jar.path().to_str().unwrap())
            .arg("-")
            // Using Google as a test target because I couldn't
            // get the mock server to work with the cookie jar
            .write_stdin("https://google.com/ncr")
            .assert()
            .success();

        // check that the cookie jar file contains the expected cookies
        let file = std::fs::File::open(cookie_jar.path()).map(std::io::BufReader::new)?;
        let cookie_store = cookie_store::serde::json::load(file)
            .map_err(|e| anyhow!("Failed to load cookie jar: {e}"))?;
        let all_cookies = cookie_store.iter_any().collect::<Vec<_>>();
        assert!(!all_cookies.is_empty());
        assert!(all_cookies.iter().all(|c| c.domain() == Some("google.com")));
        Ok(())
    }
    #[test]
    fn test_dump_inputs_glob_md() -> Result<()> {
        let pattern = fixtures_path().join("**/*.md");

        let mut cmd = main_command();
        cmd.arg("--dump-inputs")
            .arg(pattern)
            .assert()
            .success()
            .stdout(contains("fixtures/dump_inputs/subfolder/file2.md"))
            .stdout(contains("fixtures/dump_inputs/markdown.md"));

        Ok(())
    }

    #[test]
    fn test_dump_inputs_glob_all() -> Result<()> {
        let pattern = fixtures_path().join("**/*");

        let mut cmd = main_command();
        cmd.arg("--dump-inputs")
            .arg(pattern)
            .assert()
            .success()
            .stdout(contains("fixtures/dump_inputs/subfolder/test.html"))
            .stdout(contains("fixtures/dump_inputs/subfolder/file2.md"))
            .stdout(contains("fixtures/dump_inputs/subfolder"))
            .stdout(contains("fixtures/dump_inputs/markdown.md"))
            .stdout(contains("fixtures/dump_inputs/subfolder/example.bin"))
            .stdout(contains("fixtures/dump_inputs/some_file.txt"));

        Ok(())
    }

    #[test]
    fn test_dump_inputs_glob_exclude_path() -> Result<()> {
        let pattern = fixtures_path().join("**/*");

        let mut cmd = main_command();
        cmd.arg("--dump-inputs")
            .arg(pattern)
            .arg("--exclude-path")
            .arg(fixtures_path().join("dump_inputs/subfolder"))
            .assert()
            .success()
            .stdout(contains("fixtures/dump_inputs/subfolder/test.html").not())
            .stdout(contains("fixtures/dump_inputs/subfolder/file2.md").not())
            .stdout(contains("fixtures/dump_inputs/subfolder").not());

        Ok(())
    }

    #[test]
    fn test_dump_inputs_url() -> Result<()> {
        let mut cmd = main_command();
        cmd.arg("--dump-inputs")
            .arg("https://example.com")
            .assert()
            .success()
            .stdout(contains("https://example.com"));

        Ok(())
    }

    #[test]
    fn test_dump_inputs_path() -> Result<()> {
        let mut cmd = main_command();
        cmd.arg("--dump-inputs")
            .arg("fixtures")
            .assert()
            .success()
            .stdout(contains("fixtures"));

        Ok(())
    }

    #[test]
    fn test_dump_inputs_stdin() -> Result<()> {
        let mut cmd = main_command();
        cmd.arg("--dump-inputs")
            .arg("-")
            .assert()
            .success()
            .stdout(contains("Stdin"));

        Ok(())
    }

    #[test]
    fn test_fragments() {
        let mut cmd = main_command();
        let input = fixtures_path().join("fragments");

        let mut result = cmd
            .arg("--include-fragments")
            .arg("--verbose")
            .arg(input)
            .assert()
            .failure();

        let expected_successes = vec![
            "fixtures/fragments/empty_dir",
            "fixtures/fragments/empty_file#fragment", // XXX: is this a bug? a fragment in an empty file is being treated as valid
            "fixtures/fragments/file1.md#code-heading",
            "fixtures/fragments/file1.md#explicit-fragment",
            "fixtures/fragments/file1.md#f%C3%BCnf-s%C3%9C%C3%9Fe-%C3%84pfel",
            "fixtures/fragments/file1.md#f%C3%BCnf-s%C3%BC%C3%9Fe-%C3%A4pfel",
            "fixtures/fragments/file1.md#fragment-1",
            "fixtures/fragments/file1.md#fragment-2",
            "fixtures/fragments/file1.md#IGNORE-CASING",
            "fixtures/fragments/file1.md#kebab-case-fragment",
            "fixtures/fragments/file1.md#kebab-case-fragment-1",
            "fixtures/fragments/file1.md#lets-wear-a-hat-%C3%AAtre",
            "fixtures/fragments/file2.md#",
            "fixtures/fragments/file2.md#custom-id",
            "fixtures/fragments/file2.md#fragment-1",
            "fixtures/fragments/file2.md#top",
            "fixtures/fragments/file.html#",
            "fixtures/fragments/file.html#a-word",
            "fixtures/fragments/file.html#in-the-beginning",
            "fixtures/fragments/file.html#tangent%3A-kustomize",
            "fixtures/fragments/file.html#top",
            "fixtures/fragments/file.html#Upper-%C3%84%C3%96%C3%B6",
            "fixtures/fragments/sub_dir",
            "fixtures/fragments/sub_dir#a-link-inside-index-html-inside-sub-dir",
            "fixtures/fragments/zero.bin",
            "fixtures/fragments/zero.bin#",
            "fixtures/fragments/zero.bin#fragment",
            "https://github.com/lycheeverse/lychee#table-of-contents",
            "https://raw.githubusercontent.com/lycheeverse/lychee/master/fixtures/fragments/zero.bin",
            "https://raw.githubusercontent.com/lycheeverse/lychee/master/fixtures/fragments/zero.bin#",
            // zero.bin#fragment succeeds because fragment checking is skipped for this URL
            "https://raw.githubusercontent.com/lycheeverse/lychee/master/fixtures/fragments/zero.bin#fragment",
        ];

        let expected_failures = vec![
            "fixtures/fragments/sub_dir_non_existing_1",
            "fixtures/fragments/sub_dir#non-existing-fragment-2",
            "fixtures/fragments/empty_dir#non-existing-fragment-3",
            "fixtures/fragments/file2.md#missing-fragment",
            "fixtures/fragments/sub_dir#non-existing-fragment-1",
            "fixtures/fragments/sub_dir_non_existing_2",
            "fixtures/fragments/file1.md#missing-fragment",
            "fixtures/fragments/empty_dir#non-existing-fragment-4",
            "fixtures/fragments/file.html#in-the-end",
            "fixtures/fragments/file.html#in-THE-begiNNing",
            "https://github.com/lycheeverse/lychee#non-existent-anchor",
        ];

        // the stdout/stderr format looks like this:
        //
        //     [ERROR] https://github.com/lycheeverse/lychee#non-existent-anchor | Cannot find fragment
        //     [200] file:///home/rina/progs/lychee/fixtures/fragments/file.html#a-word
        //
        // errors are printed to both, but 200s are printed to stderr only.
        // we take advantage of this to ensure that good URLs do not appear
        // in stdout, and bad URLs do appear in stdout.
        //
        // also, a space or newline is appended to the URL to prevent
        // incorrect matches where one URL is a prefix of another.
        for good_url in &expected_successes {
            // additionally checks that URL is within stderr to ensure that
            // the URL is detected by lychee.
            result = result
                .stdout(contains(format!("{good_url} ")).not())
                .stderr(contains(format!("{good_url}\n")));
        }
        for bad_url in &expected_failures {
            result = result.stdout(contains(format!("{bad_url} ")));
        }

        let ok_num = expected_successes.len();
        let err_num = expected_failures.len();
        let total_num = ok_num + err_num;
        result
            .stdout(contains(format!("{ok_num} OK")))
            // Failures because of missing fragments or failed binary body scan
            .stdout(contains(format!("{err_num} Errors")))
            .stdout(contains(format!("{total_num} Total")));
    }

    #[test]
    fn test_fragments_when_accept_error_status_codes() {
        let mut cmd = main_command();
        let input = fixtures_path().join("TEST_FRAGMENT_ERR_CODE.md");

        // it's common for user to accept 429, but let's test with 404 since
        // triggering 429 may annoy the server
        cmd.arg("--verbose")
            .arg("--accept=200,404")
            .arg("--include-fragments")
            .arg(input)
            .assert()
            .success()
            .stderr(contains(
                "https://en.wikipedia.org/wiki/Should404#ignore-fragment",
            ))
            .stdout(contains("0 Errors"))
            .stdout(contains("1 OK"))
            .stdout(contains("1 Total"));
    }

    #[test]
    fn test_fallback_extensions() {
        let mut cmd = main_command();
        let input = fixtures_path().join("fallback-extensions");

        cmd.arg("--verbose")
            .arg("--fallback-extensions=htm,html")
            .arg(input)
            .assert()
            .success()
            .stdout(contains("0 Errors"));
    }

    #[test]
    fn test_fragments_fallback_extensions() {
        let mut cmd = main_command();
        let input = fixtures_path().join("fragments-fallback-extensions");

        cmd.arg("--include-fragments")
            .arg("--fallback-extensions=html")
            .arg("--no-progress")
            .arg("--offline")
            .arg("-v")
            .arg(input)
            .assert()
            .failure()
            .stdout(contains("3 Total"))
            .stdout(contains("1 OK"))
            .stdout(contains("2 Errors"));
    }

    /// Test relative paths
    ///
    /// Imagine a web server hosting a site with the following structure:
    /// root
    /// └── test
    ///     ├── index.html
    ///     └── next.html
    ///
    /// where `root/test/index.html` contains `<a href="next.html">next</a>`
    /// When checking the link in `root/test/index.html` we should be able to
    /// resolve the relative path to `root/test/next.html`
    ///
    /// Note that the relative path is not resolved to the root of the server
    /// but relative to the file that contains the link.
    #[tokio::test]
    async fn test_resolve_relative_paths_in_subfolder() -> Result<()> {
        let mock_server = wiremock::MockServer::start().await;

        let body = r#"<a href="next.html">next</a>"#;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/test/index.html"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(body))
            .mount(&mock_server)
            .await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/test/next.html"))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let mut cmd = main_command();
        cmd.arg("--verbose")
            .arg(format!("{}/test/index.html", mock_server.uri()))
            .assert()
            .success()
            .stdout(contains("1 Total"))
            .stdout(contains("0 Errors"));

        Ok(())
    }

    #[tokio::test]
    async fn test_json_format_in_config() -> Result<()> {
        let mock_server = mock_server!(StatusCode::OK);
        let config = fixtures_path().join("configs").join("format.toml");
        let mut cmd = main_command();
        cmd.arg("--config")
            .arg(config)
            .arg("-")
            .write_stdin(mock_server.uri())
            .env_clear()
            .assert()
            .success();

        // Check that the output is in JSON format
        let output = cmd.output().unwrap();
        let output = std::str::from_utf8(&output.stdout).unwrap();
        let json: serde_json::Value = serde_json::from_str(output)?;
        assert_eq!(json["total"], 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_retry() -> Result<()> {
        let mock_server = wiremock::MockServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(ResponseTemplate::new(429))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let mut cmd = main_command();
        cmd.arg("-")
            .write_stdin(mock_server.uri())
            .assert()
            .success();

        Ok(())
    }

    #[tokio::test]
    async fn test_no_header_set_on_input() -> Result<()> {
        let mut cmd = main_command();
        let server = wiremock::MockServer::start().await;
        server
            .register(
                wiremock::Mock::given(wiremock::matchers::method("GET"))
                    .respond_with(wiremock::ResponseTemplate::new(200))
                    .expect(1),
            )
            .await;

        cmd.arg("--verbose").arg(server.uri()).assert().success();

        let received_requests = server.received_requests().await.unwrap();
        assert_eq!(received_requests.len(), 1);

        let received_request = &received_requests[0];
        assert_eq!(received_request.method, Method::GET);
        assert_eq!(received_request.url.path(), "/");

        // Make sure the request does not contain the custom header
        assert!(!received_request.headers.contains_key("X-Foo"));
        Ok(())
    }

    #[tokio::test]
    async fn test_header_set_on_input() -> Result<()> {
        let mut cmd = main_command();
        let server = wiremock::MockServer::start().await;
        server
            .register(
                wiremock::Mock::given(wiremock::matchers::method("GET"))
                    .and(wiremock::matchers::header("X-Foo", "Bar"))
                    .respond_with(wiremock::ResponseTemplate::new(200))
                    // We expect the mock to be called exactly least once.
                    .expect(1)
                    .named("GET expecting custom header"),
            )
            .await;

        cmd.arg("--verbose")
            .arg("--header")
            .arg("X-Foo: Bar")
            .arg(server.uri())
            .assert()
            .success();

        // Check that the server received the request with the header
        server.verify().await;
        Ok(())
    }

    #[tokio::test]
    async fn test_multi_header_set_on_input() -> Result<()> {
        let mut cmd = main_command();
        let server = wiremock::MockServer::start().await;
        server
            .register(
                wiremock::Mock::given(wiremock::matchers::method("GET"))
                    .and(wiremock::matchers::header("X-Foo", "Bar"))
                    .and(wiremock::matchers::header("X-Bar", "Baz"))
                    .respond_with(wiremock::ResponseTemplate::new(200))
                    // We expect the mock to be called exactly least once.
                    .expect(1)
                    .named("GET expecting custom header"),
            )
            .await;

        cmd.arg("--verbose")
            .arg("--header")
            .arg("X-Foo: Bar")
            .arg("--header")
            .arg("X-Bar: Baz")
            .arg(server.uri())
            .assert()
            .success();

        // Check that the server received the request with the header
        server.verify().await;
        Ok(())
    }

    #[tokio::test]
    async fn test_header_set_in_config() -> Result<()> {
        let mut cmd = main_command();
        let server = wiremock::MockServer::start().await;
        server
            .register(
                wiremock::Mock::given(wiremock::matchers::method("GET"))
                    .and(wiremock::matchers::header("X-Foo", "Bar"))
                    .and(wiremock::matchers::header("X-Bar", "Baz"))
                    .respond_with(wiremock::ResponseTemplate::new(200))
                    // We expect the mock to be called exactly least once.
                    .expect(1)
                    .named("GET expecting custom header"),
            )
            .await;

        let config = fixtures_path().join("configs").join("headers.toml");
        cmd.arg("--verbose")
            .arg("--config")
            .arg(config)
            .arg(server.uri())
            .assert()
            .success();

        // Check that the server received the request with the header
        server.verify().await;
        Ok(())
    }

    #[test]
    fn test_sorted_error_output() -> Result<()> {
        let test_files = ["TEST_GITHUB_404.md", "TEST_INVALID_URLS.html"];

        let test_urls = [
            "https://httpbin.org/status/404",
            "https://httpbin.org/status/500",
            "https://httpbin.org/status/502",
        ];

        let cmd = &mut main_command()
            .arg("--format")
            .arg("compact")
            .arg(fixtures_path().join(test_files[1]))
            .arg(fixtures_path().join(test_files[0]))
            .assert()
            .failure()
            .code(2);

        let output = String::from_utf8_lossy(&cmd.get_output().stdout);
        let mut position: usize = 0;

        // Check that the input sources are sorted
        for file in test_files {
            assert!(output.contains(file));

            let next_position = output.find(file).unwrap();

            assert!(next_position > position);
            position = next_position;
        }

        position = 0;

        // Check that the responses are sorted
        for url in test_urls {
            assert!(output.contains(url));

            let next_position = output.find(url).unwrap();

            assert!(next_position > position);
            position = next_position;
        }

        Ok(())
    }

    #[test]
    fn test_extract_url_ending_with_period_file() {
        let test_path = fixtures_path().join("LINK_PERIOD.html");

        let mut cmd = main_command();
        cmd.arg("--dump")
            .arg(test_path)
            .assert()
            .success()
            .stdout(contains("https://www.example.com/smth."));
    }

    #[tokio::test]
    async fn test_extract_url_ending_with_period_webserver() {
        let mut cmd = main_command();
        let body = r#"<a href="https://www.example.com/smth.">link</a>"#;
        let mock_server = mock_response!(body);

        cmd.arg("--dump")
            .arg(mock_server.uri())
            .assert()
            .success()
            .stdout(contains("https://www.example.com/smth."));
    }
}
