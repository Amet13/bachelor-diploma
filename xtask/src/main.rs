//! Development tasks: `cargo xtask <task>`.
//!
//! - `schema [--check]`  write (or verify) `site/schema/v1.json`
//! - `fixtures`          rebuild PDFs and `SyncTeX` files under `fixtures/` with pdflatex

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("workspace root")?
        .to_path_buf();
    match args.first().map(String::as_str) {
        Some("schema") => schema(&root, args.iter().any(|a| a == "--check")),
        Some("fixtures") => fixtures(&root),
        _ => {
            eprintln!("usage: cargo xtask <schema [--check] | fixtures>");
            std::process::exit(2);
        }
    }
}

fn schema(root: &Path, check: bool) -> Result<()> {
    let path = root.join("site/schema/v1.json");
    let mut text = serde_json::to_string_pretty(&proofback_model::round_file_schema())?;
    text.push('\n');
    if check {
        let current = std::fs::read_to_string(&path).unwrap_or_default();
        if current != text {
            bail!(
                "{} is out of date; run `cargo xtask schema`",
                path.display()
            );
        }
        println!("schema up to date");
        return Ok(());
    }
    std::fs::create_dir_all(path.parent().context("schema dir")?)?;
    std::fs::write(&path, text)?;
    println!("wrote {}", path.display());
    Ok(())
}

fn fixtures(root: &Path) -> Result<()> {
    let dir = root.join("fixtures");
    let mut built = 0;
    for entry in std::fs::read_dir(&dir)? {
        let project: PathBuf = entry?.path();
        let main = project.join("main.tex");
        if !main.is_file() {
            continue;
        }
        let engine = std::fs::read_to_string(project.join("ENGINE"))
            .map_or_else(|_| "pdflatex".to_owned(), |s| s.trim().to_owned());
        for _ in 0..2 {
            let status = Command::new(&engine)
                .args([
                    "-synctex=1",
                    "-interaction=nonstopmode",
                    "-halt-on-error",
                    "main.tex",
                ])
                .current_dir(&project)
                .stdout(std::process::Stdio::null())
                .status()
                .with_context(|| format!("running {engine} in {}", project.display()))?;
            if !status.success() {
                bail!("{engine} failed in {}", project.display());
            }
        }
        for ext in ["aux", "log", "out", "toc"] {
            let _ = std::fs::remove_file(project.join(format!("main.{ext}")));
        }
        built += 1;
        println!("built {}", project.display());
    }
    println!("{built} fixture(s) built");
    Ok(())
}
