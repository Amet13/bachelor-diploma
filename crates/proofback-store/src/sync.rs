//! Carry anchors forward to the current source.

use proofback_anchor::{Sources, make_target};
use proofback_model::{AnchorState, Item, Target};

/// Outcome counts of a sync.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncReport {
    pub anchored: usize,
    pub moved: usize,
    pub changed: usize,
    pub lost: usize,
}

/// Re-anchor every item that has a target. Quote and context first, then position.
pub fn sync_items(items: &mut [Item], sources: &Sources) -> SyncReport {
    let mut report = SyncReport::default();
    for item in items.iter_mut() {
        let Some(target) = item.target.clone() else {
            continue;
        };
        let state = match sources.get(&target.file) {
            None => AnchorState::Lost,
            Some(src) => match relocate(&target, &src.text) {
                Relocated::Same => AnchorState::Anchored,
                Relocated::Moved(r) => {
                    item.target = Some(make_target(&target.file, src, r));
                    AnchorState::Moved
                }
                Relocated::Changed(r) => {
                    item.target = Some(make_target(&target.file, src, r));
                    AnchorState::Changed
                }
                Relocated::Lost => AnchorState::Lost,
            },
        };
        match state {
            AnchorState::Anchored => report.anchored += 1,
            AnchorState::Moved => report.moved += 1,
            AnchorState::Changed => report.changed += 1,
            AnchorState::Lost => report.lost += 1,
        }
        item.anchor.state = state;
    }
    report
}

#[derive(Debug, PartialEq, Eq)]
enum Relocated {
    Same,
    Moved(std::ops::Range<usize>),
    Changed(std::ops::Range<usize>),
    Lost,
}

fn relocate(t: &Target, text: &str) -> Relocated {
    let q = &t.quote;
    let r = t.range.as_range();
    let fits =
        r.end <= text.len() && text.is_char_boundary(r.start) && text.is_char_boundary(r.end);
    if fits
        && text[r.clone()] == q.exact
        && context_score(text, r.clone(), &q.prefix, &q.suffix) > 0
    {
        return Relocated::Same;
    }
    // The quote somewhere else: pick the occurrence whose context matches best.
    if !q.exact.is_empty() {
        let mut best: Option<(usize, std::ops::Range<usize>)> = None;
        for (pos, m) in text.match_indices(q.exact.as_str()) {
            let cand = pos..pos + m.len();
            let score = context_score(text, cand.clone(), &q.prefix, &q.suffix);
            if best.as_ref().is_none_or(|(s, _)| score > *s) {
                best = Some((score, cand));
            }
        }
        let count = text.matches(q.exact.as_str()).count();
        if let Some((score, cand)) = best
            && (count == 1 || score >= 8)
        {
            return if cand == r {
                Relocated::Same
            } else {
                Relocated::Moved(cand)
            };
        }
    } else if !q.prefix.is_empty() || !q.suffix.is_empty() {
        // Insertion point: the context around it.
        let needle = format!("{}{}", q.prefix, q.suffix);
        if let Some(pos) = unique(text, &needle) {
            let at = pos + q.prefix.len();
            return if at == r.start {
                Relocated::Same
            } else {
                Relocated::Moved(at..at)
            };
        }
    }
    // The quote is gone but its surroundings remain: the text was edited.
    let pre_tail = tail(&q.prefix, 16);
    let suf_head = head(&q.suffix, 16);
    if pre_tail.len() >= 6 && suf_head.len() >= 6 {
        // Every place where the old prefix is followed closely by the old suffix;
        // keep the one nearest to where the item used to be.
        let limit = q.exact.len() * 4 + 200;
        let best = text
            .match_indices(pre_tail)
            .filter_map(|(p, m)| {
                let after = p + m.len();
                let s = text[after..].find(suf_head)?;
                (s <= limit).then_some(after..after + s)
            })
            .min_by_key(|c| c.start.abs_diff(r.start));
        if let Some(c) = best {
            return Relocated::Changed(c);
        }
    }
    Relocated::Lost
}

fn unique(text: &str, needle: &str) -> Option<usize> {
    let mut it = text.match_indices(needle);
    let first = it.next()?.0;
    it.next().is_none().then_some(first)
}

fn tail(s: &str, n: usize) -> &str {
    let start = s
        .char_indices()
        .rev()
        .nth(n.saturating_sub(1))
        .map_or(0, |(i, _)| i);
    &s[start..]
}

fn head(s: &str, n: usize) -> &str {
    let end = s.char_indices().nth(n).map_or(s.len(), |(i, _)| i);
    &s[..end]
}

/// How many context characters around `r` agree with the stored prefix/suffix.
fn context_score(text: &str, r: std::ops::Range<usize>, prefix: &str, suffix: &str) -> usize {
    let before = &text[..r.start];
    let after = &text[r.end..];
    let p = prefix
        .chars()
        .rev()
        .zip(before.chars().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let s = suffix
        .chars()
        .zip(after.chars())
        .take_while(|(a, b)| a == b)
        .count();
    // An empty context on both sides (start or end of file) still counts as a match.
    p + s + usize::from(prefix.is_empty() && suffix.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proofback_model::{Anchor, Confidence, ItemId, Kind, Origin, Status};

    fn item_at(sources: &Sources, file: &str, needle: &str) -> Item {
        let src = sources.get(file).unwrap();
        let start = src.text.find(needle).unwrap();
        Item {
            id: ItemId::derive("r", needle),
            round: "r".into(),
            kind: Kind::Comment,
            author: None,
            created: None,
            body: String::new(),
            suggestion: None,
            target: Some(make_target(file, src, start..start + needle.len())),
            anchor: Anchor {
                state: AnchorState::Anchored,
                confidence: Confidence::Exact,
                reason: String::new(),
                pending: None,
            },
            origin: Origin::Notes {
                line: 1,
                text: String::new(),
                page: None,
            },
            status: Status::Open,
            replies: vec![],
            resolution: proofback_model::Resolution::default(),
            thread: vec![],
        }
    }

    fn src(text: &str) -> Sources {
        Sources::new([("a.tex".to_owned(), text.to_owned())])
    }

    #[test]
    fn states() {
        let before = src("Intro.\nThe estimator is strong under heavy-tailed noise.\nEnd.");
        let mut items = vec![item_at(&before, "a.tex", "strong")];

        assert_eq!(sync_items(&mut items, &before).anchored, 1);

        let moved =
            src("New first line.\nIntro.\nThe estimator is strong under heavy-tailed noise.\nEnd.");
        assert_eq!(sync_items(&mut items, &moved).moved, 1);
        let t = items[0].target.as_ref().unwrap();
        assert_eq!(
            &moved.get("a.tex").unwrap().text[t.range.as_range()],
            "strong"
        );
        assert_eq!(t.line, 3);

        let changed =
            src("New first line.\nIntro.\nThe estimator is robust under heavy-tailed noise.\nEnd.");
        assert_eq!(sync_items(&mut items, &changed).changed, 1);
        let t = items[0].target.as_ref().unwrap();
        assert_eq!(
            &changed.get("a.tex").unwrap().text[t.range.as_range()],
            "robust"
        );

        let lost = src("Completely different text.");
        assert_eq!(sync_items(&mut items, &lost).lost, 1);
    }
}
