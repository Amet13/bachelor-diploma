//! Turning items into edits: patches, applying them, and resolving items.

use std::collections::BTreeMap;

use proofback_anchor::Sources;
use proofback_model::{Item, Kind, Reply, Status};

use crate::{Error, Project, Result};

/// One planned change to a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub file: String,
    pub range: std::ops::Range<usize>,
    pub replacement: String,
    pub item: String,
}

/// The edit an item asks for, checked against the current source.
///
/// Fails when the item is not a mechanical edit, has no exact target, or the
/// anchored text changed since import (run `sync` first).
pub fn edit_for(item: &Item, sources: &Sources) -> Result<Edit> {
    let t = item
        .target
        .as_ref()
        .ok_or_else(|| Error::Refused(format!("{} has no source location", item.id)))?;
    if item.anchor.confidence != proofback_model::Confidence::Exact {
        return Err(Error::Refused(format!(
            "{} is only located to the {}; edit it by hand",
            item.id,
            item.anchor.confidence.as_str()
        )));
    }
    let src = sources
        .get(&t.file)
        .ok_or_else(|| Error::Refused(format!("{} is not a loaded source file", t.file)))?;
    let r = t.range.as_range();
    let current = src.text.get(r.clone()).ok_or_else(|| {
        Error::Refused(format!(
            "{}: range is out of date; run `proofback sync`",
            item.id
        ))
    })?;
    if current != t.quote.exact {
        return Err(Error::Refused(format!(
            "{}: the anchored text changed since import; run `proofback sync`",
            item.id
        )));
    }
    let sug = item.suggestion.as_ref();
    let (range, replacement) = match item.kind {
        Kind::Replace => {
            let with = sug
                .and_then(|s| s.replace_with.clone())
                .ok_or_else(|| Error::Refused(format!("{} has no replacement text", item.id)))?;
            (r, with)
        }
        Kind::Delete => (widen_for_delete(&src.text, r), String::new()),
        Kind::Insert => {
            let ins = sug
                .and_then(|s| s.insert.clone())
                .ok_or_else(|| Error::Refused(format!("{} has no text to insert", item.id)))?;
            let prev = src.text[..r.start].chars().next_back();
            let next = src.text[r.start..].chars().next();
            let mut text = String::new();
            if prev.is_some_and(|c| !c.is_whitespace() && !"([{`~".contains(c))
                && ins.starts_with(char::is_alphanumeric)
            {
                text.push(' ');
            }
            text.push_str(&ins);
            if next.is_some_and(char::is_alphanumeric) && ins.ends_with(char::is_alphanumeric) {
                text.push(' ');
            }
            (r.start..r.start, text)
        }
        other => {
            return Err(Error::Refused(format!(
                "{} is a {}; only replace, delete and insert items become patches",
                item.id,
                other.as_str()
            )));
        }
    };
    Ok(Edit {
        file: t.file.clone(),
        range,
        replacement,
        item: item.id.to_string(),
    })
}

/// Deleting a word should not leave a double space behind.
fn widen_for_delete(text: &str, r: std::ops::Range<usize>) -> std::ops::Range<usize> {
    let before = text[..r.start].chars().next_back();
    let after = text[r.end..].chars().next();
    match (before, after) {
        (Some(' '), Some(' ' | '\n' | '.' | ',' | ';' | ':') | None) => r.start - 1..r.end,
        (None | Some('\n'), Some(' ')) => r.start..r.end + 1,
        _ => r,
    }
}

/// File path to (old contents, new contents).
pub type Changes = BTreeMap<String, (String, String)>;

/// New contents of each touched file after applying `edits`. Overlapping edits are
/// refused.
pub fn apply_in_memory(edits: &[Edit], sources: &Sources) -> Result<Changes> {
    let mut by_file: BTreeMap<String, Vec<&Edit>> = BTreeMap::new();
    for e in edits {
        by_file.entry(e.file.clone()).or_default().push(e);
    }
    let mut out = BTreeMap::new();
    for (file, mut es) in by_file {
        let old = sources
            .get(&file)
            .ok_or_else(|| Error::Refused(format!("{file} is not loaded")))?
            .text
            .clone();
        es.sort_by_key(|e| std::cmp::Reverse((e.range.start, e.range.end)));
        for w in es.windows(2) {
            if w[1].range.end > w[0].range.start || (w[1].range == w[0].range) {
                return Err(Error::Refused(format!(
                    "{} and {} overlap",
                    w[0].item, w[1].item
                )));
            }
        }
        let mut new = old.clone();
        for e in es {
            new.replace_range(e.range.clone(), &e.replacement);
        }
        out.insert(file, (old, new));
    }
    Ok(out)
}

/// A unified diff (`git apply` format) for the given file changes.
pub fn unified_diff(changes: &Changes) -> String {
    let mut out = String::new();
    for (file, (old, new)) in changes {
        let diff = similar::TextDiff::from_lines(old, new);
        let a = format!("a/{file}");
        let b = format!("b/{file}");
        out.push_str(
            &diff
                .unified_diff()
                .context_radius(3)
                .header(&a, &b)
                .to_string(),
        );
    }
    out
}

/// Write file changes to disk, atomically, refusing anything outside the project.
pub fn write_changes(
    project: &Project,
    changes: &BTreeMap<String, (String, String)>,
) -> Result<()> {
    for (file, (old, new)) in changes {
        let path = project.safe_path(file)?;
        let on_disk = std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
        if &on_disk != old {
            return Err(Error::Refused(format!(
                "{file} changed on disk while proofback was working; try again"
            )));
        }
        crate::write_atomic(&path, new.as_bytes())?;
    }
    Ok(())
}

/// Options for [`resolve`].
#[derive(Debug, Clone, Default)]
pub struct ResolveOptions {
    pub status: Option<Status>,
    pub reply: Option<String>,
    pub author: String,
    pub now: String,
    /// Allow `done` although the anchored text is unchanged (human decision).
    pub force: bool,
    pub commit: Option<String>,
}

/// Update an item's status and replies, enforcing the guard rails from the plan:
/// - `done` on replace/delete/insert needs the anchored text to have changed;
/// - `done` on a comment or question needs a reply.
pub fn resolve(item: &mut Item, sources: &Sources, opts: &ResolveOptions) -> Result<()> {
    if let Some(body) = opts
        .reply
        .as_ref()
        .map(|r| r.trim())
        .filter(|r| !r.is_empty())
    {
        item.replies.push(Reply {
            author: opts.author.clone(),
            body: body.to_owned(),
            at: opts.now.clone(),
        });
    }
    if let Some(status) = opts.status {
        if status == Status::Done && !opts.force {
            if item.is_edit() {
                let unchanged = item.target.as_ref().is_some_and(|t| {
                    sources
                        .get(&t.file)
                        .and_then(|s| s.text.get(t.range.as_range()))
                        .is_some_and(|cur| cur == t.quote.exact)
                        && item.kind != Kind::Insert
                        || item.kind == Kind::Insert
                            && sources.get(&t.file).is_some_and(|s| {
                                let ins = item
                                    .suggestion
                                    .as_ref()
                                    .and_then(|g| g.insert.as_deref())
                                    .unwrap_or("");
                                !ins.is_empty() && !s.text.contains(ins)
                            })
                });
                if unchanged {
                    return Err(Error::Refused(format!(
                        "{} asks for an edit, but the anchored text has not changed; apply it first, or pass --force if this is a deliberate decision",
                        item.id
                    )));
                }
            } else if matches!(item.kind, Kind::Comment | Kind::Question) && item.replies.is_empty()
            {
                return Err(Error::Refused(format!(
                    "{} is a {}; add a --reply before marking it done",
                    item.id,
                    item.kind.as_str()
                )));
            }
        }
        item.status = status;
        if status == Status::Done {
            item.resolution.at = Some(opts.now.clone());
            item.resolution.commit.clone_from(&opts.commit);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proofback_anchor::make_target;
    use proofback_model::{Anchor, AnchorState, Confidence, ItemId, Origin, Suggestion};

    fn sources(text: &str) -> Sources {
        Sources::new([("a.tex".to_owned(), text.to_owned())])
    }

    fn item(s: &Sources, kind: Kind, needle: &str, sug: Option<Suggestion>) -> Item {
        let src = s.get("a.tex").unwrap();
        let start = src.text.find(needle).unwrap();
        let range = if kind == Kind::Insert {
            start + needle.len()..start + needle.len()
        } else {
            start..start + needle.len()
        };
        Item {
            id: ItemId::derive("r", needle),
            round: "r".into(),
            kind,
            author: None,
            created: None,
            body: String::new(),
            suggestion: sug,
            target: Some(make_target("a.tex", src, range)),
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

    #[test]
    fn replace_delete_insert() {
        let s =
            sources("The estimator is strong under noise. Earlier work failed.\nTable~\\ref{t}.\n");
        let rep = item(
            &s,
            Kind::Replace,
            "strong",
            Some(Suggestion {
                replace_with: Some("robust".into()),
                insert: None,
            }),
        );
        let del = item(&s, Kind::Delete, "Earlier work failed.", None);
        let ins = item(
            &s,
            Kind::Insert,
            "Table~\\ref{t}",
            Some(Suggestion {
                replace_with: None,
                insert: Some("and Figure~\\ref{f}".into()),
            }),
        );
        let edits: Vec<Edit> = [&rep, &del, &ins]
            .iter()
            .map(|i| edit_for(i, &s).unwrap())
            .collect();
        let changes = apply_in_memory(&edits, &s).unwrap();
        let (_, new) = &changes["a.tex"];
        assert_eq!(
            new,
            "The estimator is robust under noise.\nTable~\\ref{t} and Figure~\\ref{f}.\n"
        );
        let diff = unified_diff(&changes);
        assert!(
            diff.contains("--- a/a.tex") && diff.contains("+The estimator is robust under noise."),
            "{diff}"
        );
    }

    #[test]
    fn refuses_stale_and_non_edit_items() {
        let s = sources("The estimator is strong.");
        let mut it = item(
            &s,
            Kind::Replace,
            "strong",
            Some(Suggestion {
                replace_with: Some("robust".into()),
                insert: None,
            }),
        );
        let s2 = sources("The estimator is robust.");
        assert!(matches!(edit_for(&it, &s2), Err(Error::Refused(_))));
        it.kind = Kind::Comment;
        assert!(matches!(edit_for(&it, &s), Err(Error::Refused(_))));
    }

    #[test]
    fn guard_rails() {
        let s = sources("The estimator is strong.");
        let mut rep = item(
            &s,
            Kind::Replace,
            "strong",
            Some(Suggestion {
                replace_with: Some("robust".into()),
                insert: None,
            }),
        );
        let done = ResolveOptions {
            status: Some(Status::Done),
            author: "me".into(),
            now: "t".into(),
            ..Default::default()
        };
        assert!(matches!(
            resolve(&mut rep, &s, &done),
            Err(Error::Refused(_))
        ));
        let after = sources("The estimator is robust.");
        resolve(&mut rep, &after, &done).unwrap();
        assert_eq!(rep.status, Status::Done);

        let mut q = item(&s, Kind::Question, "strong", None);
        assert!(matches!(resolve(&mut q, &s, &done), Err(Error::Refused(_))));
        let with_reply = ResolveOptions {
            reply: Some("Because of X.".into()),
            ..done.clone()
        };
        resolve(&mut q, &s, &with_reply).unwrap();
        assert_eq!(q.status, Status::Done);
        assert_eq!(q.replies.len(), 1);

        let mut forced = item(&s, Kind::Delete, "strong", None);
        resolve(
            &mut forced,
            &s,
            &ResolveOptions {
                force: true,
                ..done
            },
        )
        .unwrap();
        assert_eq!(forced.status, Status::Done);
    }
}
