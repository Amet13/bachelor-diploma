//! Command implementations.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use proofback_anchor::{Input, Sources, import};
use proofback_emit::{answer_pdf, letter, markdown_checklist, sarif, tex_escape};
use proofback_model::{
    Confidence, Item, Round, RoundFile, Source, SourceKind, Status, rfc3339_utc,
};
use proofback_store::edit::{
    Edit, ResolveOptions, apply_in_memory, edit_for, resolve, unified_diff, write_changes,
};
use proofback_store::sync::sync_items;
use proofback_store::views::{detail, row};
use proofback_store::{Error, Project, Result, merge_reimport, sha256_hex, slug, write_atomic};
use proofback_synctex::SyncTex;

use crate::output::{Style, color_enabled, table};
use crate::{Cli, ColorChoice, Command, ImportArgs, LetterFormat, ListArgs, git};

/// Run a command and return the process exit code.
pub fn run(cli: &Cli) -> Result<u8> {
    let style = Style {
        on: color_enabled(match cli.color {
            ColorChoice::Always => "always",
            ColorChoice::Never => "never",
            ColorChoice::Auto => "auto",
        }),
    };
    let cwd = std::env::current_dir().map_err(|e| Error::Input(e.to_string()))?;
    let start = cli.project.clone().unwrap_or(cwd);
    let project = || Project::discover(&start);
    match &cli.command {
        Command::Init { main } => {
            let p = Project::init(&start, main.as_deref())?;
            if !cli.quiet {
                println!(
                    "Initialised proofback in {} (main file: {}).",
                    p.state_dir().display(),
                    p.config.main
                );
                println!("Next: proofback import <review.pdf>");
            }
            Ok(0)
        }
        Command::Import(a) => cmd_import(&project()?, a, cli.quiet, &style),
        Command::List(a) => cmd_list(&project()?, a, &style),
        Command::Show { id, json, context } => cmd_show(&project()?, id, *json, *context, &style),
        Command::Triage { all } => {
            let p = project()?;
            let head = git::head(&p.root);
            crate::triage::run(&p, *all, &now, head.as_deref())?;
            Ok(0)
        }
        Command::Resolve {
            id,
            status,
            reply,
            force,
            json,
        } => cmd_resolve(
            &project()?,
            id,
            status.as_deref(),
            reply.as_deref(),
            *force,
            *json,
            cli.quiet,
        ),
        Command::Patch { ids } => {
            let p = project()?;
            let (changes, _) = planned_changes(&p, ids)?;
            print!("{}", unified_diff(&changes));
            Ok(0)
        }
        Command::Apply {
            ids,
            dry_run,
            resolve,
        } => cmd_apply(&project()?, ids, *dry_run, *resolve, cli.quiet),
        Command::Sync { json, dry_run } => cmd_sync(&project()?, *json, *dry_run, cli.quiet),
        Command::Answer { output, round, pdf } => cmd_answer(
            &project()?,
            output,
            round.as_deref(),
            pdf.as_deref(),
            cli.quiet,
        ),
        Command::Letter {
            format,
            round,
            title,
            output,
        } => {
            let p = project()?;
            let rounds = p.rounds()?;
            let items = select_round_items(&rounds, round.as_deref());
            let fmt = match format {
                LetterFormat::Markdown => proofback_emit::LetterFormat::Markdown,
                LetterFormat::Latex => proofback_emit::LetterFormat::Latex,
            };
            emit(output.as_deref(), &letter(&items, title, fmt))?;
            Ok(0)
        }
        Command::Inline { ids, todo, dry_run } => {
            cmd_inline(&project()?, ids, *todo, *dry_run, cli.quiet)
        }
        Command::Sarif { output } => {
            let p = project()?;
            let rounds = p.rounds()?;
            let items: Vec<&Item> = rounds.iter().flat_map(|r| r.items.iter()).collect();
            let s = serde_json::to_string_pretty(&sarif(&items, env!("CARGO_PKG_VERSION")))
                .expect("json");
            emit(output.as_deref(), &format!("{s}\n"))?;
            Ok(0)
        }
        Command::Stats { json } => cmd_stats(&project()?, *json),
        Command::Doctor { json } => Ok(cmd_doctor(&start, *json, &style)),
        Command::Mcp => {
            let p = project()?;
            let mut server = proofback_mcp::Server::new(p, Box::new(now));
            let stdin = std::io::stdin();
            server
                .serve(stdin.lock(), std::io::stdout().lock())
                .map_err(|e| Error::Input(e.to_string()))?;
            Ok(0)
        }
        Command::Schema => {
            println!(
                "{}",
                serde_json::to_string_pretty(&proofback_model::round_file_schema()).expect("json")
            );
            Ok(0)
        }
        Command::Extract { .. } => unreachable!("handled in main"),
    }
}

fn now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    rfc3339_utc(secs)
}

fn emit(path: Option<&Path>, text: &str) -> Result<()> {
    if let Some(p) = path {
        write_atomic(p, text.as_bytes())
    } else {
        print!("{text}");
        Ok(())
    }
}

fn read_limited(path: &Path, max_mb: u64) -> Result<Vec<u8>> {
    let meta = std::fs::metadata(path)
        .map_err(|e| Error::Input(format!("cannot read {}: {e}", path.display())))?;
    if meta.len() > max_mb * 1024 * 1024 {
        return Err(Error::Input(format!(
            "{} is larger than the {max_mb} MB limit",
            path.display()
        )));
    }
    std::fs::read(path).map_err(|e| Error::Input(format!("cannot read {}: {e}", path.display())))
}

/// Find the SyncTeX file for a PDF.
fn find_synctex(p: &Project, pdf: &Path, explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(e) = explicit {
        return Some(e.to_path_buf());
    }
    let mut cands = Vec::new();
    if let (Some(dir), Some(stem)) = (pdf.parent(), pdf.file_stem()) {
        cands.push(dir.join(format!("{}.synctex.gz", stem.to_string_lossy())));
        cands.push(dir.join(format!("{}.synctex", stem.to_string_lossy())));
    }
    let main_stem = Path::new(&p.config.main)
        .file_stem()
        .map_or_else(|| "main".into(), |s| s.to_string_lossy().into_owned());
    for name in [
        format!("{main_stem}.synctex.gz"),
        format!("{main_stem}.synctex"),
        "output.synctex.gz".into(),
    ] {
        cands.push(p.build_dir().join(name));
    }
    cands.into_iter().find(|c| c.is_file())
}

fn cmd_import(p: &Project, a: &ImportArgs, quiet: bool, style: &Style) -> Result<u8> {
    let ext = a
        .file
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if ext != "pdf" {
        return Err(Error::Usage(format!(
            "{} is not a PDF; this version imports annotated PDFs (notes and Word files come in later versions)",
            a.file.display()
        )));
    }
    let pdf_bytes = read_limited(&a.file, p.config.limits.max_pdf_mb)?;
    let extracted = crate::extract::extract_isolated(&a.file, &p.config.limits)?;

    let synctex_path = if a.no_synctex {
        None
    } else {
        find_synctex(p, &a.file, a.synctex.as_deref())
    };
    let (synctex, synctex_sha) = match &synctex_path {
        Some(path) => {
            let bytes = read_limited(path, p.config.limits.max_synctex_mb)?;
            let st = SyncTex::parse(bytes.as_slice(), proofback_synctex::Limits::default())
                .map_err(|e| Error::Input(format!("{}: {e}", path.display())))?;
            (Some(st), Some(sha256_hex(&bytes)))
        }
        None => (None, None),
    };

    let (sources, skipped) = match &a.at {
        Some(rev) => (Sources::new(git::tex_files_at(&p.root, rev)?), Vec::new()),
        None => p.load_sources()?,
    };
    if sources.is_empty() {
        return Err(Error::Input(format!(
            "no .tex files found in {}",
            p.root.display()
        )));
    }
    for s in &skipped {
        eprintln!("warning: skipped {s}");
    }

    let date = now()[..10].to_owned();
    let round_name = match &a.round {
        Some(r) => r.clone(),
        None => format!(
            "{date}-{}",
            slug(
                &a.file
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default()
            )
        ),
    };
    p.round_path(&round_name)?;
    let texts: HashMap<u32, proofback_pdf::PageText> =
        extracted.texts.into_iter().map(|t| (t.page, t)).collect();
    let input = Input {
        round: &round_name,
        annotations: &extracted.annotations,
        boxes: &extracted.boxes,
        texts: &texts,
        synctex: synctex.as_ref(),
        project_root: &p.root,
        sources: &sources,
        anonymize: a.anonymize_authors || p.config.import.anonymize_authors,
    };
    let mut items = import(&input);
    if a.at.is_some() {
        // Carry anchors from the reviewed commit to the working tree.
        let (current, _) = p.load_sources()?;
        sync_items(&mut items, &current);
    }
    let existing = p.rounds()?.into_iter().find(|r| r.round.name == round_name);
    let items = merge_reimport(existing.as_ref(), items);
    let rel_file = a.file.canonicalize().ok().and_then(|f| {
        f.strip_prefix(&p.root)
            .ok()
            .map(|r| r.to_string_lossy().replace('\\', "/"))
    });
    let round = Round {
        name: round_name.clone(),
        created: existing
            .as_ref()
            .map_or_else(now, |r| r.round.created.clone()),
        source: Source {
            kind: SourceKind::Pdf,
            file: rel_file.unwrap_or_else(|| a.file.to_string_lossy().into_owned()),
            sha256: sha256_hex(&pdf_bytes),
            synctex: synctex_path.as_ref().map(|s| {
                s.strip_prefix(&p.root)
                    .unwrap_or(s)
                    .to_string_lossy()
                    .replace('\\', "/")
            }),
            synctex_sha256: synctex_sha,
            commit: a.at.clone().or_else(|| git::head(&p.root)),
        },
        anonymized: input.anonymize,
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
        let mut by_conf: BTreeMap<Confidence, usize> = BTreeMap::new();
        for it in &rf.items {
            *by_conf.entry(it.anchor.confidence).or_default() += 1;
        }
        let summary: Vec<String> = by_conf
            .iter()
            .map(|(c, n)| format!("{n} {}", style.confidence(*c)))
            .collect();
        let mode = if synctex.is_some() {
            "with SyncTeX"
        } else {
            "text only (no SyncTeX file found)"
        };
        println!(
            "Imported {} item(s) into round {} ({mode}): {}",
            rf.items.len(),
            style.bold(&round_name),
            summary.join(", ")
        );
        let refs: Vec<&Item> = rf.items.iter().collect();
        print!("{}", table(&refs, style));
    }
    Ok(0)
}

fn filtered<'a>(rounds: &'a [RoundFile], a: &ListArgs) -> Result<Vec<&'a Item>> {
    let statuses: Vec<Status> = a
        .status
        .iter()
        .map(|s| Status::parse(s).ok_or_else(|| Error::Usage(format!("unknown status {s:?}"))))
        .collect::<Result<_>>()?;
    Ok(rounds
        .iter()
        .filter(|r| a.round.as_ref().is_none_or(|n| &r.round.name == n))
        .flat_map(|r| r.items.iter())
        .filter(|i| statuses.is_empty() || statuses.contains(&i.status))
        .filter(|i| a.kind.is_empty() || a.kind.iter().any(|k| k == i.kind.as_str()))
        .filter(|i| {
            a.file.as_ref().is_none_or(|f| {
                i.target
                    .as_ref()
                    .is_some_and(|t| t.file.starts_with(f.as_str()))
            })
        })
        .collect())
}

fn sort_items(items: &mut [&Item]) {
    items.sort_by(|a, b| {
        let ka = a.target.as_ref().map(|t| (t.file.clone(), t.range.start));
        let kb = b.target.as_ref().map(|t| (t.file.clone(), t.range.start));
        ka.is_none()
            .cmp(&kb.is_none())
            .then(ka.cmp(&kb))
            .then(a.id.cmp(&b.id))
    });
}

fn cmd_list(p: &Project, a: &ListArgs, style: &Style) -> Result<u8> {
    let rounds = p.rounds()?;
    let mut items = filtered(&rounds, a)?;
    sort_items(&mut items);
    if a.json {
        let rows: Vec<_> = items.iter().map(|i| row(i)).collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({ "items": rows })).expect("json")
        );
    } else if a.md {
        print!("{}", markdown_checklist(&items));
    } else if items.is_empty() {
        println!("No items. Import a review with `proofback import <review.pdf>`.");
    } else {
        print!("{}", table(&items, style));
    }
    let open = items
        .iter()
        .any(|i| matches!(i.status, Status::Open | Status::Discuss));
    Ok(u8::from(a.fail_on_open && open))
}

fn cmd_show(p: &Project, id: &str, json: bool, context: u32, style: &Style) -> Result<u8> {
    let rounds = p.rounds()?;
    let (_, item) = p.find_item(&rounds, id)?;
    let (sources, _) = p.load_sources()?;
    let d = detail(item, &sources, context);
    if json {
        println!("{}", serde_json::to_string_pretty(&d).expect("json"));
        return Ok(0);
    }
    println!(
        "{}  {}  {}  {}",
        style.bold(&d.row.id),
        d.row.kind,
        style.status(item.status),
        style.confidence(item.anchor.confidence)
    );
    println!(
        "{}",
        style.dim(&format!("round {} · {}", d.row.round, d.reason))
    );
    if let Some(a) = &item.author {
        println!(
            "{}: {}",
            a,
            if item.body.is_empty() {
                "(no note)"
            } else {
                &item.body
            }
        );
    } else if !item.body.is_empty() {
        println!("{}", item.body);
    }
    for t in &item.thread {
        println!("  {} {}: {}", style.dim("↳"), t.author, t.body);
    }
    if let Some(s) = &item.suggestion {
        if let Some(r) = &s.replace_with {
            println!("suggestion: replace with {r:?}");
        }
        if let Some(i) = &s.insert {
            println!("suggestion: insert {i:?}");
        }
    }
    println!("{}", crate::output::location(item));
    for l in &d.context {
        let mark = if l.in_target { ">" } else { " " };
        println!("{} {:>5} | {}", mark, l.line, l.text);
    }
    for r in &item.replies {
        println!("reply ({}, {}): {}", r.author, r.at, r.body);
    }
    Ok(0)
}

fn cmd_resolve(
    p: &Project,
    id: &str,
    status: Option<&str>,
    reply: Option<&str>,
    force: bool,
    json: bool,
    quiet: bool,
) -> Result<u8> {
    let status = status
        .map(|s| Status::parse(s).ok_or_else(|| Error::Usage(format!("unknown status {s:?}"))))
        .transpose()?;
    if status.is_none() && reply.is_none() {
        return Err(Error::Usage("give --status and/or --reply".into()));
    }
    let mut rounds = p.rounds()?;
    let (ri, item) = p.find_item(&rounds, id)?;
    let item_id = item.id.clone();
    let (sources, _) = p.load_sources()?;
    let opts = ResolveOptions {
        status,
        reply: reply.map(str::to_owned),
        author: p.config.author.clone(),
        now: now(),
        force,
        commit: git::head(&p.root),
    };
    let it = rounds[ri].item_mut(&item_id).expect("found above");
    resolve(it, &sources, &opts)?;
    let snapshot = it.clone();
    p.save_round(&rounds[ri])?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&row(&snapshot)).expect("json")
        );
    } else if !quiet {
        println!("{} is now {}.", item_id, snapshot.status.as_str());
    }
    Ok(0)
}

/// Edits for the given ids (or all open exact edit items) and the resulting changes.
fn planned_changes(
    p: &Project,
    ids: &[String],
) -> Result<(proofback_store::edit::Changes, Vec<Edit>)> {
    let rounds = p.rounds()?;
    let (sources, _) = p.load_sources()?;
    let items: Vec<&Item> = if ids.is_empty() {
        rounds
            .iter()
            .flat_map(|r| r.items.iter())
            .filter(|i| {
                i.is_edit() && i.status == Status::Open && i.anchor.confidence == Confidence::Exact
            })
            .collect()
    } else {
        ids.iter()
            .map(|id| p.find_item(&rounds, id).map(|(_, i)| i))
            .collect::<Result<_>>()?
    };
    let mut edits = Vec::new();
    for it in items {
        match edit_for(it, &sources) {
            Ok(e) => edits.push(e),
            Err(e) if ids.is_empty() => eprintln!("skipping {}: {e}", it.id),
            Err(e) => return Err(e),
        }
    }
    let changes = apply_in_memory(&edits, &sources)?;
    Ok((changes, edits))
}

fn cmd_apply(
    p: &Project,
    ids: &[String],
    dry_run: bool,
    mark_done: bool,
    quiet: bool,
) -> Result<u8> {
    let (changes, edits) = planned_changes(p, ids)?;
    if changes.is_empty() {
        if !quiet {
            println!("Nothing to apply.");
        }
        return Ok(0);
    }
    if dry_run {
        print!("{}", unified_diff(&changes));
        return Ok(0);
    }
    write_changes(p, &changes)?;
    if mark_done {
        let mut rounds = p.rounds()?;
        let (sources, _) = p.load_sources()?;
        for e in &edits {
            for r in &mut rounds {
                if let Some(it) = r.items.iter_mut().find(|i| i.id.as_str() == e.item) {
                    let opts = ResolveOptions {
                        status: Some(Status::Done),
                        author: p.config.author.clone(),
                        now: now(),
                        ..Default::default()
                    };
                    resolve(it, &sources, &opts)?;
                }
            }
        }
        for r in &rounds {
            p.save_round(r)?;
        }
    }
    if !quiet {
        println!(
            "Applied {} edit(s) to {} file(s).",
            edits.len(),
            changes.len()
        );
    }
    Ok(0)
}

fn cmd_sync(p: &Project, json: bool, dry_run: bool, quiet: bool) -> Result<u8> {
    let mut rounds = p.rounds()?;
    let (sources, _) = p.load_sources()?;
    let mut total = proofback_store::sync::SyncReport::default();
    for r in &mut rounds {
        let rep = sync_items(&mut r.items, &sources);
        total.anchored += rep.anchored;
        total.moved += rep.moved;
        total.changed += rep.changed;
        total.lost += rep.lost;
        if !dry_run {
            p.save_round(r)?;
        }
    }
    if json {
        println!(
            "{}",
            serde_json::json!({"anchored": total.anchored, "moved": total.moved, "changed": total.changed, "lost": total.lost})
        );
    } else if !quiet {
        println!(
            "{} anchored, {} moved, {} changed, {} lost.",
            total.anchored, total.moved, total.changed, total.lost
        );
        if total.lost > 0 {
            println!("Lost items: proofback list --json | look for \"anchor_state\": \"lost\"");
        }
    }
    Ok(0)
}

fn select_round_items<'a>(rounds: &'a [RoundFile], round: Option<&str>) -> Vec<&'a Item> {
    rounds
        .iter()
        .filter(|r| round.is_none_or(|n| r.round.name == n))
        .flat_map(|r| r.items.iter())
        .collect()
}

fn cmd_answer(
    p: &Project,
    output: &Path,
    round: Option<&str>,
    pdf: Option<&Path>,
    quiet: bool,
) -> Result<u8> {
    let rounds = p.rounds()?;
    let r = match round {
        Some(n) => rounds
            .iter()
            .find(|r| r.round.name == n)
            .ok_or_else(|| Error::Usage(format!("no round {n:?}")))?,
        None => rounds
            .iter()
            .filter(|r| r.round.source.kind == SourceKind::Pdf)
            .max_by(|a, b| a.round.created.cmp(&b.round.created))
            .ok_or_else(|| Error::Usage("no PDF round to answer; import one first".into()))?,
    };
    let src_pdf = pdf.map_or_else(|| p.root.join(&r.round.source.file), Path::to_path_buf);
    let bytes = read_limited(&src_pdf, p.config.limits.max_pdf_mb)?;
    if pdf.is_none() && sha256_hex(&bytes) != r.round.source.sha256 {
        return Err(Error::Input(format!(
            "{} changed since it was imported; pass the original with --pdf",
            src_pdf.display()
        )));
    }
    let items: Vec<&Item> = r.items.iter().collect();
    let (out, n) =
        answer_pdf(&bytes, &items, &p.config.author).map_err(|e| Error::Input(e.to_string()))?;
    write_atomic(output, &out)?;
    if !quiet {
        println!(
            "Wrote {} with {n} repl{}.",
            output.display(),
            if n == 1 { "y" } else { "ies" }
        );
    }
    Ok(0)
}

fn cmd_inline(p: &Project, ids: &[String], todo: bool, dry_run: bool, quiet: bool) -> Result<u8> {
    let rounds = p.rounds()?;
    let (sources, _) = p.load_sources()?;
    let items: Vec<&Item> = if ids.is_empty() {
        rounds
            .iter()
            .flat_map(|r| r.items.iter())
            .filter(|i| i.status == Status::Open)
            .collect()
    } else {
        ids.iter()
            .map(|id| p.find_item(&rounds, id).map(|(_, i)| i))
            .collect::<Result<_>>()?
    };
    let mut edits = Vec::new();
    for it in items {
        let Some(t) = &it.target else { continue };
        let Some(src) = sources.get(&t.file) else {
            continue;
        };
        let text: String = it.body.split_whitespace().collect::<Vec<_>>().join(" ");
        let text = if text.is_empty() {
            format!("({})", it.kind.as_str())
        } else {
            text
        };
        if todo {
            let at = t.range.start;
            edits.push(Edit {
                file: t.file.clone(),
                range: at..at,
                replacement: format!("\\todo{{{}: {}}}", it.id, tex_escape(&text)),
                item: it.id.to_string(),
            });
        } else {
            let line_start = src.line_range(t.line as usize).map_or(0, |r| r.start);
            let indent: String = src.text[line_start..]
                .chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .collect();
            let clean: String = text.chars().filter(|c| !c.is_control()).collect();
            edits.push(Edit {
                file: t.file.clone(),
                range: line_start..line_start,
                replacement: format!(
                    "{indent}% proofback {} ({}): {clean}\n",
                    it.id,
                    it.kind.as_str()
                ),
                item: it.id.to_string(),
            });
        }
    }
    // Several comments may land on the same line; merge them.
    edits.sort_by(|a, b| (a.file.as_str(), a.range.start).cmp(&(b.file.as_str(), b.range.start)));
    let mut merged: Vec<Edit> = Vec::new();
    for e in edits {
        match merged.last_mut() {
            Some(m) if m.file == e.file && m.range == e.range => {
                m.replacement.push_str(&e.replacement);
            }
            _ => merged.push(e),
        }
    }
    let changes = apply_in_memory(&merged, &sources)?;
    if dry_run {
        print!("{}", unified_diff(&changes));
    } else {
        write_changes(p, &changes)?;
        if !quiet {
            println!(
                "Inserted {} comment(s) in {} file(s). Run `proofback sync` afterwards.",
                merged.len(),
                changes.len()
            );
        }
    }
    Ok(0)
}

fn cmd_stats(p: &Project, json: bool) -> Result<u8> {
    let rounds = p.rounds()?;
    let mut by_status: BTreeMap<&str, usize> = BTreeMap::new();
    let mut by_file: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_author: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_round: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for r in &rounds {
        for i in &r.items {
            *by_status.entry(i.status.as_str()).or_default() += 1;
            *by_file
                .entry(
                    i.target
                        .as_ref()
                        .map_or_else(|| "(unlocated)".into(), |t| t.file.clone()),
                )
                .or_default() += 1;
            *by_author
                .entry(i.author.clone().unwrap_or_else(|| "(unknown)".into()))
                .or_default() += 1;
            let e = by_round.entry(&r.round.name).or_default();
            e.0 += 1;
            e.1 += usize::from(matches!(i.status, Status::Open | Status::Discuss));
        }
    }
    if json {
        let rounds_json: BTreeMap<&str, serde_json::Value> = by_round
            .iter()
            .map(|(k, (t, o))| (*k, serde_json::json!({"total": t, "open": o})))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({"rounds": rounds_json, "status": by_status, "files": by_file, "reviewers": by_author}))
                .expect("json")
        );
        return Ok(0);
    }
    for (name, (t, o)) in &by_round {
        println!("{name}: {t} items, {o} open");
    }
    let fmt = |m: &BTreeMap<String, usize>| {
        m.iter()
            .map(|(k, v)| format!("{k} {v}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    println!(
        "status: {}",
        by_status
            .iter()
            .map(|(k, v)| format!("{k} {v}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!("files: {}", fmt(&by_file));
    println!("reviewers: {}", fmt(&by_author));
    Ok(0)
}

fn cmd_doctor(start: &Path, json: bool, style: &Style) -> u8 {
    let mut checks: Vec<(&str, &str, String)> = Vec::new();
    let project = Project::discover(start);
    match &project {
        Ok(p) => {
            checks.push(("ok", "project", p.root.display().to_string()));
            let main = p.root.join(&p.config.main);
            if main.is_file() {
                checks.push(("ok", "main file", p.config.main.clone()));
            } else {
                checks.push((
                    "fail",
                    "main file",
                    format!(
                        "{} not found; set `main` in .proofback/config.toml",
                        p.config.main
                    ),
                ));
            }
            match p.load_sources() {
                Ok((s, skipped)) => {
                    checks.push((
                        "ok",
                        "sources",
                        format!("{} .tex file(s)", s.files().count()),
                    ));
                    for sk in skipped {
                        checks.push(("warn", "sources", format!("skipped {sk}")));
                    }
                }
                Err(e) => checks.push(("fail", "sources", e.to_string())),
            }
            let pdf = p.build_dir().join(
                Path::new(&p.config.main)
                    .with_extension("pdf")
                    .file_name()
                    .unwrap_or_default(),
            );
            match find_synctex(p, &pdf, None) {
                Some(st) => {
                    let stale = std::fs::metadata(&st)
                        .and_then(|m| m.modified())
                        .ok()
                        .zip(std::fs::metadata(&main).and_then(|m| m.modified()).ok());
                    match stale {
                        Some((s, m)) if m > s => checks.push((
                            "warn",
                            "SyncTeX",
                            format!(
                                "{} is older than {}; rebuild with -synctex=1",
                                st.display(),
                                p.config.main
                            ),
                        )),
                        _ => checks.push(("ok", "SyncTeX", st.display().to_string())),
                    }
                }
                None => checks.push((
                    "warn",
                    "SyncTeX",
                    "no SyncTeX file found; build with `-synctex=1` for precise anchoring".into(),
                )),
            }
            match git::head(&p.root) {
                Some(h) => checks.push(("ok", "git", format!("HEAD {}", &h[..h.len().min(12)]))),
                None => checks.push((
                    "warn",
                    "git",
                    "not a git repository; `--at` and commit links are unavailable".into(),
                )),
            }
        }
        Err(e) => checks.push(("fail", "project", e.to_string())),
    }
    match proofback_pdf::load_pdfium(None) {
        Ok(_) => checks.push(("ok", "PDFium", "available".into())),
        Err(e) => checks.push(("fail", "PDFium", e.to_string())),
    }
    let failed = checks.iter().any(|c| c.0 == "fail");
    if json {
        let v: Vec<_> = checks
            .iter()
            .map(|(s, n, d)| serde_json::json!({"status": s, "check": n, "detail": d}))
            .collect();
        println!("{}", serde_json::to_string_pretty(&v).expect("json"));
    } else {
        for (s, n, d) in &checks {
            let tag = match *s {
                "ok" => style.confidence(Confidence::Exact).replace("exact", " ok "),
                "warn" => style.confidence(Confidence::Line).replace("line", "warn"),
                _ => style.confidence(Confidence::Page).replace("page", "fail"),
            };
            println!("[{tag}] {n}: {d}");
        }
    }
    if failed { 3 } else { 0 }
}
