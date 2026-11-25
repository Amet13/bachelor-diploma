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
        Some("bump") => bump(
            &root,
            args.get(1).context("usage: cargo xtask bump <X.Y.Z>")?,
        ),
        Some("release-check") => release_check(
            &root,
            args.get(1)
                .context("usage: cargo xtask release-check <X.Y.Z>")?
                .trim_start_matches('v'),
        ),
        Some("pdfium") => pdfium(
            &root,
            args.iter()
                .position(|a| a == "--platform")
                .and_then(|i| args.get(i + 1))
                .map(String::as_str),
        ),
        _ => {
            eprintln!(
                "usage: cargo xtask <schema [--check] | fixtures | pdfium [--platform linux-x64|mac-arm64|...] | wasm | npm | sample | bump <patch|minor|major|X.Y.Z> | release-check <X.Y.Z>>"
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

/// Files that carry the release version, and how to find it in each. The first
/// `count` matches of `"version": "X"` (JSON) are the package's own version.
const JSON_VERSION_FILES: &[(&str, usize)] = &[
    ("npm/package.json", 1),
    ("integrations/vscode/package.json", 1),
    ("integrations/vscode/package-lock.json", 2),
    ("integrations/claude-code/.claude-plugin/plugin.json", 1),
];

fn is_semver(v: &str) -> bool {
    let core = v.split(['-', '+']).next().unwrap_or("");
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// The workspace version from the root `Cargo.toml`.
fn workspace_version(root: &Path) -> Result<String> {
    let text = std::fs::read_to_string(root.join("Cargo.toml"))?;
    text.lines()
        .find_map(|l| {
            l.strip_prefix("version = \"")
                .and_then(|r| r.strip_suffix('"'))
        })
        .map(str::to_owned)
        .context("no version in Cargo.toml")
}

/// Every place the version is written, with the version found there.
fn versions(root: &Path) -> Result<Vec<(String, String)>> {
    let mut out = vec![("Cargo.toml".to_owned(), workspace_version(root)?)];
    for (file, count) in JSON_VERSION_FILES {
        let text = std::fs::read_to_string(root.join(file)).with_context(|| file.to_string())?;
        let found: Vec<&str> = text
            .match_indices("\"version\": \"")
            .take(*count)
            .map(|(i, m)| {
                let rest = &text[i + m.len()..];
                &rest[..rest.find('"').unwrap_or(0)]
            })
            .collect();
        if found.len() != *count {
            bail!("{file}: expected {count} version field(s)");
        }
        for v in found {
            out.push(((*file).to_owned(), v.to_owned()));
        }
    }
    for dir in ["crates", "xtask"] {
        let mut manifests = vec![];
        if dir == "xtask" {
            manifests.push(root.join("xtask/Cargo.toml"));
        } else {
            for e in std::fs::read_dir(root.join(dir))? {
                manifests.push(e?.path().join("Cargo.toml"));
            }
        }
        for m in manifests {
            let text = std::fs::read_to_string(&m)?;
            for line in text.lines().filter(|l| l.contains("path = \"../")) {
                if let Some(v) = line
                    .split("version = \"")
                    .nth(1)
                    .and_then(|r| r.split('"').next())
                {
                    let rel = m.strip_prefix(root).unwrap_or(&m).display().to_string();
                    out.push((rel, v.to_owned()));
                }
            }
        }
    }
    Ok(out)
}

/// Today's date (UTC) as YYYY-MM-DD, without a date library.
fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Set the version everywhere, refresh Cargo.lock and date the changelog section.
/// The version after `old` for `patch`, `minor` or `major`; any other input is
/// taken as an exact version.
fn next_version(old: &str, how: &str) -> Result<String> {
    let core = old.split(['-', '+']).next().unwrap_or(old);
    let n: Vec<u64> = core
        .split('.')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .with_context(|| format!("current version {old} is not numeric"))?;
    let [major, minor, patch] = n[..] else {
        bail!("current version {old} is not like 1.2.3");
    };
    Ok(match how {
        "patch" => format!("{major}.{minor}.{}", patch + 1),
        "minor" => format!("{major}.{}.0", minor + 1),
        "major" => format!("{}.0.0", major + 1),
        exact => exact.trim_start_matches('v').to_owned(),
    })
}

fn bump(root: &Path, how: &str) -> Result<()> {
    let old = workspace_version(root)?;
    let new = next_version(&old, how)?;
    let new = new.as_str();
    if !is_semver(new) {
        bail!("{new} is not a version like 1.2.3 (or use patch, minor, major)");
    }
    let mut touched = 0;
    for (file, _) in versions(root)?.iter().filter(|(_, v)| *v == old) {
        let path = root.join(file);
        let text = std::fs::read_to_string(&path)?;
        let json_count = JSON_VERSION_FILES
            .iter()
            .find(|(f, _)| f == file)
            .map(|(_, c)| *c);
        let updated = if let Some(count) = json_count {
            text.replacen(
                &format!("\"version\": \"{old}\""),
                &format!("\"version\": \"{new}\""),
                count,
            )
        } else if file == "Cargo.toml" {
            text.replacen(
                &format!("version = \"{old}\""),
                &format!("version = \"{new}\""),
                1,
            )
        } else {
            text.replace(
                &format!("version = \"{old}\", path = \"../"),
                &format!("version = \"{new}\", path = \"../"),
            )
        };
        if updated != text {
            std::fs::write(&path, updated)?;
            touched += 1;
        }
    }
    // Documentation that pins the GitHub Action to a release tag.
    let action_doc = root.join("docs/github-action.md");
    let text = std::fs::read_to_string(&action_doc)?;
    let updated = text.replace(
        &format!("Amet13/proofback@v{old}"),
        &format!("Amet13/proofback@v{new}"),
    );
    if updated != text {
        std::fs::write(&action_doc, updated)?;
        touched += 1;
    }
    let changelog = root.join("CHANGELOG.md");
    let text = std::fs::read_to_string(&changelog).context("CHANGELOG.md")?;
    if text.contains(&format!("## {new} (")) {
        println!("CHANGELOG.md already has a {new} section");
    } else if text.contains("## Unreleased") {
        std::fs::write(
            &changelog,
            // Keep an empty Unreleased section on top for the next changes.
            text.replacen(
                "## Unreleased",
                &format!("## Unreleased\n\n## {new} ({})", today()),
                1,
            ),
        )?;
    } else {
        bail!("CHANGELOG.md has no \"## Unreleased\" section to turn into {new}");
    }
    let status = Command::new("cargo")
        .args(["update", "--workspace", "--quiet"])
        .current_dir(root)
        .status()?;
    if !status.success() {
        bail!("cargo update --workspace failed");
    }
    release_check(root, new)?;
    println!(
        "{old} -> {new}: {touched} files updated. Merging this to the default branch releases v{new}."
    );
    Ok(())
}

/// Check that every version field and the changelog agree with `version`, and
/// write that version's changelog section to target/release-notes.md.
fn release_check(root: &Path, version: &str) -> Result<()> {
    let wrong: Vec<String> = versions(root)?
        .into_iter()
        .filter(|(_, v)| v != version)
        .map(|(f, v)| format!("{f}: {v}"))
        .collect();
    if !wrong.is_empty() {
        bail!(
            "these files do not say {version} (run `cargo xtask bump {version}`):\n  {}",
            wrong.join("\n  ")
        );
    }
    let lock = std::fs::read_to_string(root.join("Cargo.lock"))?;
    if !lock.contains(&format!(
        "name = \"proofback-cli\"\nversion = \"{version}\""
    )) {
        bail!("Cargo.lock is not at {version}; run `cargo update --workspace`");
    }
    let text = std::fs::read_to_string(root.join("CHANGELOG.md")).context("CHANGELOG.md")?;
    let head = format!("## {version} (");
    let start = text
        .find(&head)
        .with_context(|| format!("CHANGELOG.md has no \"{head}...)\" section"))?;
    let body_start = text[start..]
        .find('\n')
        .map_or(text.len(), |i| start + i + 1);
    let end = text[body_start..]
        .find("\n## ")
        .map_or(text.len(), |i| body_start + i);
    let notes = text[body_start..end].trim();
    if notes.is_empty() {
        bail!("the {version} section of CHANGELOG.md is empty");
    }
    std::fs::create_dir_all(root.join("target"))?;
    std::fs::write(root.join("target/release-notes.md"), format!("{notes}\n"))?;
    println!("release {version}: versions and changelog agree");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::next_version;

    #[test]
    fn next_versions() {
        assert_eq!(next_version("0.2.0", "patch").unwrap(), "0.2.1");
        assert_eq!(next_version("0.2.3", "minor").unwrap(), "0.3.0");
        assert_eq!(next_version("0.2.3", "major").unwrap(), "1.0.0");
        assert_eq!(next_version("1.0.0-rc.1", "patch").unwrap(), "1.0.1");
        assert_eq!(next_version("0.2.0", "v1.2.3").unwrap(), "1.2.3");
    }
}
