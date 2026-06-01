use trusted_core::policy::{Violation, ViolationKind};
use trusted_core::types::{Ecosystem, PackageRef};
use trusted_core::{format_audit_failed, AuditKind};

#[test]
fn format_violation_message() {
    let v = Violation {
        package: PackageRef {
            ecosystem: Ecosystem::PyPI,
            name: "badpkg".into(),
            version: "1.0.0".into(),
        },
        kind: ViolationKind::Osv {
            ids: vec!["OSV-TEST-1".into()],
        },
        hint: None,
    };
    let msg = format_audit_failed(AuditKind::Check, &[v]);
    assert!(msg.contains("CHECK FOUND POLICY VIOLATIONS"));
    assert!(msg.contains("badpkg@1.0.0"));
    assert!(msg.contains("OSV-TEST-1"));
    assert!(msg.contains("No install was attempted"));
}
