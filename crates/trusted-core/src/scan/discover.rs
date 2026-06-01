use std::path::Path;

use anyhow::Result;

use super::parse::{
    parse_cargo_lock, parse_go_mod, parse_npm_lock, parse_pipfile_lock, parse_pnpm_lock,
    parse_requirements_txt, parse_uv_lock, parse_yarn_lock, ScanSource,
};

type LockParser = fn(&Path) -> Result<Vec<crate::types::PackageRef>>;

const LOCK_NAMES: &[(&str, LockParser)] = &[
    ("package-lock.json", parse_npm_lock),
    ("pnpm-lock.yaml", parse_pnpm_lock),
    ("yarn.lock", parse_yarn_lock),
    ("uv.lock", parse_uv_lock),
    ("Pipfile.lock", parse_pipfile_lock),
    ("Cargo.lock", parse_cargo_lock),
    ("go.mod", parse_go_mod),
];

const REQ_NAMES: &[&str] = &[
    "requirements.txt",
    "requirements-dev.txt",
    "requirements-test.txt",
    "dev-requirements.txt",
];

/// Max directory depth below `root` to search for lockfiles (monorepos).
const MAX_DEPTH: usize = 6;

pub fn discover(root: &Path) -> Result<Vec<ScanSource>> {
    let mut sources = Vec::new();
    walk(root, root, 0, &mut |path| {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if let Some((_, parser)) = LOCK_NAMES.iter().find(|(name, _)| *name == file_name) {
            if let Ok(packages) = parser(path) {
                if !packages.is_empty() {
                    sources.push(ScanSource::new(path, manager_label(file_name), packages));
                }
            }
            return;
        }
        if REQ_NAMES.contains(&file_name) {
            if let Ok(packages) = parse_requirements_txt(path) {
                if !packages.is_empty() {
                    sources.push(ScanSource::new(path, "pip (requirements.txt)", packages));
                }
            }
        }
        if path
            .parent()
            .is_some_and(|p| p.file_name().is_some_and(|n| n == "requirements"))
            && file_name.ends_with(".txt")
        {
            if let Ok(packages) = parse_requirements_txt(path) {
                if !packages.is_empty() {
                    sources.push(ScanSource::new(path, "pip (requirements/*.txt)", packages));
                }
            }
        }
    });
    sources.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(sources)
}

fn manager_label(lock_name: &str) -> &'static str {
    match lock_name {
        "package-lock.json" => "npm (package-lock.json)",
        "pnpm-lock.yaml" => "pnpm (pnpm-lock.yaml)",
        "yarn.lock" => "yarn (yarn.lock)",
        "uv.lock" => "uv (uv.lock)",
        "Pipfile.lock" => "pip (Pipfile.lock)",
        "Cargo.lock" => "cargo (Cargo.lock)",
        "go.mod" => "go (go.mod)",
        _ => "unknown",
    }
}

fn walk(_root: &Path, dir: &Path, depth: usize, visit_file: &mut dyn FnMut(&Path)) {
    if depth > MAX_DEPTH {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if should_skip_dir(&path) {
            continue;
        }
        if path.is_dir() {
            walk(_root, &path, depth + 1, visit_file);
        } else if path.is_file() {
            visit_file(&path);
        }
    }
}

fn should_skip_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| {
            matches!(
                name,
                ".git"
                    | ".svn"
                    | "node_modules"
                    | "target"
                    | ".venv"
                    | "venv"
                    | "__pycache__"
                    | ".tox"
                    | "dist"
                    | "build"
                    | ".cargo"
            )
        })
}
