//! Project state on disk: configuration, review rounds, sources, and the
//! operations that change them (resolve, patch, apply, sync).
//!
//! Everything lives in `.proofback/` inside the author's project and is meant to be
//! committed, except `.proofback/cache/`.

mod config;
pub mod edit;
pub mod sync;
pub mod views;

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use proofback_anchor::Sources;
use proofback_model::{Item, ItemId, RoundFile, resolve_prefix};

pub use config::{Config, ImportConfig, LimitsConfig, SuggestConfig};

/// Name of the state directory.
pub const STATE_DIR: &str = ".proofback";

/// Errors, each mapped to a CLI exit code (see [`Error::exit_code`]).
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Usage(String),
    #[error("{0}")]
    Input(String),
    #[error("refused: {0}")]
    Refused(String),
    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl Error {
    /// Exit codes from the plan: 2 usage, 3 input, 4 refused write.
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::Usage(_) => 2,
            Error::Input(_) | Error::Io { .. } => 3,
            Error::Refused(_) => 4,
        }
    }

    pub(crate) fn io(path: &Path, source: std::io::Error) -> Self {
        Error::Io {
            path: path.to_path_buf(),
            source,
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// A proofback project rooted at the directory that contains `.proofback/`.
#[derive(Debug, Clone)]
pub struct Project {
    pub root: PathBuf,
    pub config: Config,
}

impl Project {
    /// Find the project containing `start` by walking up to a `.proofback/` directory.
    pub fn discover(start: &Path) -> Result<Self> {
        let start = start.canonicalize().map_err(|e| Error::io(start, e))?;
        let mut dir = Some(start.as_path());
        while let Some(d) = dir {
            if d.join(STATE_DIR).join("config.toml").is_file() {
                return Self::open(d);
            }
            dir = d.parent();
        }
        Err(Error::Usage(format!(
            "no proofback project found in {} or its parents; run `proofback init` first",
            start.display()
        )))
    }

    /// Open a project whose root is known.
    pub fn open(root: &Path) -> Result<Self> {
        let root = root.canonicalize().map_err(|e| Error::io(root, e))?;
        let path = root.join(STATE_DIR).join("config.toml");
        let text = std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
        let config =
            Config::parse(&text).map_err(|e| Error::Input(format!("{}: {e}", path.display())))?;
        Ok(Self { root, config })
    }

    /// Create `.proofback/` with a default configuration. Existing files are kept.
    pub fn init(root: &Path, main: Option<&str>) -> Result<Self> {
        let root = root.canonicalize().map_err(|e| Error::io(root, e))?;
        let state = root.join(STATE_DIR);
        for d in [state.clone(), state.join("rounds"), state.join("cache")] {
            std::fs::create_dir_all(&d).map_err(|e| Error::io(&d, e))?;
        }
        let ignore = state.join("cache").join(".gitignore");
        if !ignore.exists() {
            write_atomic(&ignore, b"*\n")?;
        }
        let cfg_path = state.join("config.toml");
        if !cfg_path.exists() {
            let main = main.map_or_else(|| guess_main(&root), str::to_owned);
            let cfg = Config {
                main,
                ..Config::default()
            };
            write_atomic(&cfg_path, cfg.to_toml().as_bytes())?;
        }
        Self::open(&root)
    }

    pub fn state_dir(&self) -> PathBuf {
        self.root.join(STATE_DIR)
    }

    pub fn rounds_dir(&self) -> PathBuf {
        self.state_dir().join("rounds")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.state_dir().join("cache")
    }

    /// Directory with build outputs (PDF, SyncTeX).
    pub fn build_dir(&self) -> PathBuf {
        self.root.join(&self.config.build_dir)
    }

    /// Load every `.tex` file under the root, skipping hidden directories, the
    /// state directory and symbolic links. Non-UTF-8 files are reported, not loaded.
    pub fn load_sources(&self) -> Result<(Sources, Vec<String>)> {
        let mut files = Vec::new();
        let mut skipped = Vec::new();
        let max = self.config.limits.max_source_mb * 1024 * 1024;
        collect(&self.root, &self.root, &mut files, &mut skipped, max)?;
        Ok((Sources::new(files), skipped))
    }

    /// Load all rounds, sorted by name.
    pub fn rounds(&self) -> Result<Vec<RoundFile>> {
        let dir = self.rounds_dir();
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Ok(out);
        };
        let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        paths.sort();
        for p in paths
            .into_iter()
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
        {
            let text = std::fs::read_to_string(&p).map_err(|e| Error::io(&p, e))?;
            let rf = RoundFile::from_json(&text)
                .map_err(|e| Error::Input(format!("{}: {e}", p.display())))?;
            out.push(rf);
        }
        Ok(out)
    }

    /// Path of a round file. Round names are restricted to a safe character set.
    pub fn round_path(&self, name: &str) -> Result<PathBuf> {
        if !valid_round_name(name) {
            return Err(Error::Usage(format!(
                "invalid round name {name:?}: use letters, digits, '-', '_' and '.' only"
            )));
        }
        Ok(self.rounds_dir().join(format!("{name}.json")))
    }

    /// Write a round deterministically.
    pub fn save_round(&self, round: &RoundFile) -> Result<()> {
        let path = self.round_path(&round.round.name)?;
        write_atomic(&path, round.to_json().as_bytes())
    }

    /// Resolve an id or unique prefix across all rounds.
    pub fn find_item<'a>(&self, rounds: &'a [RoundFile], query: &str) -> Result<(usize, &'a Item)> {
        let ids: Vec<&ItemId> = rounds
            .iter()
            .flat_map(|r| r.items.iter().map(|i| &i.id))
            .collect();
        let id =
            resolve_prefix(ids.iter().copied(), query).map_err(|e| Error::Usage(e.to_string()))?;
        for (ri, r) in rounds.iter().enumerate() {
            if let Some(it) = r.item(id) {
                return Ok((ri, it));
            }
        }
        Err(Error::Usage(format!("no item matches {query:?}")))
    }

    /// Resolve a path inside the project, refusing anything that escapes the root
    /// (`..`, absolute paths, or symbolic links pointing outside).
    pub fn safe_path(&self, rel: &str) -> Result<PathBuf> {
        let rel_path = Path::new(rel);
        if rel_path
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        {
            return Err(Error::Refused(format!(
                "{rel:?} is not a path inside the project"
            )));
        }
        let full = self.root.join(rel_path);
        if let Ok(meta) = std::fs::symlink_metadata(&full) {
            if meta.file_type().is_symlink() {
                return Err(Error::Refused(format!("{rel:?} is a symbolic link")));
            }
        }
        if let Ok(canon) = full.canonicalize() {
            if !canon.starts_with(&self.root) {
                return Err(Error::Refused(format!(
                    "{rel:?} resolves outside the project"
                )));
            }
        }
        Ok(full)
    }
}

/// Round names: `[A-Za-z0-9._-]`, 1-100 characters, not starting with a dot.
pub fn valid_round_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// Turn arbitrary text into a valid round-name fragment.
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_matches('-').chars().take(60).collect()
}

fn guess_main(root: &Path) -> String {
    let Ok(entries) = std::fs::read_dir(root) else {
        return "main.tex".into();
    };
    let mut candidates: Vec<String> = entries
        .filter_map(std::result::Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "tex"))
        .filter(|e| std::fs::read_to_string(e.path()).is_ok_and(|t| t.contains("\\documentclass")))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    candidates.sort();
    if candidates.iter().any(|c| c == "main.tex") {
        return "main.tex".into();
    }
    candidates
        .into_iter()
        .next()
        .unwrap_or_else(|| "main.tex".into())
}

fn collect(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(String, String)>,
    skipped: &mut Vec<String>,
    max: u64,
) -> Result<()> {
    let entries = std::fs::read_dir(dir).map_err(|e| Error::io(dir, e))?;
    let mut entries: Vec<_> = entries.filter_map(std::result::Result::ok).collect();
    entries.sort_by_key(std::fs::DirEntry::path);
    for e in entries {
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_symlink() || name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }
        let path = e.path();
        if ft.is_dir() {
            collect(root, &path, out, skipped, max)?;
        } else if path.extension().is_some_and(|x| x == "tex" || x == "ltx") {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let size = e.metadata().map_or(0, |m| m.len());
            if size > max {
                skipped.push(format!("{rel} (larger than the source size limit)"));
                continue;
            }
            let bytes = std::fs::read(&path).map_err(|err| Error::io(&path, err))?;
            match String::from_utf8(bytes) {
                Ok(text) => out.push((rel, text)),
                Err(_) => skipped.push(format!(
                    "{rel} (not UTF-8; convert it to UTF-8 to use it with proofback)"
                )),
            }
        }
    }
    Ok(())
}

/// Write a file atomically: write a sibling temporary file, then rename.
pub fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    let tmp = dir.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        std::process::id()
    ));
    std::fs::write(&tmp, data).map_err(|e| Error::io(&tmp, e))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        Error::io(path, e)
    })
}

/// SHA-256 of bytes as lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Merge freshly imported items into an existing round: the author's status,
/// replies and resolution survive a re-import of the same review.
pub fn merge_reimport(existing: Option<&RoundFile>, mut fresh: Vec<Item>) -> Vec<Item> {
    let Some(old) = existing else { return fresh };
    let by_id: BTreeMap<&ItemId, &Item> = old.items.iter().map(|i| (&i.id, i)).collect();
    for it in &mut fresh {
        if let Some(prev) = by_id.get(&it.id) {
            it.status = prev.status;
            it.replies.clone_from(&prev.replies);
            it.resolution = prev.resolution.clone();
        }
    }
    fresh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_names() {
        assert!(valid_round_name("2026-11-25-supervisor"));
        assert!(!valid_round_name("../evil"));
        assert!(!valid_round_name(".hidden"));
        assert!(!valid_round_name("a/b"));
        assert_eq!(slug("Review ch3 (final).pdf"), "review-ch3-final-pdf");
    }

    #[test]
    fn init_discover_and_safe_paths() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("thesis.tex"), "\\documentclass{report}").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        let p = Project::init(dir.path(), None).unwrap();
        assert_eq!(p.config.main, "thesis.tex");
        let found = Project::discover(&dir.path().join("sub")).unwrap();
        assert_eq!(found.root, p.root);
        assert!(p.safe_path("thesis.tex").is_ok());
        assert!(matches!(p.safe_path("../x.tex"), Err(Error::Refused(_))));
        assert!(matches!(p.safe_path("/etc/passwd"), Err(Error::Refused(_))));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc/hostname", dir.path().join("link.tex")).unwrap();
            assert!(matches!(p.safe_path("link.tex"), Err(Error::Refused(_))));
            let (sources, _) = p.load_sources().unwrap();
            assert!(sources.get("link.tex").is_none());
        }
        assert!(Project::discover(Path::new("/")).is_err());
    }

    #[test]
    fn non_utf8_sources_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.tex"), "ok").unwrap();
        std::fs::write(dir.path().join("latin1.tex"), [0x63, 0x61, 0x66, 0xe9]).unwrap();
        let p = Project::init(dir.path(), Some("main.tex")).unwrap();
        let (sources, skipped) = p.load_sources().unwrap();
        assert!(sources.get("main.tex").is_some());
        assert_eq!(skipped.len(), 1);
    }
}
