//! Minimal unified-diff application for agent proposals (single file).

use proofback_store::Error;

/// Apply a unified diff that changes exactly one file. `read` returns the current
/// contents for a project-relative path. Returns the path and the new contents.
pub fn apply_unified(
    diff: &str,
    read: impl Fn(&str) -> Option<String>,
) -> Result<(String, String), Error> {
    let mut file: Option<String> = None;
    let mut hunks: Vec<(usize, Vec<String>)> = Vec::new();
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("+++ ") {
            let path = rest.split('\t').next().unwrap_or("").trim();
            let path = path.strip_prefix("b/").unwrap_or(path);
            if file.as_deref().is_some_and(|f| f != path) {
                return Err(Error::Refused(
                    "the diff changes more than one file; propose one item at a time".into(),
                ));
            }
            file = Some(path.to_owned());
        } else if line.starts_with("--- ") {
        } else if let Some(h) = line.strip_prefix("@@ -") {
            let start: usize = h
                .split([',', ' '])
                .next()
                .and_then(|n| n.parse().ok())
                .ok_or_else(|| Error::Usage("malformed hunk header".into()))?;
            hunks.push((start, Vec::new()));
        } else if let Some((_, body)) = hunks.last_mut()
            && (line.starts_with([' ', '+', '-']) || line.is_empty())
        {
            body.push(if line.is_empty() {
                " ".to_owned()
            } else {
                line.to_owned()
            });
        }
    }
    let file = file.ok_or_else(|| Error::Usage("the diff has no +++ header".into()))?;
    if file.contains("..") || file.starts_with('/') || file == "/dev/null" {
        return Err(Error::Refused(format!(
            "{file:?} is not a file inside the project"
        )));
    }
    let old =
        read(&file).ok_or_else(|| Error::Refused(format!("{file} is not a loaded source file")))?;
    let ends_nl = old.ends_with('\n');
    let mut lines: Vec<String> = old.lines().map(str::to_owned).collect();
    let mut offset: isize = 0;
    for (start, body) in hunks {
        let expect: Vec<&str> = body
            .iter()
            .filter(|l| !l.starts_with('+'))
            .map(|l| &l[1..])
            .collect();
        let replace: Vec<String> = body
            .iter()
            .filter(|l| !l.starts_with('-'))
            .map(|l| l[1..].to_owned())
            .collect();
        let nominal = (start as isize - 1 + offset).max(0) as usize;
        let at = (0..=60usize)
            .flat_map(|d| [nominal.checked_add(d), nominal.checked_sub(d)])
            .flatten()
            .find(|&i| {
                i + expect.len() <= lines.len()
                    && lines[i..i + expect.len()]
                        .iter()
                        .map(String::as_str)
                        .eq(expect.iter().copied())
            })
            .ok_or_else(|| {
                Error::Refused(
                    "the diff does not apply to the current file; call sync and propose again"
                        .into(),
                )
            })?;
        let removed = expect.len();
        lines.splice(at..at + removed, replace.iter().cloned());
        offset += replace.len() as isize - removed as isize;
    }
    let mut new = lines.join("\n");
    if ends_nl {
        new.push('\n');
    }
    Ok((file, new))
}

/// Whether the change from `old` to `new` modifies bytes inside (or adjacent to)
/// `range` of `old`.
pub fn touches(old: &str, new: &str, range: std::ops::Range<usize>) -> bool {
    let prefix = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = old
        .bytes()
        .rev()
        .zip(new.bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let changed_end = old.len().saturating_sub(suffix).max(prefix);
    // Changed region in old: prefix..changed_end. It must overlap the target,
    // allowing an insertion point to count as touched.
    prefix <= range.end && changed_end >= range.start && old != new
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_and_checks_touch() {
        let old = "a\nThe estimator is strong.\nc\n".to_owned();
        let d = "--- a/x.tex\n+++ b/x.tex\n@@ -1,3 +1,3 @@\n a\n-The estimator is strong.\n+The estimator is robust.\n c\n";
        let (f, new) = apply_unified(d, |p| (p == "x.tex").then(|| old.clone())).unwrap();
        assert_eq!(f, "x.tex");
        assert_eq!(new, "a\nThe estimator is robust.\nc\n");
        let s = old.find("strong").unwrap();
        assert!(touches(&old, &new, s..s + 6));
        assert!(!touches(&old, &new, 0..1));
    }

    #[test]
    fn rejects_bad_diffs() {
        let read = |_: &str| Some("x\n".to_owned());
        assert!(apply_unified("--- a/../e\n+++ b/../e\n@@ -1 +1 @@\n-x\n+y\n", read).is_err());
        assert!(apply_unified("--- a/a\n+++ b/a\n@@ -1 +1 @@\n-nope\n+y\n", read).is_err());
        assert!(apply_unified("--- a/a\n+++ b/a\n+++ b/b\n", read).is_err());
    }
}
