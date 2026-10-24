//! Repository tasks, run with `cargo xtask <task>` (inside the dev container: `scripts/dev.sh`).
//!
//! - `ci`: fmt check, clippy with `-D warnings`, tests (nextest when installed), layering and
//!   asset checks. The gate before every commit.
//! - `layers`: library crates may only depend on crates in lower layers (see `LAYERS`).
//! - `assets`: every file under `assets/`, `docs/images/` and `packaging/` icons has a row in
//!   `ATTRIBUTION.md`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// Workspace crates by layer. A crate may depend (outside dev-dependencies) only on crates in a
/// strictly lower layer. Keep in sync with `docs/architecture.md`.
const LAYERS: &[(&str, u8)] = &[
    ("ampcraft-dock", 0),
    ("ampcraft-dsp", 0),
    ("ampcraft-core", 1),
    ("ampcraft-skin", 1),
    ("ampcraft-audio-out", 1),
    ("ampcraft-library", 2),
    ("ampcraft-ui", 2),
    ("ampcraft-platform", 3),
    ("ampcraft", 4),
];

/// Files that need an `ATTRIBUTION.md` row: everything under these folders...
const ASSET_DIRS: &[&str] = &["assets", "docs/images"];
/// ...plus these single files.
const ASSET_FILES: &[&str] = &["packaging/ampcraft.svg"];

fn main() -> ExitCode {
    let task = std::env::args().nth(1).unwrap_or_default();
    let root = workspace_root();
    let result = match task.as_str() {
        "ci" => ci(&root),
        "layers" => layers(&root),
        "assets" => assets(&root),
        _ => Err("usage: cargo xtask <ci|layers|assets>".to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}

fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().map(Path::to_path_buf).unwrap_or(manifest)
}

fn ci(root: &Path) -> Result<(), String> {
    step(root, "fmt", &["fmt", "--all", "--", "--check"])?;
    step(
        root,
        "clippy",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    if has_nextest(root) {
        step(
            root,
            "tests (nextest)",
            &[
                "nextest",
                "run",
                "--workspace",
                "--all-targets",
                "--no-tests=pass",
                "--no-fail-fast",
            ],
        )?;
        step(root, "doctests", &["test", "--workspace", "--doc"])?;
    } else {
        step(root, "tests", &["test", "--workspace", "--all-targets"])?;
    }
    println!("==> layers");
    layers(root)?;
    println!("==> assets");
    assets(root)?;
    println!("==> xtask ci passed");
    Ok(())
}

fn step(root: &Path, name: &str, args: &[&str]) -> Result<(), String> {
    println!("==> {name}");
    let status = Command::new(cargo())
        .args(args)
        .current_dir(root)
        .status()
        .map_err(|e| format!("{name}: cannot run cargo: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{name} failed"))
    }
}

fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

fn has_nextest(root: &Path) -> bool {
    Command::new(cargo())
        .args(["nextest", "--version"])
        .current_dir(root)
        .output()
        .is_ok_and(|o| o.status.success())
}

fn layers(root: &Path) -> Result<(), String> {
    let layer: BTreeMap<&str, u8> = LAYERS.iter().copied().collect();
    let mut errors = Vec::new();
    for manifest in member_manifests(root)? {
        let text =
            fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
        let Some(name) = package_name(&text) else {
            continue;
        };
        if name == "xtask" {
            continue;
        }
        let Some(&own) = layer.get(name.as_str()) else {
            errors.push(format!("{name} has no layer in xtask/src/main.rs LAYERS"));
            continue;
        };
        for dep in normal_dependencies(&text) {
            match layer.get(dep.as_str()) {
                Some(&l) if l < own => {}
                Some(&l) => {
                    errors.push(format!("{name} (layer {own}) depends on {dep} (layer {l})"))
                }
                None => {}
            }
        }
    }
    report(errors)
}

fn member_manifests(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for dir in ["crates", "apps"] {
        let entries = fs::read_dir(root.join(dir)).map_err(|e| format!("{dir}: {e}"))?;
        for entry in entries.flatten() {
            let m = entry.path().join("Cargo.toml");
            if m.is_file() {
                out.push(m);
            }
        }
    }
    out.sort();
    Ok(out)
}

fn package_name(manifest: &str) -> Option<String> {
    let mut in_package = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
        } else if in_package && let Some(v) = line.strip_prefix("name = ") {
            return Some(v.trim_matches('"').to_string());
        }
    }
    None
}

/// Workspace crates named in `[dependencies]` and target-specific dependency tables.
fn normal_dependencies(manifest: &str) -> Vec<String> {
    let mut in_deps = false;
    let mut out = Vec::new();
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_deps = line.ends_with("dependencies]")
                && !line.contains("dev-dependencies")
                && !line.contains("build-dependencies");
        } else if in_deps && line.starts_with("ampcraft") {
            let name: String = line
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect();
            out.push(name);
        }
    }
    out
}

fn assets(root: &Path) -> Result<(), String> {
    let attribution = fs::read_to_string(root.join("ATTRIBUTION.md"))
        .map_err(|e| format!("ATTRIBUTION.md: {e}"))?;
    let mut files: Vec<String> = ASSET_FILES.iter().map(|s| s.to_string()).collect();
    for dir in ASSET_DIRS {
        collect_files(root, &root.join(dir), &mut files);
    }
    let errors = files
        .into_iter()
        .filter(|f| root.join(f).is_file() && !attribution.contains(&format!("`{f}`")))
        .map(|f| format!("{f} has no row in ATTRIBUTION.md (write the path in backticks)"))
        .collect();
    report(errors)
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, out);
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

fn report(errors: Vec<String>) -> Result<(), String> {
    if errors.is_empty() {
        return Ok(());
    }
    for e in &errors {
        eprintln!("  {e}");
    }
    Err(format!("{} problem(s)", errors.len()))
}
