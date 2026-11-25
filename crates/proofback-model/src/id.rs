//! Item IDs: `pb_` followed by 8 lowercase Crockford base32 characters.

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";
const PREFIX: &str = "pb_";
const LEN: usize = 8;

/// A stable, short, human-typeable item identifier.
///
/// IDs are derived from the round name and the annotation's identity, so importing
/// the same review twice yields the same IDs and never duplicates items.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ItemId(String);

impl JsonSchema for ItemId {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ItemId".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^pb_[0-9a-hjkmnp-tv-z]{8}$",
            "description": "Stable item id: pb_ plus 8 lowercase Crockford base32 characters"
        })
    }
}

impl ItemId {
    /// Derive an ID from a round name and an identity string.
    pub fn derive(round: &str, identity: &str) -> Self {
        let mut h = Sha256::new();
        h.update(round.as_bytes());
        h.update([0]);
        h.update(identity.as_bytes());
        let digest = h.finalize();
        // 40 bits are plenty for items within one project.
        let mut bits: u64 = 0;
        for &b in &digest[..5] {
            bits = (bits << 8) | u64::from(b);
        }
        let mut s = String::with_capacity(PREFIX.len() + LEN);
        s.push_str(PREFIX);
        for i in (0..LEN).rev() {
            s.push(ALPHABET[((bits >> (i * 5)) & 31) as usize] as char);
        }
        Self(s)
    }

    /// Derive an ID that does not collide with `taken`, by salting the identity.
    pub fn derive_unique(round: &str, identity: &str, taken: impl Fn(&ItemId) -> bool) -> Self {
        let mut id = Self::derive(round, identity);
        let mut salt = 1u32;
        while taken(&id) {
            id = Self::derive(round, &format!("{identity}#{salt}"));
            salt += 1;
        }
        id
    }

    /// Parse a full ID (`pb_` plus 8 characters).
    pub fn parse(s: &str) -> Option<Self> {
        let rest = s.strip_prefix(PREFIX)?;
        (rest.len() == LEN && rest.bytes().all(|b| ALPHABET.contains(&b)))
            .then(|| Self(s.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for ItemId {
    type Error = String;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::parse(&s).ok_or_else(|| format!("invalid item id {s:?}"))
    }
}

impl From<ItemId> for String {
    fn from(id: ItemId) -> Self {
        id.0
    }
}

/// Why a prefix did not resolve to exactly one ID.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PrefixError {
    #[error("no item matches {0:?}")]
    NotFound(String),
    #[error("{query:?} is ambiguous; it matches {}", candidates.join(", "))]
    Ambiguous {
        query: String,
        candidates: Vec<String>,
    },
}

/// Resolve a unique prefix (with or without `pb_`), like git does for commits.
pub fn resolve_prefix<'a>(
    ids: impl IntoIterator<Item = &'a ItemId>,
    query: &str,
) -> Result<&'a ItemId, PrefixError> {
    let q = query.trim().to_ascii_lowercase();
    let q = q.strip_prefix(PREFIX).unwrap_or(&q).to_owned();
    if q.is_empty() {
        return Err(PrefixError::NotFound(query.to_owned()));
    }
    let matches: Vec<&ItemId> = ids
        .into_iter()
        .filter(|id| id.0[PREFIX.len()..].starts_with(&q))
        .collect();
    match matches.as_slice() {
        [one] => Ok(one),
        [] => Err(PrefixError::NotFound(query.to_owned())),
        many => {
            let mut candidates: Vec<String> = many.iter().map(|i| i.0.clone()).collect();
            candidates.sort();
            candidates.dedup();
            if candidates.len() == 1 {
                return Ok(many[0]);
            }
            Err(PrefixError::Ambiguous {
                query: query.to_owned(),
                candidates,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derivation_is_stable_and_valid() {
        let a = ItemId::derive("round", "nm:abc");
        assert_eq!(a, ItemId::derive("round", "nm:abc"));
        assert_ne!(a, ItemId::derive("round", "nm:abd"));
        assert!(ItemId::parse(a.as_str()).is_some());
        assert_eq!(a.as_str().len(), 11);
    }

    #[test]
    fn parse_rejects_bad_ids() {
        for bad in [
            "pb_1234567",
            "pb_123456789",
            "xx_12345678",
            "pb_1234567i",
            "pb_ABCDEFGH",
        ] {
            assert!(ItemId::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn prefixes_resolve_like_git() {
        let ids = vec![
            ItemId::parse("pb_7k2m9q00").unwrap(),
            ItemId::parse("pb_7k3aaaaa").unwrap(),
        ];
        assert_eq!(
            resolve_prefix(&ids, "pb_7k2").unwrap().as_str(),
            "pb_7k2m9q00"
        );
        assert_eq!(resolve_prefix(&ids, "7K3").unwrap().as_str(), "pb_7k3aaaaa");
        assert!(matches!(
            resolve_prefix(&ids, "7k"),
            Err(PrefixError::Ambiguous { .. })
        ));
        assert!(matches!(
            resolve_prefix(&ids, "zz"),
            Err(PrefixError::NotFound(_))
        ));
        assert!(matches!(
            resolve_prefix(&ids, ""),
            Err(PrefixError::NotFound(_))
        ));
    }

    #[test]
    fn unique_derivation_avoids_collisions() {
        let first = ItemId::derive("r", "x");
        let second = ItemId::derive_unique("r", "x", |id| id == &first);
        assert_ne!(first, second);
    }
}
