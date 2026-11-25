//! Semi-global alignment of a short query against a longer text.
//!
//! The text may contain [`PLACEHOLDER`] characters (references, citations, inline
//! maths) whose rendering is unknown; a placeholder matches any run of one or more
//! non-space query characters at no cost, so "Table 3.1" aligns with
//! "Table \u{FFFC}".

use proofback_tex::PLACEHOLDER;

/// One alignment of the query inside the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    /// Character range in the text.
    pub start: usize,
    pub end: usize,
    /// Edit cost (0 is exact, placeholders included).
    pub cost: usize,
}

/// Best alignments: all hits with the minimal cost, non-overlapping, plus the
/// second-best cost so callers can judge uniqueness.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Alignment {
    pub best: Vec<Hit>,
    pub second_cost: Option<usize>,
}

/// Align `query` (folded characters) against `text` (folded characters).
///
/// Returns hits whose cost is at most `max_cost`. Complexity is
/// `O(query.len() * text.len())`; callers keep the text to a window.
pub fn align(query: &[char], text: &[char], max_cost: usize) -> Alignment {
    let m = query.len();
    let n = text.len();
    if m == 0 || n == 0 {
        return Alignment::default();
    }
    // dp[j][i]: minimal cost of aligning query[..i] so that it ends at text[..j].
    // start[j][i]: text index where that alignment starts.
    // Rolling columns over j.
    let inf = usize::MAX / 4;
    let mut prev = vec![inf; m + 1];
    let mut prev_start = vec![0usize; m + 1];
    let mut cur = vec![inf; m + 1];
    let mut cur_start = vec![0usize; m + 1];
    prev[0] = 0;
    prev_start[0] = 0;
    // Column 0 (empty text prefix): deleting query characters.
    for i in 1..=m {
        prev[i] = i;
        prev_start[i] = 0;
    }
    let mut ends: Vec<Hit> = Vec::new();
    for j in 1..=n {
        let t = text[j - 1];
        cur[0] = 0;
        cur_start[0] = j; // free leading text
        if t == PLACEHOLDER {
            // A placeholder absorbs a run of non-space query characters (the rendered
            // reference), or nothing when it was not marked; both cost 0.
            for i in 1..=m {
                let (mut best, mut bs) = (prev[i], prev_start[i]);
                if query[i - 1] != ' ' {
                    if prev[i - 1] < best {
                        best = prev[i - 1];
                        bs = prev_start[i - 1];
                    }
                    if cur[i - 1] < best {
                        best = cur[i - 1];
                        bs = cur_start[i - 1];
                    }
                }
                cur[i] = best;
                cur_start[i] = bs;
            }
        } else {
            for i in 1..=m {
                let sub = prev[i - 1] + usize::from(query[i - 1] != t);
                let del_text = prev[i] + 1; // extra text char
                let del_query = cur[i - 1] + 1; // missing text char
                let (mut best, mut bs) = (sub, prev_start[i - 1]);
                if del_text < best {
                    best = del_text;
                    bs = prev_start[i];
                }
                if del_query < best {
                    best = del_query;
                    bs = cur_start[i - 1];
                }
                cur[i] = best;
                cur_start[i] = bs;
            }
        }
        if cur[m] <= max_cost {
            ends.push(Hit {
                start: cur_start[m],
                end: j,
                cost: cur[m],
            });
        }
        std::mem::swap(&mut prev, &mut cur);
        std::mem::swap(&mut prev_start, &mut cur_start);
    }
    summarize(ends)
}

fn summarize(mut ends: Vec<Hit>) -> Alignment {
    if ends.is_empty() {
        return Alignment::default();
    }
    // Group overlapping hits and keep the cheapest (then shortest) per group.
    ends.sort_by_key(|h| (h.start, h.end));
    let mut groups: Vec<Hit> = Vec::new();
    for h in ends {
        match groups.last_mut() {
            Some(g) if h.start < g.end => {
                if (h.cost, h.end - h.start) < (g.cost, g.end - g.start) {
                    *g = h;
                }
            }
            _ => groups.push(h),
        }
    }
    let min = groups.iter().map(|h| h.cost).min().unwrap_or(0);
    let second = groups.iter().map(|h| h.cost).filter(|&c| c > min).min();
    let best: Vec<Hit> = groups.into_iter().filter(|h| h.cost == min).collect();
    Alignment {
        best,
        second_cost: second,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn exact_unique_match() {
        let a = align(&c("strong"), &c("the estimator is strong under"), 0);
        assert_eq!(
            a.best,
            vec![Hit {
                start: 17,
                end: 23,
                cost: 0
            }]
        );
    }

    #[test]
    fn repeated_matches_are_all_reported() {
        let a = align(&c("ab"), &c("xab yab"), 0);
        assert_eq!(a.best.len(), 2);
    }

    #[test]
    fn fuzzy_match_within_cost() {
        let a = align(&c("estimater"), &c("the estimator is"), 2);
        assert_eq!(a.best.len(), 1);
        assert_eq!(a.best[0].cost, 1);
        assert_eq!(align(&c("zzzzzz"), &c("the estimator is"), 1).best.len(), 0);
    }

    #[test]
    fn placeholder_absorbs_rendered_reference() {
        let text: Vec<char> = format!("given in table {PLACEHOLDER} and figure")
            .chars()
            .collect();
        let a = align(&c("in table 3.1 and"), &text, 0);
        assert_eq!(a.best.len(), 1, "{a:?}");
        let h = a.best[0];
        assert_eq!(
            text[h.start..h.end].iter().collect::<String>(),
            format!("in table {PLACEHOLDER} and")
        );
    }
}
