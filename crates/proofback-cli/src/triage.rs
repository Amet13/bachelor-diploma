//! `proofback triage`: a keyboard UI for working through items.
//!
//! The state machine ([`App`]) is independent of the terminal so it can be tested;
//! [`draw`] renders it with ratatui.

use proofback_anchor::Sources;
use proofback_model::{Confidence, Item, RoundFile, Status};
use proofback_store::edit::{ResolveOptions, apply_in_memory, edit_for, resolve, write_changes};
use proofback_store::{Error, Project, Result};
use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

/// What a key press asks the outer loop to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    Quit,
    SetStatus(Status),
    Reply(String),
    Apply,
    /// The filter changed; rebuild the list.
    Refilter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Browse,
    /// Typing a reply.
    Reply(String),
}

/// Triage state: which item is selected, the mode, and a status message.
#[derive(Debug)]
pub struct App {
    /// (round index, item index) in display order.
    pub order: Vec<(usize, usize)>,
    pub selected: usize,
    pub mode: Mode,
    pub message: String,
    pub show_all: bool,
}

impl App {
    pub fn new(rounds: &[RoundFile], show_all: bool) -> Self {
        let mut app = Self {
            order: Vec::new(),
            selected: 0,
            mode: Mode::Browse,
            message: String::new(),
            show_all,
        };
        app.rebuild(rounds);
        app.message = "j/k move · d done · w won't fix · x discuss · o open · r reply · a apply edit · f filter · q quit".into();
        app
    }

    /// Recompute the display order (open items first unless showing all).
    pub fn rebuild(&mut self, rounds: &[RoundFile]) {
        let current = self.order.get(self.selected).copied();
        let mut order: Vec<(usize, usize)> = rounds
            .iter()
            .enumerate()
            .flat_map(|(ri, r)| r.items.iter().enumerate().map(move |(ii, _)| (ri, ii)))
            .filter(|&(ri, ii)| {
                self.show_all
                    || matches!(rounds[ri].items[ii].status, Status::Open | Status::Discuss)
            })
            .collect();
        order.sort_by(|a, b| {
            let ia = &rounds[a.0].items[a.1];
            let ib = &rounds[b.0].items[b.1];
            let ka = ia.target.as_ref().map(|t| (t.file.clone(), t.range.start));
            let kb = ib.target.as_ref().map(|t| (t.file.clone(), t.range.start));
            ka.is_none().cmp(&kb.is_none()).then(ka.cmp(&kb))
        });
        self.order = order;
        self.selected = current
            .and_then(|c| self.order.iter().position(|o| *o == c))
            .unwrap_or(self.selected)
            .min(self.order.len().saturating_sub(1));
    }

    pub fn current(&self) -> Option<(usize, usize)> {
        self.order.get(self.selected).copied()
    }

    /// Handle one key press.
    pub fn key(&mut self, k: KeyEvent) -> Action {
        if k.kind != KeyEventKind::Press {
            return Action::None;
        }
        if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c') {
            return Action::Quit;
        }
        match &mut self.mode {
            Mode::Reply(buf) => match k.code {
                KeyCode::Esc => {
                    self.mode = Mode::Browse;
                    self.message = "Reply cancelled.".into();
                    Action::None
                }
                KeyCode::Enter => {
                    let text = buf.trim().to_owned();
                    self.mode = Mode::Browse;
                    if text.is_empty() {
                        Action::None
                    } else {
                        Action::Reply(text)
                    }
                }
                KeyCode::Backspace => {
                    buf.pop();
                    Action::None
                }
                KeyCode::Char(c) => {
                    if buf.len() < 4000 {
                        buf.push(c);
                    }
                    Action::None
                }
                _ => Action::None,
            },
            Mode::Browse => match k.code {
                KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
                KeyCode::Char('j') | KeyCode::Down => {
                    if self.selected + 1 < self.order.len() {
                        self.selected += 1;
                    }
                    Action::None
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.selected = self.selected.saturating_sub(1);
                    Action::None
                }
                KeyCode::Char('g') | KeyCode::Home => {
                    self.selected = 0;
                    Action::None
                }
                KeyCode::Char('G') | KeyCode::End => {
                    self.selected = self.order.len().saturating_sub(1);
                    Action::None
                }
                KeyCode::Char('d') => Action::SetStatus(Status::Done),
                KeyCode::Char('w') => Action::SetStatus(Status::Wontfix),
                KeyCode::Char('x') => Action::SetStatus(Status::Discuss),
                KeyCode::Char('o') => Action::SetStatus(Status::Open),
                KeyCode::Char('u') => Action::SetStatus(Status::Duplicate),
                KeyCode::Char('a') => Action::Apply,
                KeyCode::Char('f') => {
                    self.show_all = !self.show_all;
                    self.message = if self.show_all {
                        "Showing all items.".into()
                    } else {
                        "Showing open items.".into()
                    };
                    Action::Refilter
                }
                KeyCode::Char('r') => {
                    if self.current().is_some() {
                        self.mode = Mode::Reply(String::new());
                        self.message = "Type your reply, Enter to save, Esc to cancel.".into();
                    }
                    Action::None
                }
                _ => Action::None,
            },
        }
    }
}

/// Render the UI.
pub fn draw(f: &mut Frame<'_>, app: &App, rounds: &[RoundFile], sources: &Sources) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(f.area());
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(outer[0]);

    let items: Vec<ListItem<'_>> = app
        .order
        .iter()
        .map(|&(ri, ii)| {
            let it = &rounds[ri].items[ii];
            let loc = it.target.as_ref().map_or_else(
                || "unlocated".to_owned(),
                |t| format!("{}:{}", t.file, t.line),
            );
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:<9}", it.status.as_str()),
                    status_style(it.status),
                ),
                Span::raw(format!("{:<9} ", it.kind.as_str())),
                Span::styled(loc, Style::default().fg(conf_color(it.anchor.confidence))),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    if !app.order.is_empty() {
        state.select(Some(app.selected));
    }
    let title = format!(
        " items ({}{}) ",
        app.order.len(),
        if app.show_all { ", all" } else { ", open" }
    );
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    f.render_stateful_widget(list, cols[0], &mut state);

    let detail = app.current().map_or_else(
        || vec![Line::from("Nothing to triage. Press f to show all items.")],
        |(ri, ii)| detail_lines(&rounds[ri].items[ii], sources),
    );
    f.render_widget(
        Paragraph::new(detail)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title(" detail ")),
        cols[1],
    );

    let bottom = match &app.mode {
        Mode::Reply(buf) => format!("reply> {buf}_"),
        Mode::Browse => app.message.clone(),
    };
    f.render_widget(
        Paragraph::new(bottom).block(Block::default().borders(Borders::ALL)),
        outer[1],
    );
}

fn status_style(s: Status) -> Style {
    match s {
        Status::Open => Style::default().add_modifier(Modifier::BOLD),
        Status::Done => Style::default().fg(Color::Green),
        Status::Discuss => Style::default().fg(Color::Magenta),
        Status::Wontfix | Status::Duplicate => Style::default().fg(Color::DarkGray),
    }
}

fn conf_color(c: Confidence) -> Color {
    match c {
        Confidence::Exact => Color::Green,
        Confidence::Line => Color::Yellow,
        Confidence::Region => Color::Cyan,
        Confidence::Page => Color::Red,
    }
}

fn detail_lines<'a>(it: &Item, sources: &Sources) -> Vec<Line<'a>> {
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                it.id.to_string(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("  {}  ", it.kind.as_str())),
            Span::styled(
                it.anchor.confidence.as_str().to_owned(),
                Style::default().fg(conf_color(it.anchor.confidence)),
            ),
            Span::raw(format!("  {}", it.anchor.reason)),
        ]),
        Line::from(""),
    ];
    let who = it.author.clone().unwrap_or_else(|| "Reviewer".into());
    let body = if it.body.is_empty() {
        "(no note)".to_owned()
    } else {
        clean(&it.body)
    };
    lines.push(Line::from(vec![
        Span::styled(format!("{who}: "), Style::default().fg(Color::Yellow)),
        Span::raw(body),
    ]));
    for t in &it.thread {
        lines.push(Line::from(format!("  ↳ {}: {}", t.author, clean(&t.body))));
    }
    if let Some(s) = &it.suggestion {
        if let Some(r) = &s.replace_with {
            lines.push(Line::from(format!("suggestion: replace with {r:?}")));
        }
        if let Some(i) = &s.insert {
            lines.push(Line::from(format!("suggestion: insert {i:?}")));
        }
    }
    lines.push(Line::from(""));
    if let Some(t) = &it.target {
        if let Some(src) = sources.get(&t.file) {
            lines.push(Line::styled(
                format!("{}:{}", t.file, t.line),
                Style::default().add_modifier(Modifier::DIM),
            ));
            let first = t.line.saturating_sub(3).max(1);
            let last = (t.end_line + 3).min(src.line_count() as u32);
            for l in first..=last {
                if let Some(r) = src.line_range(l as usize) {
                    let mark = if l >= t.line && l <= t.end_line {
                        ">"
                    } else {
                        " "
                    };
                    let style = if mark == ">" {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default()
                    };
                    lines.push(Line::styled(
                        format!("{mark}{l:>5} | {}", clean(&src.text[r])),
                        style,
                    ));
                }
            }
        }
    }
    for r in &it.replies {
        lines.push(Line::from(""));
        lines.push(Line::from(format!("you ({}): {}", r.at, clean(&r.body))));
    }
    lines
}

/// Strip control characters so reviewer text cannot drive the terminal.
fn clean(s: &str) -> String {
    s.chars()
        .map(|c| if c == '\n' || c == '\t' { ' ' } else { c })
        .filter(|c| !c.is_control())
        .collect()
}

/// Run the interactive UI.
pub fn run(
    p: &Project,
    show_all: bool,
    now: &dyn Fn() -> String,
    head: Option<&str>,
) -> Result<()> {
    let mut rounds = p.rounds()?;
    let (mut sources, _) = p.load_sources()?;
    let mut app = App::new(&rounds, show_all);
    let mut terminal = ratatui::try_init()
        .map_err(|e| Error::Input(format!("cannot start the terminal UI: {e}")))?;
    let result = (|| -> Result<()> {
        loop {
            terminal
                .draw(|f| draw(f, &app, &rounds, &sources))
                .map_err(|e| Error::Input(e.to_string()))?;
            let Event::Key(k) = event::read().map_err(|e| Error::Input(e.to_string()))? else {
                continue;
            };
            let action = app.key(k);
            if action == Action::Refilter {
                app.rebuild(&rounds);
                continue;
            }
            let Some((ri, ii)) = app.current() else {
                if action == Action::Quit {
                    return Ok(());
                }
                continue;
            };
            match action {
                Action::None | Action::Refilter => {}
                Action::Quit => return Ok(()),
                Action::SetStatus(status) => {
                    let opts = ResolveOptions {
                        status: Some(status),
                        author: p.config.author.clone(),
                        now: now(),
                        commit: head.map(str::to_owned),
                        ..Default::default()
                    };
                    match resolve(&mut rounds[ri].items[ii], &sources, &opts) {
                        Ok(()) => {
                            p.save_round(&rounds[ri])?;
                            app.message = format!("Marked {}.", status.as_str());
                            app.rebuild(&rounds);
                        }
                        Err(e) => app.message = e.to_string(),
                    }
                }
                Action::Reply(text) => {
                    let opts = ResolveOptions {
                        reply: Some(text),
                        author: p.config.author.clone(),
                        now: now(),
                        ..Default::default()
                    };
                    resolve(&mut rounds[ri].items[ii], &sources, &opts)?;
                    p.save_round(&rounds[ri])?;
                    app.message = "Reply saved.".into();
                }
                Action::Apply => {
                    let it = &rounds[ri].items[ii];
                    match edit_for(it, &sources)
                        .and_then(|e| apply_in_memory(&[e], &sources))
                        .and_then(|c| write_changes(p, &c))
                    {
                        Ok(()) => {
                            sources = p.load_sources()?.0;
                            let opts = ResolveOptions {
                                status: Some(Status::Done),
                                author: p.config.author.clone(),
                                now: now(),
                                commit: head.map(str::to_owned),
                                ..Default::default()
                            };
                            resolve(&mut rounds[ri].items[ii], &sources, &opts)?;
                            p.save_round(&rounds[ri])?;
                            app.message = "Applied and marked done.".into();
                            app.rebuild(&rounds);
                        }
                        Err(e) => app.message = e.to_string(),
                    }
                }
            }
        }
    })();
    ratatui::restore();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use proofback_model::{Anchor, AnchorState, ItemId, Kind, Origin, Round, Source, SourceKind};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn rounds() -> Vec<RoundFile> {
        let mk = |n: &str, status: Status| Item {
            id: ItemId::derive("r", n),
            round: "r".into(),
            kind: Kind::Comment,
            author: Some("Prof. Hale".into()),
            created: None,
            body: format!("note {n}\x1b[31m"),
            suggestion: None,
            target: None,
            anchor: Anchor {
                state: AnchorState::Anchored,
                confidence: Confidence::Page,
                reason: "test".into(),
            },
            origin: Origin::Notes {
                line: 1,
                text: String::new(),
                page: None,
            },
            status,
            replies: vec![],
            resolution: proofback_model::Resolution::default(),
            thread: vec![],
        };
        vec![RoundFile::new(
            Round {
                name: "r".into(),
                created: "t".into(),
                source: Source {
                    kind: SourceKind::Notes,
                    file: "n.txt".into(),
                    sha256: String::new(),
                    synctex: None,
                    synctex_sha256: None,
                    commit: None,
                },
                anonymized: false,
            },
            vec![
                mk("a", Status::Open),
                mk("b", Status::Done),
                mk("c", Status::Open),
            ],
        )]
    }

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    #[test]
    fn navigation_filters_and_reply_mode() {
        let r = rounds();
        let mut app = App::new(&r, false);
        assert_eq!(app.order.len(), 2);
        assert_eq!(app.key(key(KeyCode::Char('j'))), Action::None);
        assert_eq!(app.selected, 1);
        app.key(key(KeyCode::Char('j')));
        assert_eq!(app.selected, 1, "stays at the end");
        assert_eq!(
            app.key(key(KeyCode::Char('d'))),
            Action::SetStatus(Status::Done)
        );
        assert_eq!(app.key(key(KeyCode::Char('f'))), Action::Refilter);
        app.rebuild(&r);
        assert_eq!(app.order.len(), 3);
        app.key(key(KeyCode::Char('r')));
        for c in "Thanks".chars() {
            app.key(key(KeyCode::Char(c)));
        }
        app.key(key(KeyCode::Backspace));
        assert_eq!(app.key(key(KeyCode::Enter)), Action::Reply("Thank".into()));
        assert_eq!(app.mode, Mode::Browse);
        assert_eq!(app.key(key(KeyCode::Char('q'))), Action::Quit);
    }

    #[test]
    fn renders_without_control_characters() {
        let r = rounds();
        let app = App::new(&r, true);
        let mut t = Terminal::new(TestBackend::new(100, 20)).unwrap();
        t.draw(|f| draw(f, &app, &r, &Sources::default())).unwrap();
        let buf = t.backend().buffer().clone();
        let text: String = buf
            .content()
            .iter()
            .map(|c| c.symbol().to_owned())
            .collect();
        assert!(text.contains("note a"));
        assert!(!text.contains('\x1b'));
    }
}
