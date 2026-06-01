use std::io::Write;

use tempfile::TempDir;
use trusted_core::scan::discover;
use trusted_core::scan::parse::{
    parse_cargo_lock, parse_go_mod, parse_requirements_txt, parse_uv_lock,
};

#[test]
fn parse_requirements_pins() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("requirements.txt");
    let mut f = std::fs::File::create(&path).unwrap();
    writeln!(f, "urllib3==1.26.4").unwrap();
    writeln!(f, "# comment").unwrap();
    writeln!(f, "django>=4.0").unwrap();
    let pkgs = parse_requirements_txt(&path).unwrap();
    assert_eq!(pkgs.len(), 1);
    assert_eq!(pkgs[0].name, "urllib3");
    assert_eq!(pkgs[0].version, "1.26.4");
}

#[test]
fn parse_go_mod_requires() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("go.mod");
    std::fs::write(
        &path,
        r#"module example.com/foo

require (
    golang.org/x/text v0.14.0
)
"#,
    )
    .unwrap();
    let pkgs = parse_go_mod(&path).unwrap();
    assert_eq!(pkgs.len(), 1);
    assert_eq!(pkgs[0].name, "golang.org/x/text");
}

#[test]
fn parse_uv_lock_simple() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("uv.lock");
    std::fs::write(
        &path,
        r#"
[[package]]
name = "requests"
version = "2.31.0"
"#,
    )
    .unwrap();
    let pkgs = parse_uv_lock(&path).unwrap();
    assert!(pkgs
        .iter()
        .any(|p| p.name == "requests" && p.version == "2.31.0"));
}

#[test]
fn discover_finds_root_lockfiles() {
    let dir = TempDir::new().unwrap();
    let cargo = dir.path().join("Cargo.lock");
    std::fs::write(
        &cargo,
        r#"
[[package]]
name = "serde"
version = "1.0.0"
"#,
    )
    .unwrap();
    let sources = discover(dir.path()).unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].packages.len(), 1);
    let pkgs = parse_cargo_lock(&cargo).unwrap();
    assert_eq!(pkgs[0].name, "serde");
}
