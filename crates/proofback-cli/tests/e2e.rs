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

fn project_copy() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/thesis-long"),
        dir.path(),
    );
    assert_eq!(run(dir.path(), &["init", "-q"]).0, 0);
    dir
}

#[test]
fn notes_import_locates_quotes_sections_and_pages() {
    let dir = project_copy();
    let d = dir.path();
    std::fs::write(
        d.join("notes.txt"),
        "Dear author, some notes:\n\n- Section 3.1: why this threshold?\n- p. 5, para 1: too long\n- \"the variance of the residuals\" is vague\n- Chapter 2 needs a better ending.\n",
    )
    .unwrap();
    let (code, out, err) = run(d, &["import", "notes.txt", "--round", "n1", "--json"]);
    assert_eq!(code, 0, "{err}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 5, "{out}");
    let by_text = |t: &str| {
        items
            .iter()
            .find(|i| {
                i["untrusted_reviewer_text"]["body"]
                    .as_str()
                    .unwrap()
                    .contains(t)
            })
            .unwrap()
            .clone()
    };
    let sec = by_text("Section 3.1");
    assert_eq!(sec["confidence"], "region");
    assert_eq!(sec["file"], "chapters/ch3-method.tex");
    let para = by_text("p. 5");
    assert_eq!(para["confidence"], "line", "{para}");
    let ch = by_text("Chapter 2");
    assert_eq!(ch["file"], "chapters/ch2-background.tex");
}

#[test]
fn ai_is_off_by_default_and_previews_before_sending() {
    let dir = project_copy();
    let d = dir.path();
    std::fs::write(
        d.join("messy.txt"),
        "the bit about thresholds on the fifth page is unclear",
    )
    .unwrap();
    let (code, _, err) = run(d, &["import", "messy.txt", "--ai"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("AI features are off"));

    // A local stand-in for an OpenAI-compatible endpoint.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let cfg_path = d.join(".proofback/config.toml");
    let cfg = std::fs::read_to_string(&cfg_path)
        .unwrap()
        .replace("[suggest]\nenabled = false", "[suggest]\nenabled = true");
    let cfg = cfg
        .replace("provider = \"\"", "provider = \"ollama\"")
        .replace("model = \"\"", "model = \"test-model\"")
        .replace(
            "endpoint = \"\"",
            &format!("endpoint = \"http://{addr}/v1\""),
        );
    std::fs::write(&cfg_path, cfg).unwrap();

    let (code, out, err) = run(d, &["import", "messy.txt", "--ai"]);
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains("Nothing was sent") && out.contains("chat/completions"),
        "{out}"
    );

    let server = std::thread::spawn(move || {
        use std::io::{Read, Write};
        let (mut s, _) = listener.accept().unwrap();
        let mut buf = vec![0u8; 1 << 16];
        let mut got = Vec::new();
        loop {
            let n = s.read(&mut buf).unwrap();
            got.extend_from_slice(&buf[..n]);
            let t = String::from_utf8_lossy(&got).to_string();
            if let Some(i) = t.find("\r\n\r\n") {
                let len = t
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .map(|v| v.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                if got.len() >= i + 4 + len {
                    break;
                }
            }
            if n == 0 {
                break;
            }
        }
        let content = r#"[{\"line\": 1, \"text\": \"the bit about thresholds is unclear\", \"page\": 5, \"paragraph\": 1}]"#;
        let body = format!(r#"{{"choices":[{{"message":{{"content":"{content}"}}}}]}}"#);
        let resp = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        s.write_all(resp.as_bytes()).unwrap();
    });
    let (code, out, err) = run(
        d,
        &[
            "import",
            "messy.txt",
            "--ai",
            "--consent",
            "--round",
            "ai1",
            "--json",
        ],
    );
    server.join().unwrap();
    assert_eq!(code, 0, "{err}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let item = &v["items"][0];
    assert_eq!(
        item["confidence"], "page",
        "AI-structured notes start at page: {item}"
    );
    assert!(
        std::fs::read_to_string(&cfg_path)
            .unwrap()
            .contains("consent = true")
    );
    let id = item["id"].as_str().unwrap().to_owned();
    let (code, out, err) = run(d, &["confirm", &id]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("confirmed (line)"), "{out}");
}

#[test]
fn typst_project_import_uses_the_glyph_index() {
    let Ok(pdfium) = proofback_pdf::load_pdfium(None) else {
        assert!(
            std::env::var_os("PROOFBACK_REQUIRE_PDFIUM").is_none(),
            "PDFium required"
        );
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/typst-paper"),
        d,
    );
    let mut files = std::collections::HashMap::new();
    files.insert(
        "main.typ".to_owned(),
        std::fs::read_to_string(d.join("main.typ")).unwrap(),
    );
    for e in std::fs::read_dir(d.join("sections")).unwrap() {
        let p = e.unwrap().path();
        files.insert(
            format!("sections/{}", p.file_name().unwrap().to_string_lossy()),
            std::fs::read_to_string(&p).unwrap(),
        );
    }
    let compiled = proofback_typst::compile(&files, "main.typ", None).unwrap();
    let pdf = proofback_typst::pdf(&compiled).unwrap();
    // Highlight "Each sensor supports" wherever it appears in the PDF.
    let ex = proofback_pdf::TextExtractor::open(&pdfium, &pdf).unwrap();
    let phrase = "Each sensor supports";
    let (page, chars) = (1..=ex.page_count())
        .find_map(|p| {
            let t = ex.page(p).unwrap();
            let s: String = t.chars.iter().map(|c| c.c).collect();
            s.find(phrase).map(|b| {
                (
                    p,
                    t.chars[s[..b].chars().count()..s[..b].chars().count() + phrase.len()].to_vec(),
                )
            })
        })
        .expect("phrase in PDF");
    let bb = chars
        .iter()
        .fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, c| {
            [
                b[0].min(c.bbox[0]),
                b[1].min(c.bbox[1]),
                b[2].max(c.bbox[2]),
                b[3].max(c.bbox[3]),
            ]
        });
    let mut spec = proofback_pdf::AnnotationSpec::new(page, "StrikeOut", bb);
    spec.quads = vec![[bb[0], bb[3], bb[2], bb[3], bb[0], bb[1], bb[2], bb[1]]];
    spec.contents = Some("Replace with \u{201c}Every sensor supports\u{201d}".into());
    std::fs::write(
        d.join("review.pdf"),
        proofback_pdf::write_annotations(&pdf, &[spec]).unwrap(),
    )
    .unwrap();
    assert_eq!(run(d, &["init", "-q", "--main", "main.typ"]).0, 0);
    let (code, out, err) = run(d, &["import", "review.pdf", "--json", "--round", "t1"]);
    assert_eq!(code, 0, "{err}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let it = &v["items"][0];
    assert_eq!(it["confidence"], "exact", "{it}");
    assert_eq!(it["kind"], "replace");
    let (code, diff, _) = run(d, &["patch"]);
    assert_eq!(code, 0);
    assert!(diff.contains("+Every sensor supports"), "{diff}");
}
