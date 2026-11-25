use super::*;
use proptest::prelude::*;

fn norm(s: &str) -> String {
    normalize(s, &Options::default()).text()
}

#[test]
fn plain_text_and_whitespace() {
    assert_eq!(norm("Hello   world\nagain"), "Hello world again");
    assert_eq!(norm("  lead\n\n\npara  "), "lead para");
}

#[test]
fn comments_are_removed() {
    assert_eq!(norm("a % note\n   b"), "a b");
    assert_eq!(norm("50\\% sure"), "50% sure");
    assert_eq!(norm("con%\ntinued"), "continued");
}

#[test]
fn formatting_commands_keep_their_text() {
    assert_eq!(
        norm("an \\emph{important} and \\textbf{bold} word"),
        "an important and bold word"
    );
    assert_eq!(
        norm("\\section{Estimating the threshold}\\label{sec:thr}"),
        "Estimating the threshold"
    );
    assert_eq!(norm("\\section*[short]{Long title}"), "Long title");
}

#[test]
fn references_and_citations_become_placeholders() {
    let t = norm("given in Table~\\ref{tab:full} and \\cite[p.~3]{knuth84}.");
    assert_eq!(
        t,
        format!("given in Table {PLACEHOLDER} and {PLACEHOLDER}.")
    );
}

#[test]
fn maths() {
    assert_eq!(norm("at $\\tau = 0.35$."), format!("at {PLACEHOLDER}."));
    assert_eq!(norm("at \\(x\\) here"), format!("at {PLACEHOLDER} here"));
    let n = normalize("before \\[ x^2 \\] after $$y$$ end", &Options::default());
    assert_eq!(n.text(), "before after end");
    assert_eq!(n.blocks().len(), 2);
    let n = normalize(
        "a\n\\begin{equation}\n e=mc^2 \\label{x}\n\\end{equation}\nb",
        &Options::default(),
    );
    assert_eq!(n.text(), "a b");
    assert_eq!(n.blocks()[0].kind, BlockKind::DisplayMath);
}

#[test]
fn quotes_dashes_and_ties() {
    assert_eq!(
        norm("``quoted'' -- range --- dash `single'"),
        "“quoted” – range — dash ‘single’"
    );
    assert_eq!(norm("Dr.~Smith"), "Dr. Smith");
}

#[test]
fn accents_compose() {
    assert_eq!(
        norm("caf\\'e na\\\"ive \\c{c}a \\'{\\i} \\v s"),
        "café naïve ça í š"
    );
}

#[test]
fn symbols() {
    assert_eq!(
        norm("\\LaTeX\\ is fine\\ldots\\ really"),
        "LaTeX is fine… really"
    );
    assert_eq!(norm("\\S 3 and \\& co"), "§3 and & co");
}

#[test]
fn environments() {
    let src = "\\begin{table}[h]\n\\centering\n\\begin{tabular}{lr}\nA & 1 \\\\\n\\end{tabular}\n\\caption{Results.}\\label{t}\n\\end{table}";
    let n = normalize(src, &Options::default());
    assert_eq!(n.text(), "A 1 Results.");
    assert_eq!(n.blocks()[0].kind, BlockKind::Table);
    assert_eq!(
        norm("\\begin{itemize}\n\\item one\n\\item two\n\\end{itemize}"),
        "one two"
    );
    assert_eq!(norm("a \\begin{comment}hidden\\end{comment} b"), "a b");
    let n = normalize(
        "x \\begin{verbatim}\\raw  $code$\\end{verbatim} y",
        &Options::default(),
    );
    assert_eq!(n.text(), "x \\raw $code$ y");
    assert_eq!(norm("say \\verb|\\x{}| now"), "say \\x{} now");
}

#[test]
fn macros_expand_and_unknown_commands_are_placeholders() {
    let pre = "\\newcommand{\\method}{the estimator}\\newcommand\\two[1]{#1#1}\\def\\x{ex}";
    let opts = Options {
        macros: collect_macros(pre),
    };
    assert_eq!(opts.macros.len(), 2);
    let n = normalize("Using \\method{} and \\x.", &opts);
    assert_eq!(n.text(), "Using the estimator and ex.");
    assert_eq!(norm("a \\foo b"), format!("a {PLACEHOLDER}b"));
}

#[test]
fn macro_preamble_does_not_leak_into_text() {
    assert_eq!(norm("\\newcommand{\\method}{the estimator}Text"), "Text");
    assert_eq!(norm("\\def\\x#1{[#1]}Text"), "Text");
}

#[test]
fn offset_map_points_at_source() {
    let src = "The estimator is strong under \\emph{heavy}-tailed noise,";
    let n = normalize(src, &Options::default());
    let text = n.text();
    let start = text.find("strong").unwrap();
    let start_c = text[..start].chars().count();
    let r = n.source_range(start_c..start_c + 6).unwrap();
    assert_eq!(&src[r.clone()], "strong");
    let h = text.find("heavy").unwrap();
    let hc = text[..h].chars().count();
    assert_eq!(&src[n.source_range(hc..hc + 5).unwrap()], "heavy");
    let back = n.chars_for_source(r.clone()).unwrap();
    assert_eq!(back, start_c..start_c + 6);
}

#[test]
fn fold_unifies_typography() {
    let f = fold_str("“Fine” – ﬁt".chars());
    assert_eq!(f.chars.iter().collect::<String>(), "\"fine\" - fit");
    assert_eq!(f.origin[f.origin.len() - 2], 9);
}

#[test]
fn fixture_chapter_normalises() {
    let src = include_str!("../../../fixtures/thesis-basic/chapters/method.tex");
    let pre = include_str!("../../../fixtures/thesis-basic/main.tex");
    let n = normalize(
        src,
        &Options {
            macros: collect_macros(pre),
        },
    );
    let t = n.text();
    assert!(
        t.starts_with("Estimating the threshold The estimator is strong under heavy-tailed noise"),
        "{t}"
    );
    assert!(
        t.contains(
            "Using robust weights—as in the estimator—we avoid “outliers” dominating the fit."
        ),
        "{t}"
    );
    assert!(
        t.contains("Dataset Error Full 0.12 Results on the full dataset."),
        "{t}"
    );
}

/// TeX-like input built from fragments, to exercise odd combinations.
fn texish() -> impl Strategy<Value = String> {
    let frag = prop_oneof![
        Just("\\emph{".to_owned()),
        Just("}".to_owned()),
        Just("{".to_owned()),
        Just("$x$".to_owned()),
        Just("$".to_owned()),
        Just("\\ref{a}".to_owned()),
        Just("% c\n".to_owned()),
        Just("\\'e".to_owned()),
        Just("\\'".to_owned()),
        Just("\\".to_owned()),
        Just("\\begin{table}".to_owned()),
        Just("\\end{table}".to_owned()),
        Just("\\begin{verbatim}".to_owned()),
        Just("\\verb|".to_owned()),
        Just("--".to_owned()),
        Just("``".to_owned()),
        Just("~".to_owned()),
        Just("\\\\[2pt]".to_owned()),
        Just("\\newcommand{\\m}{x}".to_owned()),
        "[a-zA-Z0-9 .,\n]{0,12}",
        "\\PC{0,4}",
    ];
    proptest::collection::vec(frag, 0..40).prop_map(|v| v.concat())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 2000, ..ProptestConfig::default() })]

    #[test]
    fn spans_are_valid_and_ordered(src in texish()) {
        let n = normalize(&src, &Options::default());
        let mut prev_start = 0;
        for i in 0..n.len() {
            let r = n.span(i);
            prop_assert!(r.start <= r.end && r.end <= src.len());
            prop_assert!(src.is_char_boundary(r.start) && src.is_char_boundary(r.end));
            prop_assert!(r.start >= prev_start, "spans must not go backwards");
            prev_start = r.start;
        }
    }

    #[test]
    fn plain_characters_map_to_themselves(src in "[a-zA-Z0-9 .,:;!?()]{0,200}") {
        let n = normalize(&src, &Options::default());
        for (i, c) in n.chars().iter().enumerate() {
            if *c != ' ' {
                let r = n.span(i);
                prop_assert_eq!(&src[r], c.to_string());
            }
        }
    }

    #[test]
    fn fold_origin_is_monotonic(s in "\\PC{0,100}") {
        let f = fold_str(s.chars());
        prop_assert_eq!(f.chars.len(), f.origin.len());
        prop_assert!(f.origin.windows(2).all(|w| w[0] <= w[1]));
    }
}

#[test]
fn regression_verbatim_unicode_space() {
    let src = "$0$\\begin{verbatim}\\'e\u{2000}!";
    let n = normalize(src, &Options::default());
    for i in 0..n.len() {
        let r = n.span(i);
        assert!(
            src.is_char_boundary(r.start) && src.is_char_boundary(r.end),
            "{i} {r:?} {:?}",
            n.chars()[i]
        );
    }
}

/// Found by the fuzzer: a macro that uses itself several times expanded
/// exponentially (4^8 times here) and took tens of seconds on a 500-byte input.
#[test]
fn self_referencing_macros_have_bounded_cost() {
    let src = format!(
        "\\def\\boom{{\\boom\\boom\\boom\\boom{}}}\n{}",
        " filler text".repeat(20),
        "\\boom ".repeat(50)
    );
    let opts = Options {
        macros: collect_macros(&src),
    };
    assert!(
        opts.macros.contains_key("boom"),
        "the test needs the macro to be collected"
    );
    let start = std::time::Instant::now();
    let n = normalize(&src, &opts);
    assert!(
        start.elapsed() < std::time::Duration::from_secs(2),
        "took {:?}",
        start.elapsed()
    );
    // Output is bounded by the expansion budget plus the source itself.
    assert!(n.len() < 64 * 1024 + 5 * src.len());
}
