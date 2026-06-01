pub mod cache;
pub mod config;
pub mod http_client;
pub mod osv;
pub mod policy;
pub mod progress;
pub mod registry;
pub mod scan;
pub mod terminal;
pub mod types;

pub use config::Config;
pub use policy::{PolicyEngine, PolicyOutcome, Violation, ViolationKind};
pub use scan::{scan_repo, ScanReport, ScanSource, ScanViolation};
pub use terminal::{
    format_audit_failed, format_install_blocked, print_check_failed, print_check_ok,
    print_check_would_repin, print_install_ask_prompt, print_install_blocked,
    print_install_declined, print_repin, print_scan_results, print_stale_shim_warning, AuditKind,
};
pub use types::{Ecosystem, PackageRef, UnsafeAction};
