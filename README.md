# 🛡️ Trusted

🤖+🧑‍💻 _**Trusted protects both you and you agents!**_

**Trusted** is the lightweight safety net that ensures your app only runs what is safe. No blind faith and no complex bloat - Trusted is instant, bulletproof validation that stops known vulnerable and malicious packages before they touch your codebase. It checks resolved packages against [OSV](https://osv.dev), enforces a minimum release age (default 7 days), and blocks or adjusts unsafe installs.

Trused wraps common package managers, so you (and your agents!) don't need to run any additional commands - simply install as you always do, and _trusted_ works in the background for you.

## Supported package managers

- pip / pip3
- uv
- npm
- pnpm
- cargo
- go

## Quick start

Download a release from [GitHub Releases](https://github.com/shakedzychlinski/repos/trusted/releases) (draft builds are produced by the Release workflow), or build from source:

```bash
cargo install --path crates/trusted
trusted setup
# Add to ~/.zshrc or ~/.bashrc:
export PATH="$HOME/.trusted/shims:$PATH"
```

If you develop from the repo, **`cargo build --release` does not update `~/.cargo/bin/trusted` by itself**. Run setup from the binary you built (updates shims **and** `~/.cargo/bin/trusted`):

```bash
cargo build --release
./target/release/trusted setup
```

Or: `cargo install --path crates/trusted --force`

Run `trusted doctor` — version should include `(audit-output-v2)`. If `trusted check` still says "INSTALLATION STOPPED", your PATH binary is stale; run `setup` again.

Copy [config.example.toml](config.example.toml) to `~/.config/trusted/config.toml` (created automatically on `trusted setup`).

## Usage

After setup, use package managers normally:

```bash
pip install requests
npm install lodash
```

### Subcommands

- `trusted setup` — install shims under `~/.trusted/shims`
- `trusted doctor` — verify PATH, shims, and OSV connectivity
- `trusted check pypi:requests@2.32.0` — check packages without installing
- `trusted scan` — find lockfiles in the repo and check every pinned dependency
- `trusted scan --path /path/to/repo` — scan another directory
- `trusted config` — print effective configuration

### `trusted scan`

Walks the repository (skipping `node_modules`, `.git`, `target`, venvs, etc.) and parses:

| File | Ecosystem |
|------|-----------|
| `package-lock.json` | npm |
| `pnpm-lock.yaml` | pnpm |
| `yarn.lock` | yarn / npm |
| `uv.lock` | PyPI |
| `Pipfile.lock` | PyPI |
| `requirements.txt` (pinned `==` only) | PyPI |
| `Cargo.lock` | crates.io |
| `go.mod` | Go |

Uses the same OSV and release-age rules as install-time checks; does not run package managers or install anything.

### Configuration

| Option | Default | Description |
|--------|---------|-------------|
| `unsafe_action` | `block` | `block`, `ask`, or `closest_safe` |
| `min_release_age_days` | `7` | Reject versions newer than this (0 = off) |
| `closest_safe_no_candidate` | `block` | When no safe version exists in range |

Project overrides: `.trusted.toml` in the current directory.

## Policies

- **block** — abort install and print violations
- **ask** — prompt on a TTY; CI/non-TTY falls back to block
- **closest_safe** — re-pin to the highest safe version at or below the resolver’s chosen version, then install once

## Terminal output

Blocked installs print a **red banner on stderr** (not stdout), so long scripts and piped logs stay readable. Colors follow your terminal; disable with `NO_COLOR=1` or `TRUSTED_NO_COLOR=1`.
