use std::ffi::OsString;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use trusted_core::config::{global_config_path, shims_dir, write_default_config, Config};
use trusted_core::print_stale_shim_warning;
use trusted_core::types::{Ecosystem, PackageRef};
use trusted_shim::dispatch::{dispatch_shim, shim_from_argv0};
use trusted_shim::run::{run_check, run_scan};

#[derive(Parser)]
#[command(
    name = "trusted",
    about = "Install-time package safety guard",
    version = concat!(env!("CARGO_PKG_VERSION"), " (audit-output-v2)")
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Install PATH shims for package managers
    Setup,
    /// Verify installation and connectivity
    Doctor,
    /// Check explicit package(s) without installing
    Check {
        /// Packages as ecosystem:name@version (e.g. pypi:requests@2.32.0)
        #[arg(required = true)]
        packages: Vec<String>,
    },
    /// Discover lockfiles in a repo and check all pinned dependencies
    Scan {
        /// Repository root (default: current directory)
        #[arg(long, default_value = ".")]
        path: PathBuf,
    },
    /// Print effective configuration
    Config,
}

const SHIM_TOOLS: &[&str] = &["pip", "pip3", "uv", "npm", "pnpm", "cargo", "go"];

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("trusted=info")),
        )
        .init();

    let argv0 = std::env::args_os().next().unwrap_or_default();
    if shim_from_argv0(&argv0).is_some() {
        let args: Vec<OsString> = std::env::args_os().skip(1).collect();
        let code = dispatch_shim(&argv0, args).await?;
        std::process::exit(code);
    }

    let cli = Cli::parse();
    match cli.command {
        None => {
            eprintln!("trusted: use `trusted setup` or run via a package manager shim");
            eprintln!("Run `trusted --help` for subcommands.");
            Ok(())
        }
        Some(Commands::Setup) => cmd_setup(),
        Some(Commands::Doctor) => cmd_doctor().await,
        Some(Commands::Check { packages }) => {
            let refs = packages
                .iter()
                .map(|p| parse_check_pkg(p))
                .collect::<Result<Vec<_>>>()?;
            let code = run_check(refs).await?;
            std::process::exit(code);
        }
        Some(Commands::Scan { path }) => {
            let code = run_scan(&path).await?;
            std::process::exit(code);
        }
        Some(Commands::Config) => cmd_config(),
    }
}

fn cmd_setup() -> Result<()> {
    let shims = shims_dir()?;
    std::fs::create_dir_all(&shims)?;
    let exe = std::env::current_exe()?;
    link_shims(&exe, &shims)?;
    link_cli_binary(&exe)?;
    if let Some(path) = global_config_path() {
        write_default_config(&path)?;
        println!("Config: {}", path.display());
    }
    println!("Shims installed to: {}", shims.display());
    println!("Shims now link to: {}", exe.display());
    println!();
    println!("Add to your shell profile:");
    println!(
        r#"  export PATH="{}/shims:$PATH""#,
        trusted_core::config::trusted_home()?.display()
    );
    Ok(())
}

#[cfg(unix)]
fn link_shims(exe: &std::path::Path, shims: &std::path::Path) -> Result<()> {
    for tool in SHIM_TOOLS {
        let link = shims.join(tool);
        if link.exists() {
            std::fs::remove_file(&link)?;
        }
        std::os::unix::fs::symlink(exe, &link)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn link_shims(_exe: &std::path::Path, _shims: &std::path::Path) -> Result<()> {
    anyhow::bail!("trusted setup requires Unix (macOS/Linux) in v1");
}

/// Symlink this binary to `~/.cargo/bin/trusted` so `trusted check` on PATH uses the same build as shims.
#[cfg(unix)]
fn link_cli_binary(exe: &std::path::Path) -> Result<()> {
    let Some(home) = dirs::home_dir() else {
        return Ok(());
    };
    let cargo_bin = home.join(".cargo").join("bin");
    if !cargo_bin.is_dir() {
        return Ok(());
    }
    let dest = cargo_bin.join("trusted");
    if dest.exists() {
        std::fs::remove_file(&dest)?;
    }
    std::os::unix::fs::symlink(exe, &dest)?;
    println!("CLI binary: {} -> {}", dest.display(), exe.display());
    Ok(())
}

#[cfg(not(unix))]
fn link_cli_binary(_exe: &std::path::Path) -> Result<()> {
    Ok(())
}

async fn cmd_doctor() -> Result<()> {
    let shims = shims_dir()?;
    let current = std::env::current_exe()?.canonicalize()?;
    println!("trusted doctor");
    println!("  version: {} (audit-output-v2)", env!("CARGO_PKG_VERSION"));
    println!("  binary (this run): {}", current.display());
    if !binary_has_audit_output(&current) {
        eprintln!();
        eprintln!("  WARNING: this binary looks outdated (missing audit-output-v2).");
        eprintln!("  Reinstall: cargo install --path crates/trusted --force");
        eprintln!("  Or run:     ./target/release/trusted setup");
    }
    println!(
        "  shims dir: {} (exists: {})",
        shims.display(),
        shims.exists()
    );
    if shims.exists() {
        let npm_shim = shims.join("npm");
        if npm_shim.exists() {
            if let Ok(target) = std::fs::read_link(&npm_shim) {
                let resolved = if target.is_absolute() {
                    target
                } else {
                    shims.join(target)
                };
                let resolved = resolved.canonicalize().unwrap_or(resolved);
                println!("  shims/npm -> {}", resolved.display());
                if resolved != current {
                    print_stale_shim_warning(&current, &resolved);
                } else {
                    println!("  shims match this binary: yes");
                }
            }
        }
    }
    let path = std::env::var("PATH").unwrap_or_default();
    let shims_first = path
        .split(':')
        .next()
        .is_some_and(|p| std::path::Path::new(p) == shims.as_path());
    println!("  shims first on PATH: {shims_first}");
    for tool in SHIM_TOOLS {
        let ok = which(tool, &shims);
        println!("  {tool}: {}", if ok { "ok" } else { "not found on PATH" });
    }
    let config = Config::load()?;
    println!("  unsafe_action: {:?}", config.unsafe_action);
    println!("  min_release_age_days: {}", config.min_release_age_days);
    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.osv.dev/v1/query")
        .json(&serde_json::json!({
            "package": { "name": "requests", "ecosystem": "PyPI" },
            "version": "2.0.0"
        }))
        .send()
        .await?;
    println!("  OSV API reachable: {}", resp.status().is_success());
    Ok(())
}

fn binary_has_audit_output(path: &std::path::Path) -> bool {
    std::fs::read(path)
        .ok()
        .is_some_and(|bytes| {
            bytes
                .windows(b"CHECK FOUND POLICY VIOLATIONS".len())
                .any(|w| w == b"CHECK FOUND POLICY VIOLATIONS")
        })
}

fn which(tool: &str, _shims: &PathBuf) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|dir| dir.join(tool).is_file()))
        .unwrap_or(false)
}

fn cmd_config() -> Result<()> {
    let config = Config::load()?;
    println!("{}", toml::to_string_pretty(&config)?);
    Ok(())
}

fn parse_check_pkg(s: &str) -> Result<PackageRef> {
    let (eco, rest) = s
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("expected ecosystem:name@version, got {s}"))?;
    let ecosystem = match eco.to_lowercase().as_str() {
        "pypi" | "py" => Ecosystem::PyPI,
        "npm" | "node" => Ecosystem::Npm,
        "crates" | "crate" | "rust" => Ecosystem::CratesIo,
        "go" | "golang" => Ecosystem::Go,
        _ => anyhow::bail!("unknown ecosystem: {eco}"),
    };
    let (name, version) = rest
        .split_once('@')
        .ok_or_else(|| anyhow::anyhow!("expected name@version in {rest}"))?;
    Ok(PackageRef {
        ecosystem,
        name: name.to_string(),
        version: version.to_string(),
    })
}
