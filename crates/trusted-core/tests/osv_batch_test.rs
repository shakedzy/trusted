use trusted_core::types::{Ecosystem, PackageRef};

#[test]
fn osv_fixture_package_ref() {
    let pkg = PackageRef {
        ecosystem: Ecosystem::PyPI,
        name: "vuln-demo".into(),
        version: "1.0.0".into(),
    };
    assert_eq!(pkg.display(), "vuln-demo@1.0.0");
    assert_eq!(pkg.ecosystem.osv_name(), "PyPI");
}
