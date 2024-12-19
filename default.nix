{ pkgs }:
let
in
{
  app = pkgs.rustPlatform.buildRustPackage {
    pname = "lychee";
    version = "0.17.0";
    src = ./.;

    cargoLock = {
      lockFile = ./Cargo.lock;
    };

    nativeBuildInputs = [ pkgs.pkg-config ];
    buildInputs =
      [ pkgs.openssl ]
      ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isDarwin [
        pkgs.Security
        pkgs.SystemConfiguration
      ];

    PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
    RUST_BACKTRACE = 1;

    checkFlags = [
      "--skip=src/lib.rs"
      "--skip=client::tests"
      "--skip=collector::tests::test_url_without_extension_is_html"
    ];
  };
}
