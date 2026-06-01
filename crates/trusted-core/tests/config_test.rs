use trusted_core::config::Config;
use trusted_core::types::{Ecosystem, PackageRef};

#[test]
fn default_config_values() {
    let cfg = Config::default();
    assert_eq!(cfg.min_release_age_days, 7);
}

#[test]
fn allowlist_matches() {
    let mut cfg = Config::default();
    cfg.allow.push(trusted_core::config::AllowEntry {
        ecosystem: "PyPI".into(),
        package: "foo".into(),
        version: Some("1.0.0".into()),
    });
    let pkg = PackageRef {
        ecosystem: Ecosystem::PyPI,
        name: "foo".into(),
        version: "1.0.0".into(),
    };
    assert!(cfg.is_allowed(&pkg));
}
