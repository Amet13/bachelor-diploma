//! Notes corpus (plan section 16, M6): notes written the way reviewers write them,
//! with ground truth from unique phrases and the document's section numbering.
//! Gate: wrong-anchor rate under 1%.

use std::path::Path;

use proofback_corpus::{Fixture, Rng};
use proofback_model::Confidence;
use proofback_notes::{Context, headings, locate, parse};
use proofback_tex::fold_str;

#[test]
fn notes_wrong_anchor_rate_under_one_percent() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/thesis-long");
    let fx = Fixture::load(&root).unwrap();
    let heads = headings(&fx.sources, "main.tex");
    assert!(heads.len() > 10);
    let mut rng = Rng::new(5);
    let mut lines = Vec::new();
    let mut truth = Vec::new(); // (file, Some(range) for quotes | None, section index)
    let files: Vec<(String, String)> = fx
        .sources
        .files()
        .map(|(f, s)| (f.to_owned(), s.text.clone()))
        .collect();
    let mut attempts = 0;
    while truth.len() < 150 && attempts < 20_000 {
        attempts += 1;
        if truth.len() % 3 == 2 {
            let i = rng.below(heads.len());
            lines.push(format!(
                "- Section {}: this part needs work.",
                heads[i].number
            ));
            truth.push((heads[i].file.clone(), None, Some(i)));
            continue;
        }
        let (file, text) = &files[rng.below(files.len())];
        let words: Vec<&str> = text
            .split_whitespace()
            .filter(|w| w.chars().all(char::is_alphabetic))
            .collect();
        if words.len() < 10 {
            continue;
        }
        let start = rng.below(words.len() - 4);
        let n = 3 + rng.below(3);
        let phrase = words[start..start + n].join(" ");
        let folded: String = fold_str(phrase.chars()).chars.iter().collect();
        let hits: usize = fx
            .sources
            .files()
            .map(|(_, s)| s.find_folded(&folded).len())
            .sum();
        if hits != 1 {
            continue;
        }
        let src = fx.sources.get(file).unwrap();
        let Some(r) = src
            .find_folded(&folded)
            .first()
            .and_then(|r| src.source_range_for_folded(r.clone()))
        else {
            continue;
        };
        let style = truth.len() % 4;
        lines.push(match style {
            0 => format!("- \"{phrase}\" is unclear"),
            1 => format!("- p. 3: “{phrase}” -> “something else”"),
            2 => format!("- delete '{phrase}'"),
            _ => format!("- Why “{phrase}”?"),
        });
        truth.push((file.clone(), Some(r), None));
    }
    let notes = parse(&lines.join("\n"));
    assert_eq!(notes.len(), truth.len());
    let ctx = Context {
        sources: &fx.sources,
        synctex: Some(&fx.synctex),
        project_root: &fx.root,
        main: "main.tex",
    };
    let items = locate(&notes, &ctx, "notes", false);
    let (mut exact, mut region, mut wrong, mut page) = (0, 0, 0, 0);
    for (it, (file, range, sec)) in items.iter().zip(&truth) {
        let t = it.target.as_ref();
        match it.anchor.confidence {
            Confidence::Page => page += 1,
            _ if t.is_none_or(|t| &t.file != file) => wrong += 1,
            Confidence::Exact
                if range
                    .as_ref()
                    .is_some_and(|r| t.unwrap().range.as_range() == *r) =>
            {
                exact += 1;
            }
            Confidence::Region
                if sec.is_some_and(|i| t.unwrap().range.start == heads[i].offset) =>
            {
                region += 1;
            }
            _ => wrong += 1,
        }
    }
    let n = truth.len();
    eprintln!("notes corpus n={n}: exact {exact}, region {region}, page {page}, wrong {wrong}");
    assert!(wrong * 100 < n, "wrong-anchor rate {wrong}/{n}");
    assert!(
        exact + region >= n * 9 / 10,
        "too few located: {exact} + {region} of {n}"
    );
}
