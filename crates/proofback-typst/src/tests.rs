use std::collections::HashMap;
use std::path::Path;

pub(crate) fn fixture_files() -> HashMap<String, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/typst-paper");
    let mut files = HashMap::new();
    files.insert(
        "main.typ".to_owned(),
        std::fs::read_to_string(root.join("main.typ")).unwrap(),
    );
    for e in std::fs::read_dir(root.join("sections")).unwrap() {
        let p = e.unwrap().path();
        files.insert(
            format!("sections/{}", p.file_name().unwrap().to_string_lossy()),
            std::fs::read_to_string(&p).unwrap(),
        );
    }
    files
}

#[test]
fn compiles_and_indexes_glyphs_to_source() {
    let files = fixture_files();
    let c = super::compile(&files, "main.typ", None).unwrap();
    assert!(c.document.pages().len() >= 3);
    assert!(c.glyphs.len() > 5000, "{}", c.glyphs.len());
    // Every indexed glyph points at the character that produced it.
    let mut checked = 0;
    for g in c
        .glyphs
        .iter()
        .filter(|g| g.c.is_ascii_alphabetic())
        .take(2000)
    {
        let text = &files[&g.file];
        assert_eq!(text[g.source.clone()].chars().next(), Some(g.c), "{g:?}");
        checked += 1;
    }
    assert!(checked > 1000);
    let pdf = super::pdf(&c).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}

#[test]
fn compile_errors_and_missing_main_are_reported() {
    let mut files = HashMap::new();
    files.insert("main.typ".to_owned(), "#let x = (".to_owned());
    assert!(
        super::compile(&files, "main.typ", None)
            .unwrap_err()
            .contains("Typst compilation failed")
    );
    assert!(super::compile(&files, "other.typ", None).is_err());
    let mut pkg = HashMap::new();
    pkg.insert(
        "main.typ".to_owned(),
        "#import \"@preview/foo:0.1.0\": *".to_owned(),
    );
    assert!(super::compile(&pkg, "main.typ", None).is_err());
}
