use trusted_core::types::{Ecosystem, PackageRef};

#[test]
fn package_display() {
    let p = PackageRef {
        ecosystem: Ecosystem::Npm,
        name: "lodash".into(),
        version: "4.17.21".into(),
    };
    assert_eq!(p.display(), "lodash@4.17.21");
}
