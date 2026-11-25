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
        Some("wasm") => wasm(&root, "site/app/engine", "no-modules"),
        Some("npm") => wasm(&root, "npm/engine", "nodejs"),
        Some("sample") => sample(&root),
        Some("pdfium") => pdfium(
            &root,
            args.iter()
                .position(|a| a == "--platform")
                .and_then(|i| args.get(i + 1))
                .map(String::as_str),
        ),
        _ => {
            eprintln!(
                "usage: cargo xtask <schema [--check] | fixtures | pdfium [--platform linux-x64|mac-arm64|...] | wasm | npm | sample>"
            );
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

fn pdfium(root: &Path, platform_override: Option<&str>) -> Result<()> {
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
    let platform = platform_override.map_or_else(|| format!("{os}-{arch}"), str::to_owned);
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

/// PDFium WebAssembly build pinned for the browser app.
const PDFIUM_WASM_SHA256: &str = "77db4ef8c9ab0ecef2af659fe7d407279a0ebacff4cfcc1b3cc28160be2abfbd";
/// Must match the `wasm-bindgen` crate version pinned in `crates/proofback-wasm`.
const WASM_BINDGEN_VERSION: &str = "0.2.130";

fn wasm(root: &Path, out_dir: &str, target: &str) -> Result<()> {
    use sha2::{Digest, Sha256};
    let out = root.join(out_dir);
    std::fs::create_dir_all(&out)?;
    let status = Command::new("cargo")
        .args([
            "build",
            "--locked",
            "--profile",
            "wasm-release",
            "--target",
            "wasm32-unknown-unknown",
            "-p",
            "proofback-wasm",
        ])
        .current_dir(root)
        .status()?;
    if !status.success() {
        bail!("cargo build for wasm32 failed");
    }
    let version = Command::new("wasm-bindgen")
        .arg("--version")
        .output()
        .context("wasm-bindgen CLI not found; install version 0.2.130")?;
    let version = String::from_utf8_lossy(&version.stdout).to_string();
    if !version.contains(WASM_BINDGEN_VERSION) {
        bail!("wasm-bindgen CLI {version:?} does not match {WASM_BINDGEN_VERSION}");
    }
    let wasm_in = root.join("target/wasm32-unknown-unknown/wasm-release/proofback_wasm.wasm");
    let status = Command::new("wasm-bindgen")
        .arg(&wasm_in)
        .args(["--target", target, "--no-typescript", "--out-dir"])
        .arg(&out)
        .status()?;
    if !status.success() {
        bail!("wasm-bindgen failed");
    }
    if target == "nodejs" {
        // pdfium-render needs the module's own exports (memory, table) to bridge
        // into PDFium; wasm-bindgen's Node output does not expose them.
        let js = out.join("proofback_wasm.js");
        let mut text = std::fs::read_to_string(&js)?;
        text.push_str("\nmodule.exports.__wasm = wasm;\n");
        std::fs::write(&js, text)?;
    }
    let bg = out.join("proofback_wasm_bg.wasm");
    match Command::new("wasm-opt")
        .args([
            "-Os",
            "--enable-bulk-memory",
            "--enable-nontrapping-float-to-int",
            "--enable-sign-ext",
            "--enable-mutable-globals",
            "-o",
        ])
        .arg(&bg)
        .arg(&bg)
        .status()
    {
        Ok(s) if s.success() => {}
        _ => eprintln!(
            "warning: wasm-opt not available or failed; the engine is larger than necessary"
        ),
    }
    // PDFium's WebAssembly build.
    let archive = root.join("target/pdfium-wasm.tgz");
    if !archive.exists() {
        let url = format!(
            "https://github.com/bblanchon/pdfium-binaries/releases/download/{}/pdfium-wasm.tgz",
            PDFIUM_TAG.replace('/', "%2F")
        );
        let status = Command::new("curl")
            .args(["-sSfL", "-o"])
            .arg(&archive)
            .arg(&url)
            .status()?;
        if !status.success() {
            bail!("download failed: {url}");
        }
    }
    let digest =
        Sha256::digest(std::fs::read(&archive)?)
            .iter()
            .fold(String::new(), |mut acc, b| {
                use std::fmt::Write as _;
                let _ = write!(acc, "{b:02x}");
                acc
            });
    if digest != PDFIUM_WASM_SHA256 {
        let _ = std::fs::remove_file(&archive);
        bail!("checksum mismatch for pdfium-wasm.tgz: {digest}");
    }
    let tmp = root.join("target/pdfium-wasm");
    std::fs::create_dir_all(&tmp)?;
    let status = Command::new("tar")
        .arg("xzf")
        .arg(&archive)
        .arg("-C")
        .arg(&tmp)
        .status()?;
    if !status.success() {
        bail!("could not unpack pdfium-wasm.tgz");
    }
    for f in ["pdfium.js", "pdfium.wasm"] {
        std::fs::copy(tmp.join("lib").join(f), out.join(f))?;
    }
    std::fs::copy(tmp.join("LICENSE"), out.join("PDFIUM-LICENSE.txt"))?;
    for e in std::fs::read_dir(&out)? {
        let e = e?;
        println!(
            "{:>10}  {}",
            e.metadata()?.len(),
            e.file_name().to_string_lossy()
        );
    }
    Ok(())
}

/// An annotated review of `fixtures/thesis-long` plus the native import result, used
/// to check that the browser app gives identical answers.
fn sample(root: &Path) -> Result<()> {
    let fx_dir = root.join("fixtures/thesis-long");
    let fx = proofback_corpus::Fixture::load(&fx_dir)?;
    let pdfium = proofback_pdf::load_pdfium(None)?;
    let (pdf, truths) =
        proofback_corpus::generate(&fx, &pdfium, 40, 99).map_err(anyhow::Error::msg)?;
    let (_, items) = proofback_corpus::evaluate(&fx, &pdfium, &pdf, &truths, true)
        .map_err(anyhow::Error::msg)?;
    let out = root.join("target/sample");
    std::fs::create_dir_all(&out)?;
    std::fs::write(out.join("review.pdf"), &pdf)?;
    let expected: Vec<serde_json::Value> = items
        .iter()
        .map(|i| serde_json::json!({
            "nm": match &i.origin { proofback_model::Origin::Pdf { nm, .. } => nm.clone(), _ => None },
            "confidence": i.anchor.confidence.as_str(),
            "file": i.target.as_ref().map(|t| t.file.clone()),
            "start": i.target.as_ref().map(|t| t.range.start),
            "end": i.target.as_ref().map(|t| t.range.end),
        }))
        .collect();
    std::fs::write(
        out.join("expected.json"),
        serde_json::to_string_pretty(&expected)?,
    )?;
    println!("wrote {} items to {}", expected.len(), out.display());
    Ok(())
}
