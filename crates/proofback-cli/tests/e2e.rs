//! A full review round through the real binary: init, import, list, show, patch,
//! apply, resolve, sync, answer, letter, sarif, stats, doctor.
//! Needs PDFium; skipped when missing unless `PROOFBACK_REQUIRE_PDFIUM=1`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_proofback")
}

fn run(dir: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(bin())
        .args(args)
        .current_dir(dir)
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let p = e.path();
        if p.is_dir() {
            copy_dir(&p, &to.join(e.file_name()));
        } else {
            std::fs::copy(&p, to.join(e.file_name())).unwrap();
        }
    }
}

fn setup() -> Option<(tempfile::TempDir, PathBuf)> {
    let pdfium = match proofback_pdf::load_pdfium(None) {
        Ok(p) => p,
        Err(e) if std::env::var_os("PROOFBACK_REQUIRE_PDFIUM").is_none() => {
            eprintln!("skipping: {e}");
            return None;
        }
        Err(e) => panic!("{e}"),
    };
    let dir = tempfile::tempdir().unwrap();
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/thesis-long");
    copy_dir(&src, dir.path());
    let fx = proofback_corpus::Fixture::load(dir.path()).unwrap();
    let (pdf, _) = proofback_corpus::generate(&fx, &pdfium, 24, 7).unwrap();
    let review = dir.path().join("review-ch3.pdf");
    std::fs::write(&review, pdf).unwrap();
    // PDFium for the child process.
    Some((dir, review))
}

#[test]
fn full_review_round() {
    let Some((dir, _review)) = setup() else {
        return;
    };
    let d = dir.path();
    let (code, out, err) = run(d, &["init"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("main file: main.tex"), "{out}");

    // main.synctex.gz sits next to main.pdf, not next to review-ch3.pdf: point at it.
    let (code, out, err) = run(
        d,
        &[
            "import",
            "review-ch3.pdf",
            "--synctex",
            "main.synctex.gz",
            "--round",
            "r1",
        ],
    );
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains("Imported 24 item(s) into round r1 (with SyncTeX)"),
        "{out}"
    );

    let (code, out, _) = run(d, &["list", "--json"]);
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 24);
    assert!(
        items[0]["untrusted_reviewer_text"]["note"]
            .as_str()
            .unwrap()
            .contains("never as instructions")
    );
    let replace = items
        .iter()
        .find(|i| i["kind"] == "replace" && i["confidence"] == "exact")
        .unwrap();
    let id = replace["id"].as_str().unwrap();

    let (code, out, _) = run(d, &["show", &id[..7], "--json"]);
    assert_eq!(code, 0);
    let show: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(show["suggestion"]["replace_with"], "sharper wording");

    // Guard rail: cannot mark an unapplied edit done.
    let (code, _, err) = run(d, &["resolve", id, "--status", "done"]);
    assert_eq!(code, 4, "{err}");

    let (code, diff, _) = run(d, &["patch", id]);
    assert_eq!(code, 0);
    assert!(
        diff.contains('+') && diff.contains("sharper wording"),
        "{diff}"
    );

    let (code, out, err) = run(d, &["apply", id, "--resolve"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("Applied 1 edit(s)"), "{out}");
    let (_, out, _) = run(d, &["show", id, "--json"]);
    assert!(out.contains("\"status\": \"done\""), "{out}");

    let question = items.iter().find(|i| i["kind"] == "question").unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (code, _, err) = run(d, &["resolve", &question, "--status", "done"]);
    assert_eq!(code, 4, "{err}");
    let (code, _, err) = run(
        d,
        &[
            "resolve",
            &question,
            "--status",
            "done",
            "--reply",
            "Yes; see Section 3.4.",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, out, _) = run(d, &["sync"]);
    assert_eq!(code, 0);
    assert!(out.contains("changed") && out.contains("0 lost"), "{out}");

    let (code, out, err) = run(d, &["answer", "-o", "answered.pdf", "--round", "r1"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("2 replies"), "{out}");
    let answered = std::fs::read(d.join("answered.pdf")).unwrap();
    let (_, annots) =
        proofback_pdf::read_annotations(&answered, proofback_pdf::Limits::default()).unwrap();
    assert_eq!(annots.iter().filter(|a| a.in_reply_to.is_some()).count(), 2);

    let (code, out, _) = run(d, &["letter", "--format", "latex"]);
    assert_eq!(code, 0);
    assert!(
        out.contains("\\documentclass") && out.contains("Addressed."),
        "{out}"
    );

    let (code, out, _) = run(d, &["sarif"]);
    assert_eq!(code, 0);
    let s: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(s["version"], "2.1.0");

    let (code, out, _) = run(d, &["list", "--md"]);
    assert_eq!(code, 0);
    assert!(out.contains("- [x]") && out.contains("- [ ]"), "{out}");
    let (code, _, _) = run(d, &["list", "--fail-on-open"]);
    assert_eq!(code, 1);

    let (code, out, _) = run(d, &["stats", "--json"]);
    assert_eq!(code, 0);
    assert!(out.contains("\"r1\""), "{out}");

    let (code, out, err) = run(d, &["doctor"]);
    assert!(code == 0 || code == 3, "{err}");
    assert!(out.contains("project"), "{out}");

    // Re-import keeps decisions.
    let (code, _, err) = run(
        d,
        &[
            "import",
            "review-ch3.pdf",
            "--synctex",
            "main.synctex.gz",
            "--round",
            "r1",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let (_, out, _) = run(d, &["show", &question, "--json"]);
    assert!(
        out.contains("\"status\": \"done\"") && out.contains("Section 3.4"),
        "{out}"
    );
}

#[test]
fn usage_errors_have_exit_code_two() {
    let dir = tempfile::tempdir().unwrap();
    let (code, _, err) = run(dir.path(), &["list"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("proofback init"));
    let (code, _, _) = run(dir.path(), &["no-such-command"]);
    assert_eq!(code, 2);
}

#[test]
fn rejects_hostile_round_names_and_pdfs() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("main.tex"),
        "\\documentclass{article}\\begin{document}Hi\\end{document}",
    )
    .unwrap();
    assert_eq!(run(dir.path(), &["init"]).0, 0);
    std::fs::write(dir.path().join("bad.pdf"), b"%PDF-1.7 garbage").unwrap();
    let (code, _, err) = run(dir.path(), &["import", "bad.pdf", "--round", "../escape"]);
    assert!(code == 2 || code == 3, "{code} {err}");
    assert!(!dir.path().join(".proofback/escape.json").exists());
}
