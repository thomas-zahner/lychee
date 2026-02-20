use anyhow::Result;

use super::StatsFormatter;
use crate::formatters::stats::{OutputStats, ResponseStats};

/// The JUnit XML report format.
/// This format can be imported on code forges (e.g. GitHub & GitLab)
/// to create useful annotations where failing links are detected.
pub(crate) struct Junit {}

impl Junit {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl StatsFormatter for Junit {
    /// Format stats as JSON object
    fn format(&self, stats: OutputStats) -> Result<String> {
        Ok(junit_xml(stats.response_stats))
    }
}

/// Unfortunately there is no official specification of this format,
/// but there is documentation available at <https://github.com/testmoapp/junitxml>.
/// Note that using a library would be overkill in this case.
fn junit_xml(response_stats: ResponseStats) -> String {
    let testcases = junit_testcases(response_stats);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="lychee link check results">{testcases}</testsuites>
"#
    )
}

fn junit_testcases(response_stats: ResponseStats) -> String {
    response_stats
        .error_map
        .into_iter()
        .flat_map(|(s, b)| {
            b.into_iter().map(move |r| {
                let name = format!("Broken URL: {}", r.uri);
                let message = format!("Check returned: {}", r.status.code_as_string());
                let details = r.status.details().unwrap_or_default();
                let line = r
                    .span
                    .map(|s| format!(r#" line="{}""#, s.line))
                    .unwrap_or_default();

                format!(
                    r#"
    <testcase name="{name}" file="{s}"{line}>
        <failure message="{message}">{details}</failure>
    </testcase>
"#
                )
            })
        })
        .collect::<Vec<String>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use crate::formatters::stats::{StatsFormatter, get_dummy_stats, junit::Junit};

    #[test]
    fn test_junit_formatter() {
        let formatter = Junit::new();
        let result = formatter.format(get_dummy_stats()).unwrap();

        assert_eq!(result, r#""#);
    }
}
