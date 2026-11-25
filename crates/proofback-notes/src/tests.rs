use super::*;

fn sources() -> Sources {
    Sources::new([
        (
            "main.tex".to_owned(),
            "\\documentclass{report}\n\\begin{document}\n\\chapter{Intro}\nWe study robust thresholds.\n\n\\input{ch2}\n\\appendix\n\\chapter{Extra}\nMore.\n\\end{document}\n".to_owned(),
        ),
        (
            "ch2.tex".to_owned(),
            "\\chapter{Method}\n\\section{Setup}\nThe state of the art is weak.\n\nSecond paragraph here.\n\n\\section*{Unnumbered}\n\\section{Estimating the threshold}\nThe estimator is strong under noise.\nThe estimator is strong again.\n".to_owned(),
        ),
    ])
}

fn ctx(s: &Sources) -> Context<'_> {
    Context {
        sources: s,
        synctex: None,
        project_root: std::path::Path::new("/p"),
        main: "main.tex",
    }
}

#[test]
fn parses_references_and_changes() {
    let notes = parse(
        "Dear author,\n\n- p. 12, para 2: unclear\n- §2.1: 'state of the art' -> 'current'\n- Section 2.2: why this threshold?\n  (it seems arbitrary)\n- Delete \"Second paragraph here\"\n3. insert \"carefully\" after \"We study\"\n> quoted email line\n",
    );
    assert_eq!(notes.len(), 6, "{notes:#?}");
    assert_eq!(notes[1].page, Some(12));
    assert_eq!(notes[1].paragraph, Some(2));
    assert_eq!(notes[2].section.as_deref(), Some("2.1"));
    assert_eq!(notes[2].quote.as_deref(), Some("state of the art"));
    assert_eq!(notes[2].replacement.as_deref(), Some("current"));
    assert!(notes[3].text.contains("arbitrary"), "continuation joined");
    assert!(notes[4].delete);
    assert_eq!(notes[5].insert.as_deref(), Some("carefully"));
    assert_eq!(notes[5].quote.as_deref(), Some("We study"));
}

#[test]
fn section_numbering_follows_inputs_and_appendix() {
    let s = sources();
    let h = headings(&s, "main.tex");
    let nums: Vec<&str> = h.iter().map(|h| h.number.as_str()).collect();
    assert_eq!(nums, ["1", "2", "2.1", "2.2", "A"]);
    assert_eq!(h[3].title, "Estimating the threshold");
}

#[test]
fn locates_quotes_sections_and_paragraphs() {
    let s = sources();
    let notes = parse(
        "- §2.1: 'state of the art' -> 'current'\n- Section 2.2: why?\n- Section 2.1, para 2: too long\n- 'strong under noise' is vague\n- 'The estimator is strong' twice\n- p. 3: something\n",
    );
    let items = locate(&notes, &ctx(&s), "r", false);
    assert_eq!(items[0].kind, Kind::Replace);
    assert_eq!(items[0].anchor.confidence, Confidence::Exact);
    assert_eq!(
        items[0].target.as_ref().unwrap().quote.exact,
        "state of the art"
    );
    assert_eq!(
        items[0]
            .suggestion
            .as_ref()
            .unwrap()
            .replace_with
            .as_deref(),
        Some("current")
    );
    assert_eq!(items[1].anchor.confidence, Confidence::Region);
    assert!(
        items[1]
            .target
            .as_ref()
            .unwrap()
            .quote
            .exact
            .starts_with("\\section{Estimating")
    );
    assert_eq!(items[2].anchor.confidence, Confidence::Line);
    assert_eq!(
        items[2].target.as_ref().unwrap().quote.exact,
        "Second paragraph here."
    );
    assert_eq!(items[3].anchor.confidence, Confidence::Exact);
    // Ambiguous quotes are never guessed.
    assert_eq!(items[4].anchor.confidence, Confidence::Page);
    assert!(items[4].target.is_none());
    // Page without SyncTeX stays page-level.
    assert_eq!(items[5].anchor.confidence, Confidence::Page);
}

#[test]
fn insertion_points_and_ai_cap() {
    let s = sources();
    let notes = parse("insert \"carefully\" after \"We study\"\n");
    let items = locate(&notes, &ctx(&s), "r", false);
    assert_eq!(items[0].kind, Kind::Insert);
    let t = items[0].target.as_ref().unwrap();
    assert_eq!(t.range.start, t.range.end);
    assert_eq!(
        &s.get("main.tex").unwrap().text[..t.range.start]
            .trim_end()
            .rsplit('\n')
            .next()
            .unwrap(),
        &"We study"
    );
    let items = locate(&notes, &ctx(&s), "r", true);
    assert_eq!(items[0].anchor.confidence, Confidence::Page);
    assert_eq!(items[0].anchor.pending, Some(Confidence::Exact));
}

#[test]
fn structured_input_is_validated() {
    let json = r#"[{"line": 1, "text": "x", "page": 0, "section": "1;rm", "quote": "  "}, {"line": 2, "text": "y", "paragraph": 2}]"#;
    let n = parse_structured(json).unwrap();
    assert_eq!(n[0].page, None);
    assert_eq!(n[0].section, None);
    assert_eq!(n[0].quote, None);
    assert_eq!(n[1].paragraph, Some(2));
    assert!(parse_structured("not json").is_err());
}
