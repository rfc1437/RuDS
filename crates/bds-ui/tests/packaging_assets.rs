use std::{fs, path::Path};

const CRATE_DIR: &str = env!("CARGO_MANIFEST_DIR");

#[test]
fn desktop_packages_have_native_icons_and_cargo_commands() {
    let assets = Path::new(CRATE_DIR).join("assets/app-icons");
    let png = fs::read(assets.join("bds.png")).expect("Linux PNG icon");
    let ico = fs::read(assets.join("bds.ico")).expect("Windows ICO icon");
    let icns = fs::read(assets.join("bds.icns")).expect("macOS ICNS icon");

    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 1024);
    assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 1024);
    assert_eq!(&ico[..4], &[0, 0, 1, 0]);
    assert!(u16::from_le_bytes(ico[4..6].try_into().unwrap()) >= 6);
    assert_eq!(&icns[..4], b"icns");

    let manifest = fs::read_to_string(Path::new(CRATE_DIR).join("Cargo.toml")).unwrap();
    for required in [
        "[package.metadata.packager]",
        "assets/app-icons/bds.png",
        "assets/app-icons/bds.ico",
        "assets/app-icons/bds.icns",
        "identifier = \"de.rfc1437.ruds\"",
        "deep-link-protocols = [{ schemes = [\"ruds\"] }]",
        "minimum-system-version = \"26.0\"",
        "../../target/release/bds-runtime/*",
        "cargo run --quiet -p bds-package-runtime",
        "cargo run --quiet -p bds-package-runtime -- sign-app-for-dmg",
        "{ path = \"bds-mcp\" }",
    ] {
        assert!(manifest.contains(required), "missing {required}");
    }
    assert!(!manifest.contains("signing-identity"));

    let cargo_config = fs::read_to_string(Path::new(CRATE_DIR).join("../../.cargo/config.toml"))
        .expect("Cargo packaging aliases");
    for alias in ["bundle-macos", "bundle-windows", "bundle-linux"] {
        assert!(cargo_config.contains(alias), "missing cargo {alias}");
    }
    assert!(cargo_config.contains("prefer-dynamic"));
    assert!(cargo_config.contains("-mmacosx-version-min=26.0"));
    assert!(cargo_config.contains("@executable_path/../Resources"));
    assert!(cargo_config.contains("$ORIGIN/../lib/bds-ui"));

    let main = fs::read_to_string(Path::new(CRATE_DIR).join("src/main.rs")).unwrap();
    assert!(main.contains("assets/app-icons/bds.png"));

    let build = fs::read_to_string(Path::new(CRATE_DIR).join("build.rs")).unwrap();
    assert!(build.contains("set_icon(\"assets/app-icons/bds.ico\")"));

    let workspace_manifest =
        fs::read_to_string(Path::new(CRATE_DIR).join("../../Cargo.toml")).unwrap();
    assert!(workspace_manifest.contains("strip = \"symbols\""));
    assert!(workspace_manifest.contains("ort-download-binaries-rustls-tls"));

    let core_manifest =
        fs::read_to_string(Path::new(CRATE_DIR).join("../bds-core/Cargo.toml")).unwrap();
    assert!(core_manifest.contains("crate-type = [\"dylib\"]"));
    let server_manifest =
        fs::read_to_string(Path::new(CRATE_DIR).join("../bds-server/Cargo.toml")).unwrap();
    assert!(server_manifest.contains("crate-type = [\"dylib\"]"));
    assert!(!manifest.contains("{ path = \"bds-server\" }"));
    assert!(!manifest.contains("-p bds-server"));
    assert!(
        !Path::new(CRATE_DIR)
            .join("../bds-server/src/main.rs")
            .exists()
    );

    let packager =
        fs::read_to_string(Path::new(CRATE_DIR).join("../bds-package-runtime/src/main.rs"))
            .unwrap();
    assert!(packager.contains("Command::new(\"codesign\")"));
    assert!(packager.contains("CARGO_PACKAGER_FORMAT"));
    assert!(!packager.contains("--options"));
}
