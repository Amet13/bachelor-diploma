//! Notes import (plain and AI-structured), `confirm` and `suggest`.

use std::path::{Path, PathBuf};

use proofback_model::{Confidence, Item, Round, RoundFile, Source, SourceKind};
use proofback_store::edit::{Edit, apply_in_memory, unified_diff};
use proofback_store::views::{detail, row};
use proofback_store::{Error, Project, Result, merge_reimport, sha256_hex, slug, write_atomic};
use proofback_synctex::SyncTex;

use crate::commands::{find_synctex, now, read_limited};
use crate::output::{Style, table};
use crate::{ImportArgs, git};

/// Load the SyncTeX file (if any) for notes and Word imports.
pub fn load_synctex(p: &Project, a: &ImportArgs) -> Result<Option<(PathBuf, SyncTex, String)>> {
    if a.no_synctex {
        return Ok(None);
    }
    let main_pdf = p.build_dir().join(
        Path::new(&p.config.main)
            .with_extension("pdf")
            .file_name()
            .unwrap_or_default(),
    );
    let Some(path) = find_synctex(p, &main_pdf, a.synctex.as_deref()) else {
        return Ok(None);
    };
    let bytes = read_limited(&path, p.config.limits.max_synctex_mb)?;
    let st = SyncTex::parse(bytes.as_slice(), proofback_synctex::Limits::default())
        .map_err(|e| Error::Input(format!("{}: {e}", path.display())))?;
    Ok(Some((path, st, sha256_hex(&bytes))))
}

type Preview<'a> = dyn Fn(&proofback_ai::Settings) -> std::result::Result<proofback_ai::Request, proofback_ai::Error>
    + 'a;

/// The prepared AI request, enforcing "off by default" and consent. `None` means
/// the payload was only previewed.
fn ai_request(
    p: &Project,
    consent_flag: bool,
    build: &Preview<'_>,
) -> Result<Option<proofback_ai::Request>> {
    let cfg = &p.config.suggest;
    if !cfg.enabled {
        return Err(Error::Usage(
            "AI features are off. To use them, set in .proofback/config.toml:\n  [suggest]\n  enabled = true\n  provider = \"anthropic\"   # or \"openai\" or \"ollama\"\n  model = \"...\"\n  api_key_env = \"ANTHROPIC_API_KEY\""
                .into(),
        ));
    }
    let api_key = (!cfg.api_key_env.trim().is_empty())
        .then(|| std::env::var(cfg.api_key_env.trim()).ok())
        .flatten()
        .filter(|k| !k.trim().is_empty());
    let settings = proofback_ai::Settings {
        provider: cfg.provider.clone(),
        model: cfg.model.clone(),
        endpoint: cfg.endpoint.clone(),
        api_key,
    };
    let req = build(&settings).map_err(|e| Error::Usage(e.to_string()))?;
    if !cfg.consent && !consent_flag {
        println!("proofback would send this request to your AI provider:\n");
        println!("{}", req.preview());
        println!(
            "\nNothing was sent. Run the same command with --consent to send it (recorded in .proofback/config.toml)."
        );
        return Ok(None);
    }
    if !cfg.consent {
        let mut updated = p.config.clone();
        updated.suggest.consent = true;
        write_atomic(
            &p.state_dir().join("config.toml"),
            updated.to_toml().as_bytes(),
        )?;
    }
    Ok(Some(req))
}

pub fn import(p: &Project, a: &ImportArgs, quiet: bool, style: &Style, image: bool) -> Result<u8> {
    if image && !a.ai {
        return Err(Error::Usage(format!(
            "{} is an image; reading handwritten marks needs --ai (experimental)",
            a.file.display()
        )));
    }
    let bytes = read_limited(&a.file, if image { 20 } else { 8 })?;
    let notes = if a.ai {
        let mime = match a
            .file
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .as_deref()
        {
            Some("png") => "image/png",
            Some("webp") => "image/webp",
            Some("gif") => "image/gif",
            Some("jpg" | "jpeg") => "image/jpeg",
            _ => "text/plain",
        };
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let build = |s: &proofback_ai::Settings| {
            if image {
                proofback_ai::image_request(s, mime, &bytes)
            } else {
                proofback_ai::notes_request(s, &text)
            }
        };
        let Some(req) = ai_request(p, a.consent, &build)? else {
            return Ok(0);
        };
        let answer = proofback_ai::send(&req).map_err(|e| Error::Input(e.to_string()))?;
        proofback_notes::parse_structured(proofback_ai::strip_fences(&answer))
            .map_err(Error::Input)?
    } else {
        let text = String::from_utf8(bytes.clone())
            .map_err(|_| Error::Input(format!("{} is not UTF-8 text", a.file.display())))?;
        proofback_notes::parse(&text)
    };
    if notes.is_empty() {
        return Err(Error::Input(format!(
            "no notes found in {}",
            a.file.display()
        )));
    }
    let synctex = load_synctex(p, a)?;
    let (sources, skipped) = p.load_sources()?;
    for s in &skipped {
        eprintln!("warning: skipped {s}");
    }
    let stem = a
        .file
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let round_name = a
        .round
        .clone()
        .unwrap_or_else(|| format!("{}-{}", &now()[..10], slug(&stem)));
    p.round_path(&round_name)?;
    let ctx = proofback_notes::Context {
        sources: &sources,
        synctex: synctex.as_ref().map(|s| &s.1),
        project_root: &p.root,
        main: &p.config.main,
    };
    let items = proofback_notes::locate(&notes, &ctx, &round_name, a.ai);
    let existing = p.rounds()?.into_iter().find(|r| r.round.name == round_name);
    let items = merge_reimport(existing.as_ref(), items);
    let round = Round {
        name: round_name.clone(),
        created: existing
            .as_ref()
            .map_or_else(now, |r| r.round.created.clone()),
        source: Source {
            kind: SourceKind::Notes,
            file: a.file.to_string_lossy().into_owned(),
            sha256: sha256_hex(&bytes),
            synctex: synctex.as_ref().map(|s| {
                s.0.strip_prefix(&p.root)
                    .unwrap_or(&s.0)
                    .to_string_lossy()
                    .replace('\\', "/")
            }),
            synctex_sha256: synctex.as_ref().map(|s| s.2.clone()),
            commit: git::head(&p.root),
        },
        anonymized: false,
    };
    let rf = RoundFile::new(round, items);
    p.save_round(&rf)?;
    if a.json {
        let rows: Vec<_> = rf.items.iter().map(row).collect();
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({ "round": round_name, "items": rows })
            )
            .expect("json")
        );
    } else if !quiet {
        println!(
            "Imported {} note(s) into round {}.",
            rf.items.len(),
            style.bold(&round_name)
        );
        let refs: Vec<&Item> = rf.items.iter().collect();
        print!("{}", table(&refs, style));
        if a.ai {
            println!(
                "Locations from an AI model start at `page`; check each with `proofback show` and accept it with `proofback confirm <id>`."
            );
        }
    }
    Ok(0)
}

pub fn confirm(p: &Project, id: &str, quiet: bool) -> Result<u8> {
    let mut rounds = p.rounds()?;
    let (ri, item) = p.find_item(&rounds, id)?;
    let item_id = item.id.clone();
    let it = rounds[ri].item_mut(&item_id).expect("found");
    let Some(pending) = it.anchor.pending.take() else {
        return Err(Error::Usage(format!("{item_id} has nothing to confirm")));
    };
    it.anchor.confidence = pending;
    it.anchor.reason = format!("confirmed by {}", p.config.author);
    p.save_round(&rounds[ri])?;
    if !quiet {
        println!("{item_id} confirmed ({}).", pending.as_str());
    }
    Ok(0)
}

pub fn suggest(p: &Project, id: &str, consent: bool) -> Result<u8> {
    let rounds = p.rounds()?;
    let (_, item) = p.find_item(&rounds, id)?;
    let (sources, _) = p.load_sources()?;
    let target = item
        .target
        .clone()
        .ok_or_else(|| Error::Refused(format!("{} has no source location to edit", item.id)))?;
    if item.anchor.confidence != Confidence::Exact {
        return Err(Error::Refused(format!(
            "{} is only located to the {}; suggestions need an exact location",
            item.id,
            item.anchor.confidence.as_str()
        )));
    }
    let detail_json = serde_json::to_value(detail(item, &sources, 8)).expect("json");
    let build = |s: &proofback_ai::Settings| proofback_ai::suggest_request(s, &detail_json);
    let Some(req) = ai_request(p, consent, &build)? else {
        return Ok(0);
    };
    let answer = proofback_ai::send(&req).map_err(|e| Error::Input(e.to_string()))?;
    let sug = proofback_ai::parse_suggestion(&answer).map_err(|e| Error::Input(e.to_string()))?;
    let Some(new_text) = sug.new_text else {
        println!("No change suggested: {}", sug.rationale);
        return Ok(0);
    };
    let edit = Edit {
        file: target.file.clone(),
        range: target.range.as_range(),
        replacement: new_text,
        item: item.id.to_string(),
    };
    let changes = apply_in_memory(&[edit], &sources)?;
    let diff = unified_diff(&changes);
    let path = p
        .cache_dir()
        .join("suggest")
        .join(format!("{}.diff", item.id));
    write_atomic(&path, diff.as_bytes())?;
    println!("{}\n{diff}", sug.rationale);
    println!(
        "Not applied. Review it, then: git apply {}",
        path.strip_prefix(&p.root).unwrap_or(&path).display()
    );
    Ok(0)
}
