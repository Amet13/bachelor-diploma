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
        Some("pdfium") => pdfium(&root),
        _ => {
            eprintln!("usage: cargo xtask <schema [--check] | fixtures | pdfium>");
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

/// PDFium release pinned for builds and tests (`bblanchon/pdfium-binaries`).
const PDFIUM_TAG: &str = "chromium/8125";
const PDFIUM_BUILDS: &[(&str, &str)] = &[
    (
        "linux-x64",
        "e1c3ce6e270cb5e0c20816b379ad9f2c1309972697c505f508de92bf5e7d6659",
    ),
    (
        "linux-arm64",
        "8b8818f2ec57d1b213d952ca6d7f0ffeca9d0110487d0e20ad83ba918bc92efa",
    ),
    (
        "mac-x64",
        "8a5193535b80c7215d8932f6647ae5097465b6dba2811602c21e120c2af112b0",
    ),
    (
        "mac-arm64",
        "4939bd935ac83ddc09a3a4f038bc43a19cea1660316a3667f26cb07a070e139d",
    ),
    (
        "win-x64",
        "9a964261760cff52570aabf038ab346d3a5034dd8b30d0d5fe8861235d356b8b",
    ),
    (
        "win-arm64",
        "d394ba61991fb53f4d87f70988035f337d9e73f2bd2c3c1b966df8c43fcabb05",
    ),
];

fn pdfium(root: &Path) -> Result<()> {
    use sha2::{Digest, Sha256};
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "mac",
        "windows" => "win",
        other => bail!("no pinned PDFium build for {other}"),
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => bail!("no pinned PDFium build for {other}"),
    };
    let platform = format!("{os}-{arch}");
    let (_, sha) = PDFIUM_BUILDS
        .iter()
        .find(|(p, _)| *p == platform)
        .context("platform not pinned")?;
    let dest = root.join("target/pdfium");
    std::fs::create_dir_all(&dest)?;
    let archive = dest.join(format!("pdfium-{platform}.tgz"));
    let url = format!(
        "https://github.com/bblanchon/pdfium-binaries/releases/download/{}/pdfium-{platform}.tgz",
        PDFIUM_TAG.replace('/', "%2F")
    );
    let status = Command::new("curl")
        .args(["-sSfL", "-o"])
        .arg(&archive)
        .arg(&url)
        .status()
        .context("running curl")?;
    if !status.success() {
        bail!("download failed: {url}");
    }
    let digest =
        Sha256::digest(std::fs::read(&archive)?)
            .iter()
            .fold(String::new(), |mut acc, b| {
                use std::fmt::Write as _;
                let _ = write!(acc, "{b:02x}");
                acc
            });
    if digest != *sha {
        let _ = std::fs::remove_file(&archive);
        bail!("checksum mismatch for {url}: got {digest}, expected {sha}");
    }
    let status = Command::new("tar")
        .arg("xzf")
        .arg(&archive)
        .arg("-C")
        .arg(&dest)
        .status()
        .context("running tar")?;
    if !status.success() {
        bail!("could not unpack {}", archive.display());
    }
    println!(
        "PDFium {PDFIUM_TAG} ({platform}) ready in {}",
        dest.display()
    );
    Ok(())
}
