//! A Typst `World` over in-memory sources, embedded fonts and (optionally) other
//! project files read from disk inside the project root. Packages are not
//! downloaded.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};

static FONTS: LazyLock<(LazyHash<FontBook>, Vec<Font>)> = LazyLock::new(|| {
    let fonts: Vec<Font> = typst_assets::fonts()
        .flat_map(|data| Font::iter(Bytes::new(data)))
        .collect();
    (LazyHash::new(FontBook::from_fonts(&fonts)), fonts)
});

static LIBRARY: LazyLock<LazyHash<Library>> = LazyLock::new(|| LazyHash::new(Library::default()));

/// The project as Typst sees it.
pub struct ProjectWorld {
    files: HashMap<String, String>,
    root: Option<PathBuf>,
    main: FileId,
    ids: Mutex<HashMap<FileId, String>>,
}

impl std::fmt::Debug for ProjectWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectWorld")
            .field("files", &self.files.len())
            .finish_non_exhaustive()
    }
}

fn id_for(rel: &str) -> Result<FileId, String> {
    let vpath = VirtualPath::new(format!("/{}", rel.trim_start_matches('/')))
        .map_err(|e| format!("{rel}: {e:?}"))?;
    Ok(RootedPath::new(VirtualRoot::Project, vpath).intern())
}

impl ProjectWorld {
    pub fn new(
        files: HashMap<String, String>,
        main: &str,
        root: Option<PathBuf>,
    ) -> Result<Self, String> {
        if !files.contains_key(main) {
            return Err(format!("{main} is not among the project files"));
        }
        let main_id = id_for(main)?;
        let mut ids = HashMap::new();
        for k in files.keys() {
            ids.insert(id_for(k)?, k.clone());
        }
        Ok(Self {
            files,
            root,
            main: main_id,
            ids: Mutex::new(ids),
        })
    }

    /// Project-relative path of a file id (project files only).
    pub fn rel_path(&self, id: FileId) -> Option<String> {
        if let Some(p) = self.ids.lock().ok()?.get(&id) {
            return Some(p.clone());
        }
        None
    }

    fn rel_of(&self, id: FileId) -> FileResult<String> {
        let rp = id.get();
        if !matches!(rp.root(), VirtualRoot::Project) {
            return Err(FileError::Other(Some(
                "packages are not available offline in proofback".into(),
            )));
        }
        let p = rp.vpath().get_without_slash().to_owned();
        if p.split('/').any(|c| c == "..") {
            return Err(FileError::AccessDenied);
        }
        if let Ok(mut ids) = self.ids.lock() {
            ids.entry(id).or_insert_with(|| p.clone());
        }
        Ok(p)
    }
}

impl World for ProjectWorld {
    fn library(&self) -> &LazyHash<Library> {
        &LIBRARY
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &FONTS.0
    }

    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        let rel = self.rel_of(id)?;
        match self.files.get(&rel) {
            Some(text) => Ok(Source::new(id, text.clone())),
            None => Err(FileError::NotFound(rel.into())),
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        let rel = self.rel_of(id)?;
        if let Some(text) = self.files.get(&rel) {
            return Ok(Bytes::new(text.clone().into_bytes()));
        }
        let root = self
            .root
            .as_ref()
            .ok_or_else(|| FileError::NotFound(rel.clone().into()))?;
        let path = root.join(&rel);
        let canon = path
            .canonicalize()
            .map_err(|_| FileError::NotFound(rel.clone().into()))?;
        let root_canon = root.canonicalize().map_err(|_| FileError::AccessDenied)?;
        if !canon.starts_with(&root_canon) {
            return Err(FileError::AccessDenied);
        }
        let meta =
            std::fs::metadata(&canon).map_err(|_| FileError::NotFound(rel.clone().into()))?;
        if meta.len() > 64 << 20 {
            return Err(FileError::Other(Some("file too large".into())));
        }
        std::fs::read(&canon)
            .map(Bytes::new)
            .map_err(|_| FileError::NotFound(rel.into()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        FONTS.1.get(index).cloned()
    }

    fn today(&self, _offset: Option<typst::foundations::Duration>) -> Option<Datetime> {
        // Fixed date: compilation is reproducible and needs no clock.
        Datetime::from_ymd(2026, 1, 1)
    }
}
