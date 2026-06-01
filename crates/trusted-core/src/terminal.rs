//! Colored, high-visibility messages on stderr (safe for long scripts — stdout untouched).

use std::io::{self, Write};

use owo_colors::OwoColorize;

use crate::policy::{Violation, ViolationKind};
use crate::scan::{display_path, ScanReport, ScanViolation};

const BAR: &str = "════════════════════════════════════════════════════════════════════════";

/// Whether to emit ANSI colors (respects `NO_COLOR`, honors `CLICOLOR_FORCE=1`).
pub fn stderr_use_color() -> bool {
    if std::env::var_os("NO_COLOR").is_some() || std::env::var_os("TRUSTED_NO_COLOR").is_some() {
        return false;
    }
    if std::env::var("CLICOLOR_FORCE").as_deref() == Ok("1") {
        return true;
    }
    use std::io::IsTerminal;
    std::io::stderr().is_terminal()
}

fn writeln_err(line: impl AsRef<str>) {
    let mut out = io::stderr();
    let _ = writeln!(out, "{}", line.as_ref());
}

fn write_block<F>(plain: F)
where
    F: FnOnce(&mut dyn Write) -> io::Result<()>,
{
    let mut buf = Vec::new();
    if plain(&mut buf).is_ok() {
        let _ = io::stderr().write_all(&buf);
        let _ = io::stderr().flush();
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AuditKind {
    Check,
    Scan,
}

/// Plain-text install block (for tests and `--no-color` pipelines).
pub fn format_install_blocked(violations: &[Violation]) -> String {
    format_audit_report(
        "INSTALLATION STOPPED BY TRUSTED",
        &format!("Blocked {} package(s):", violations.len()),
        violations,
        "The package manager was NOT allowed to install. This command exits with code 1.",
        None,
    )
}

/// Plain-text check/scan failure.
pub fn format_audit_failed(kind: AuditKind, violations: &[Violation]) -> String {
    let (title, summary) = audit_titles(kind, violations.len());
    format_audit_report(
        title,
        &summary,
        violations,
        "No install was attempted. This command exits with code 1.",
        None,
    )
}

fn format_audit_report(
    title: &str,
    summary: &str,
    violations: &[Violation],
    footer: &str,
    extra_footer: Option<&str>,
) -> String {
    let mut lines = vec![title.to_string(), String::new(), summary.to_string()];
    for (i, v) in violations.iter().enumerate() {
        lines.push(format_violation_entry(i + 1, v));
    }
    lines.push(String::new());
    lines.push(footer.to_string());
    if let Some(extra) = extra_footer {
        lines.push(extra.to_string());
    }
    lines.join("\n")
}

fn audit_titles(kind: AuditKind, count: usize) -> (&'static str, String) {
    match kind {
        AuditKind::Check => (
            "CHECK FOUND POLICY VIOLATIONS",
            format!("{count} package(s) failed the safety check:"),
        ),
        AuditKind::Scan => (
            "SCAN FOUND POLICY VIOLATIONS",
            format!("{count} unsafe package(s) in this repo:"),
        ),
    }
}

fn audit_ok_title(kind: AuditKind) -> &'static str {
    match kind {
        AuditKind::Check => "CHECK PASSED",
        AuditKind::Scan => "SCAN PASSED",
    }
}

fn write_violation_list(w: &mut dyn Write, violations: &[Violation]) -> io::Result<()> {
    for (i, v) in violations.iter().enumerate() {
        write_one_violation(w, i + 1, v, None)?;
    }
    Ok(())
}

fn write_one_violation(
    w: &mut dyn Write,
    index: usize,
    v: &Violation,
    declared_in: Option<&str>,
) -> io::Result<()> {
    let (kind_label, detail) = violation_detail(v);
    writeln!(w, "  [{index}] {}", v.package.display().cyan().bold())?;
    writeln!(w, "      {} {}", kind_label.bold(), detail.white())?;
    if let Some(hint) = &v.hint {
        writeln!(w, "      {} {}", "hint:".dimmed(), hint.dimmed())?;
    }
    if let Some(paths) = declared_in {
        writeln!(w, "      {} {}", "declared in:".dimmed(), paths.dimmed())?;
    }
    writeln!(w)?;
    Ok(())
}

fn print_scan_violations(root: &std::path::Path, scan_violations: &[ScanViolation]) {
    let flat: Vec<Violation> = scan_violations
        .iter()
        .map(|sv| sv.violation.clone())
        .collect();
    let (title, summary) = audit_titles(AuditKind::Scan, scan_violations.len());

    if stderr_use_color() {
        write_block(|w| {
            writeln!(w)?;
            writeln!(w, "{}", BAR.red().bold())?;
            writeln!(w, "{}", format!("  {title}").red().bold())?;
            writeln!(w, "{}", BAR.red().bold())?;
            writeln!(w)?;
            writeln!(
                w,
                "{}",
                "  Read-only scan — no package manager install was run.".dimmed()
            )?;
            writeln!(w, "{}", summary.white())?;
            writeln!(w)?;
            for (i, sv) in scan_violations.iter().enumerate() {
                let declared = if sv.sources.is_empty() {
                    None
                } else {
                    Some(
                        sv.sources
                            .iter()
                            .map(|p| display_path(root, p))
                            .collect::<Vec<_>>()
                            .join(", "),
                    )
                };
                write_one_violation(w, i + 1, &sv.violation, declared.as_deref())?;
            }
            writeln!(
                w,
                "{}",
                "  Exit code: 1 — nothing was installed or modified.".red()
            )?;
            writeln!(w)?;
            writeln!(
                w,
                "{}",
                "  Config: ~/.config/trusted/config.toml  |  Project: .trusted.toml".dimmed()
            )?;
            writeln!(w)?;
            Ok(())
        });
    } else {
        writeln_err("");
        writeln_err(format_audit_failed(AuditKind::Scan, &flat));
        for sv in scan_violations {
            for p in &sv.sources {
                writeln_err(format!(
                    "      declared in: {} ({})",
                    display_path(root, p),
                    sv.violation.package.display()
                ));
            }
        }
        writeln_err("");
    }
}

fn print_audit_ok_banner(kind: AuditKind, message: &str) {
    let title = audit_ok_title(kind);
    if stderr_use_color() {
        write_block(|w| {
            writeln!(w)?;
            writeln!(w, "{}", BAR.green().bold())?;
            writeln!(w, "{}", format!("  {title}").green().bold())?;
            writeln!(w, "{}", BAR.green().bold())?;
            writeln!(w)?;
            writeln!(w, "{}", format!("  {message}").green())?;
            writeln!(w)?;
            Ok(())
        });
    } else {
        writeln_err("");
        writeln_err(title);
        writeln_err(message);
        writeln_err("");
    }
}

fn print_audit_failed_banner(kind: AuditKind, violations: &[Violation]) {
    let (title, summary) = audit_titles(kind, violations.len());
    if stderr_use_color() {
        write_block(|w| {
            writeln!(w)?;
            writeln!(w, "{}", BAR.red().bold())?;
            writeln!(w, "{}", format!("  {title}").red().bold())?;
            writeln!(w, "{}", BAR.red().bold())?;
            writeln!(w)?;
            writeln!(
                w,
                "{}",
                "  Read-only check — no package manager install was run.".dimmed()
            )?;
            writeln!(w, "{}", summary.white())?;
            writeln!(w)?;
            write_violation_list(w, violations)?;
            writeln!(
                w,
                "{}",
                "  Exit code: 1 — nothing was installed or modified.".red()
            )?;
            writeln!(w)?;
            writeln!(
                w,
                "{}",
                "  Config: ~/.config/trusted/config.toml  |  Project: .trusted.toml".dimmed()
            )?;
            writeln!(w)?;
            Ok(())
        });
    } else {
        writeln_err("");
        writeln_err(format_audit_failed(kind, violations));
        writeln_err("");
    }
}

fn format_violation_entry(index: usize, v: &Violation) -> String {
    let (kind_label, detail) = violation_detail(v);
    let pkg = v.package.display();
    let mut lines = vec![
        format!("  [{index}] {pkg}"),
        format!("      {kind_label}: {detail}"),
    ];
    if let Some(hint) = &v.hint {
        lines.push(format!("      hint: {hint}"));
    }
    lines.join("\n")
}

fn violation_detail(v: &Violation) -> (&'static str, String) {
    match &v.kind {
        ViolationKind::Osv { ids } => {
            let shown = if ids.len() > 4 {
                format!("{} (+{} more)", ids[..4].join(", "), ids.len() - 4)
            } else {
                ids.join(", ")
            };
            ("Security (OSV)", shown)
        }
        ViolationKind::TooNew {
            age_days,
            minimum_days,
        } => (
            "Policy (release age)",
            format!(
                "published {age_days} day(s) ago; minimum required age is {minimum_days} day(s)"
            ),
        ),
    }
}

/// Loud stderr banner when an install is blocked (shim or `trusted check`).
pub fn print_install_blocked(package_manager: Option<&str>, violations: &[Violation]) {
    let pm = package_manager.unwrap_or("package manager");
    let n = violations.len();

    if stderr_use_color() {
        write_block(|w| {
            writeln!(w)?;
            writeln!(w, "{}", BAR.red().bold())?;
            writeln!(w, "{}", "  INSTALLATION STOPPED BY TRUSTED".red().bold())?;
            writeln!(w, "{}", BAR.red().bold())?;
            writeln!(w)?;
            writeln!(
                w,
                "{}",
                format!("  Your {pm} install did NOT run.").yellow().bold()
            )?;
            writeln!(
                w,
                "{}",
                format!("  {n} package(s) failed the safety check.").white()
            )?;
            writeln!(w)?;
            for (i, v) in violations.iter().enumerate() {
                let (kind_label, detail) = violation_detail(v);
                writeln!(w, "  [{}] {}", i + 1, v.package.display().cyan().bold())?;
                writeln!(w, "      {} {}", kind_label.bold(), detail.white())?;
                if let Some(hint) = &v.hint {
                    writeln!(w, "      {} {}", "hint:".dimmed(), hint.dimmed())?;
                }
                writeln!(w)?;
            }
            writeln!(
                w,
                "{}",
                "  Exit code: 1 — the install was aborted before any packages were written.".red()
            )?;
            writeln!(w)?;
            writeln!(
                w,
                "{}",
                "  Config: ~/.config/trusted/config.toml  |  Project: .trusted.toml".dimmed()
            )?;
            writeln!(w)?;
            Ok(())
        });
    } else {
        writeln_err("");
        writeln_err(format_install_blocked(violations));
        writeln_err("");
    }
}

/// Warning before interactive override (`unsafe_action = ask`).
pub fn print_install_ask_prompt(package_manager: Option<&str>, violations: &[Violation]) {
    print_install_blocked(package_manager, violations);
    if stderr_use_color() {
        write_block(|w| {
            writeln!(
                w,
                "{}",
                "  Override: install unsafe version anyway? [y/N] "
                    .yellow()
                    .bold()
            )?;
            Ok(())
        });
    } else {
        writeln_err("  Override: install unsafe version anyway? [y/N] ");
    }
}

/// User chose not to override in ask mode.
pub fn print_install_declined() {
    if stderr_use_color() {
        write_block(|w| {
            writeln!(w, "{}", "  Aborted — install still blocked.".red().bold())?;
            Ok(())
        });
    } else {
        writeln_err("  Aborted — install still blocked.");
    }
}

/// `closest_safe` is rewriting versions before the real install runs.
pub fn print_repin(package_manager: Option<&str>, pins: &[(crate::types::PackageRef, String)]) {
    let pm = package_manager.unwrap_or("package manager");
    if stderr_use_color() {
        write_block(|w| {
            writeln!(w)?;
            writeln!(w, "{}", BAR.blue().bold())?;
            writeln!(
                w,
                "{}",
                "  TRUSTED: ADJUSTING VERSIONS (closest_safe)".blue().bold()
            )?;
            writeln!(w, "{}", BAR.blue().bold())?;
            writeln!(
                w,
                "{}",
                format!("  Continuing {pm} install with safer pins:").white()
            )?;
            writeln!(w)?;
            for (pkg, ver) in pins {
                writeln!(
                    w,
                    "    {}  {}  {}",
                    pkg.name.cyan().bold(),
                    pkg.version.dimmed(),
                    format!("-> {ver}").green().bold()
                )?;
            }
            writeln!(w)?;
            Ok(())
        });
    } else {
        writeln_err("");
        writeln_err("TRUSTED: adjusting versions (closest_safe)");
        for (pkg, ver) in pins {
            writeln_err(format!("  {} {} -> {}", pkg.name, pkg.version, ver));
        }
        writeln_err("");
    }
}

/// `trusted check` — all clear.
pub fn print_check_ok(count: usize) {
    print_audit_ok_banner(
        AuditKind::Check,
        &format!("All {count} package(s) passed OSV and release-age policy."),
    );
}

/// `trusted check` — policy violations found (no install attempted).
pub fn print_check_failed(_package_manager: Option<&str>, violations: &[Violation]) {
    print_audit_failed_banner(AuditKind::Check, violations);
}

/// Shims point at a different binary than the one running `trusted doctor`.
pub fn print_stale_shim_warning(current: &std::path::Path, shim_target: &std::path::Path) {
    if stderr_use_color() {
        write_block(|w| {
            writeln!(w)?;
            writeln!(
                w,
                "{}",
                "  WARNING: PATH shims are linked to a different trusted binary."
                    .yellow()
                    .bold()
            )?;
            writeln!(
                w,
                "    shims -> {}",
                shim_target.display().to_string().dimmed()
            )?;
            writeln!(
                w,
                "    you ran -> {}",
                current.display().to_string().dimmed()
            )?;
            writeln!(
                w,
                "{}",
                "    Fix: re-run setup from the binary you want, e.g.".white()
            )?;
            writeln!(w, "{}", "      ./target/release/trusted setup".cyan())?;
            writeln!(
                w,
                "{}",
                "    or:  cargo install --path crates/trusted --force && trusted setup".cyan()
            )?;
            writeln!(
                w,
                "{}",
                "    (cargo build --release alone does not update ~/.trusted/shims or ~/.cargo/bin/trusted)"
                    .dimmed()
            )?;
            writeln!(w)?;
            Ok(())
        });
    } else {
        writeln_err("");
        writeln_err("WARNING: PATH shims are linked to a different trusted binary.");
        writeln_err(format!("  shims -> {}", shim_target.display()));
        writeln_err(format!("  you ran -> {}", current.display()));
        writeln_err("  Fix: ./target/release/trusted setup");
        writeln_err("  or:  cargo install --path crates/trusted --force && trusted setup");
        writeln_err("");
    }
}

/// Repository scan summary and results.
pub fn print_scan_results(report: &ScanReport) {
    let root = &report.root;
    if stderr_use_color() {
        write_block(|w| {
            writeln!(w)?;
            writeln!(w, "{}", BAR.cyan().bold())?;
            writeln!(w, "{}", "  TRUSTED REPOSITORY SCAN".cyan().bold())?;
            writeln!(w, "{}", BAR.cyan().bold())?;
            writeln!(w, "  Root: {}", root.display().to_string().white())?;
            writeln!(w)?;
            if report.sources.is_empty() {
                writeln!(
                    w,
                    "{}",
                    "  No supported lockfiles or requirements files found."
                        .yellow()
                        .bold()
                )?;
                writeln!(
                    w,
                    "{}",
                    "  Looked for: package-lock.json, pnpm-lock.yaml, yarn.lock, uv.lock,".dimmed()
                )?;
                writeln!(
                    w,
                    "{}",
                    "  Pipfile.lock, Cargo.lock, go.mod, requirements.txt (also under subdirs)."
                        .dimmed()
                )?;
            } else {
                writeln!(w, "{}", "  Dependency sources:".white().bold())?;
                for source in &report.sources {
                    writeln!(
                        w,
                        "    {}  {}  ({} packages)",
                        source.manager.dimmed(),
                        display_path(root, &source.path).cyan(),
                        source.packages.len()
                    )?;
                }
                writeln!(w)?;
                writeln!(
                    w,
                    "  {} unique package(s) checked against OSV + release-age policy.",
                    report.unique_packages
                )?;
            }
            writeln!(w)?;
            Ok(())
        });
    } else {
        writeln_err("");
        writeln_err(format!("TRUSTED REPOSITORY SCAN: {}", root.display()));
        for source in &report.sources {
            writeln_err(format!(
                "  {} {} ({} packages)",
                source.manager,
                display_path(root, &source.path),
                source.packages.len()
            ));
        }
        writeln_err(format!(
            "  {} unique package(s) checked.",
            report.unique_packages
        ));
        writeln_err("");
    }

    if report.sources.is_empty() {
        return;
    }

    if report.violations.is_empty() {
        print_audit_ok_banner(
            AuditKind::Scan,
            &format!(
                "All {} unique package(s) passed OSV and release-age policy.",
                report.unique_packages
            ),
        );
        return;
    }

    print_scan_violations(root, &report.violations);
}

pub fn print_check_would_repin(pins: &[(crate::types::PackageRef, String)]) {
    if stderr_use_color() {
        write_block(|w| {
            writeln!(w)?;
            writeln!(w, "{}", BAR.yellow().bold())?;
            writeln!(
                w,
                "{}",
                "  CHECK: WOULD SUGGEST SAFER VERSIONS (closest_safe)"
                    .yellow()
                    .bold()
            )?;
            writeln!(w, "{}", BAR.yellow().bold())?;
            writeln!(
                w,
                "{}",
                "  Read-only check — no install was run. If this were an install:".dimmed()
            )?;
            writeln!(w)?;
            for (p, v) in pins {
                writeln!(w, "  {} -> {}", p.display().cyan(), v.green())?;
            }
            writeln!(w)?;
            Ok(())
        });
    } else {
        writeln_err("CHECK: would suggest safer versions (closest_safe):");
        for (p, v) in pins {
            writeln_err(format!("  {} -> {}", p.display(), v));
        }
    }
}
