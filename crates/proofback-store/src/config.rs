//! `.proofback/config.toml`.

use serde::{Deserialize, Serialize};

/// Project configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Main LaTeX file, relative to the project root.
    pub main: String,
    /// Directory holding the compiled PDF and SyncTeX file.
    pub build_dir: String,
    /// Name used for your replies.
    pub author: String,
    pub import: ImportConfig,
    pub limits: LimitsConfig,
    pub suggest: SuggestConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            main: "main.tex".into(),
            build_dir: ".".into(),
            author: "me".into(),
            import: ImportConfig::default(),
            limits: LimitsConfig::default(),
            suggest: SuggestConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ImportConfig {
    /// Replace reviewer names with "Reviewer N" (blind review).
    pub anonymize_authors: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LimitsConfig {
    pub max_pdf_mb: u64,
    pub max_pages: usize,
    pub max_annotations: usize,
    pub max_source_mb: u64,
    pub max_synctex_mb: u64,
    /// Wall-clock limit for parsing a PDF, in seconds.
    pub parse_timeout_secs: u64,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_pdf_mb: 512,
            max_pages: 20_000,
            max_annotations: 100_000,
            max_source_mb: 8,
            max_synctex_mb: 1024,
            parse_timeout_secs: 300,
        }
    }
}

/// Optional AI suggestions (off by default).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SuggestConfig {
    pub enabled: bool,
    /// `anthropic`, `openai` (any OpenAI-compatible endpoint) or `ollama`.
    pub provider: String,
    pub model: String,
    /// Base URL; empty means the provider's default.
    pub endpoint: String,
    /// Name of the environment variable holding the API key. Keys are never stored
    /// in this file.
    pub api_key_env: String,
    /// Set once you have seen what `suggest` sends and agreed to it.
    pub consent: bool,
}

impl Config {
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    pub fn to_toml(&self) -> String {
        let body = toml::to_string_pretty(self).expect("config serialises");
        format!("# proofback project configuration. See docs/PLAN.md section 7.\n{body}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_rejects_unknown_keys() {
        let c = Config::default();
        assert_eq!(Config::parse(&c.to_toml()).unwrap(), c);
        assert!(Config::parse("mian = \"x.tex\"").is_err());
        assert_eq!(Config::parse("").unwrap(), Config::default());
    }
}
