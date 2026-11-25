//! Git plumbing, always called with explicit arguments and never through a shell.

use std::path::Path;
use std::process::Command;

use proofback_store::{Error, Result};

/// A revision argument that cannot be mistaken for an option.
pub fn validate_rev(rev: &str) -> Result<()> {
    let ok = !rev.is_empty()
        && !rev.starts_with('-')
        && rev.len() <= 200
        && rev.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'/' | b'^' | b'~' | b'-' | b'@')
        });
    if ok {
        Ok(())
    } else {
        Err(Error::Usage(format!("{rev:?} is not a valid git revision")))
    }
}

fn git(root: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout)
}

/// Current commit, if the project is a git repository.
pub fn head(root: &Path) -> Option<String> {
    let out = git(root, &["rev-parse", "--verify", "--quiet", "HEAD"])?;
    Some(String::from_utf8_lossy(&out).trim().to_owned()).filter(|s| !s.is_empty())
}

/// All `.tex` files at `rev`, relative to the project root.
pub fn tex_files_at(root: &Path, rev: &str) -> Result<Vec<(String, String)>> {
    validate_rev(rev)?;
    let commit = git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{rev}^{{commit}}"),
        ],
    )
    .ok_or_else(|| Error::Usage(format!("{rev} is not a commit in this repository")))?;
    let commit = String::from_utf8_lossy(&commit).trim().to_owned();
    let prefix = git(root, &["rev-parse", "--show-prefix"])
        .map(|p| String::from_utf8_lossy(&p).trim().to_owned())
        .unwrap_or_default();
    let list = git(
        root,
        &["ls-tree", "-r", "-z", "--name-only", &commit, "--", "."],
    )
    .ok_or_else(|| Error::Input(format!("cannot list files at {rev}")))?;
    let mut out = Vec::new();
    for name in list.split(|b| *b == 0).filter(|n| !n.is_empty()) {
        let name = String::from_utf8_lossy(name).into_owned();
        let ext = Path::new(&name)
            .extension()
            .map(std::ffi::OsStr::to_ascii_lowercase);
        if !ext.is_some_and(|e| e == "tex" || e == "ltx") {
            continue;
        }
        let rel = name.strip_prefix(&prefix).unwrap_or(&name).to_owned();
        if rel.split('/').any(|c| c.starts_with('.')) {
            continue;
        }
        let blob = git(root, &["show", &format!("{commit}:{name}")])
            .ok_or_else(|| Error::Input(format!("cannot read {name} at {rev}")))?;
        if let Ok(text) = String::from_utf8(blob) {
            out.push((rel, text));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revisions_are_validated() {
        assert!(validate_rev("HEAD~2").is_ok());
        assert!(validate_rev("v1.0").is_ok());
        assert!(validate_rev("a1b2c3").is_ok());
        assert!(validate_rev("--output=/tmp/x").is_err());
        assert!(validate_rev("main;rm -rf").is_err());
        assert!(validate_rev("").is_err());
    }
}
