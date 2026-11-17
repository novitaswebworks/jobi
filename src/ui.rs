use crate::store::{Application, Interview, STAGES, Store};
use anyhow::{Context, Result, bail};
use chrono::Local;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use std::{
    cell::Cell,
    io::{IsTerminal, Write},
    time::Duration,
};
const ACCENT: Color = Color::Cyan;

pub fn order_actions(rows: &mut Vec<Application>) {
    rows.retain(|a| !a.action.is_empty());
    rows.sort_by(|a, b| {
        (a.due.is_empty(), &a.due, &a.company, a.id).cmp(&(
            b.due.is_empty(),
            &b.due,
            &b.company,
            b.id,
        ))
    });
}
struct Form {
    id: Option<i64>,
    values: Vec<String>,
    field: usize,
    error: String,
}
impl Form {
    fn new(a: Option<&Application>) -> Self {
        Self {
            id: a.map(|a| a.id),
            values: a
                .map(|a| {
                    vec![
                        a.company.clone(),
                        a.role.clone(),
                        a.stage.clone(),
                        a.action.clone(),
                        a.due.clone(),
                    ]
                })
                .unwrap_or(vec![
                    String::new(),
                    String::new(),
                    "Applied".into(),
                    String::new(),
                    String::new(),
                ]),
            field: 0,
            error: String::new(),
        }
    }
}
enum Mode {
    Browse,
    Search,
    Form(Form),
    Note(String),
    Help,
    Archive(i64, bool),
    /// kind (0 posting, 1 interview, 2 document, 3 edit interview), fields, focused field, error, record id
    Prepare(usize, Vec<String>, usize, String, Option<i64>),
    /// application row, selected interview/document
    Inspect(usize, usize),
    /// is_interview, id, label
    Remove(bool, i64, String),
}
/// Fields that hold long text and can be edited in $VISUAL/$EDITOR.
fn multiline(kind: usize, field: usize) -> bool {
    matches!((kind, field), (0, 1) | (1 | 3, 4))
}
fn edit_externally(text: &str) -> Result<String> {
    let cmd = ["VISUAL", "EDITOR"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "vi".into());
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let path = std::env::temp_dir().join(format!("jobi-edit-{}-{nanos}.txt", std::process::id()));
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
    opts.open(&path)?.write_all(text.as_bytes())?;
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{cmd} \"$1\""))
        .arg("jobi-editor")
        .arg(&path)
        .status();
    let out = std::fs::read_to_string(&path);
    let _ = std::fs::remove_file(&path);
    let status = status.context("Cannot start editor; set $VISUAL or $EDITOR")?;
    if !status.success() {
        bail!("Editor exited with {status}; text unchanged");
    }
    Ok(out?.trim_end_matches('\n').to_string())
}
struct App {
    tab: usize,
    query: String,
    selected: usize,
    rows: Vec<Application>,
    history: Vec<String>,
    scroll: u16,
    mode: Mode,
    status: String,
    /// set by the key handler; the run loop then suspends the TUI to launch the editor
    editor: bool,
    /// largest useful details scroll, measured by the last draw
    max_scroll: Cell<u16>,
}
impl App {
    fn refresh(&mut self, store: &Store) -> Result<()> {
        self.rows = store.list(self.tab == 2)?;
        if self.tab == 0 {
            order_actions(&mut self.rows);
        }
        let q = self.query.to_lowercase();
        self.rows.retain(|a| {
            format!("{} {} {} {}", a.company, a.role, a.stage, a.action)
                .to_lowercase()
                .contains(&q)
        });
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
        self.history = match self.rows.get(self.selected) {
            Some(a) => store.history(a.id)?,
            None => vec![],
        };
        Ok(())
    }
}
pub fn run(store: &mut Store, add: bool) -> Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!(
            "The interactive app requires a terminal. Use `jobi list`, `jobi today`, or `jobi add --company NAME --role ROLE`."
        );
    }
    let mut app = App {
        tab: 0,
        query: String::new(),
        selected: 0,
        rows: vec![],
        history: vec![],
        scroll: 0,
        mode: if add {
            Mode::Form(Form::new(None))
        } else {
            Mode::Browse
        },
        status: "Local storage · no account required".into(),
        editor: false,
        max_scroll: Cell::new(0),
    };
    app.refresh(store)?;
    let mut terminal = ratatui::init();
    let result = (|| -> Result<()> {
        loop {
            terminal.draw(|f| draw(f, &app, store))?;
            if !event::poll(Duration::from_millis(250))? {
                continue;
            }
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    break;
                }
                match handle(&mut app, store, key) {
                    Ok(true) => break,
                    Ok(false) => {}
                    Err(e) => app.status = format!("Error: {e}"),
                }
                if std::mem::take(&mut app.editor) {
                    ratatui::restore();
                    if let Mode::Prepare(_, fields, field, error, _) = &mut app.mode {
                        match edit_externally(&fields[*field]) {
                            Ok(t) => fields[*field] = t,
                            Err(e) => *error = e.to_string(),
                        }
                    }
                    terminal = ratatui::init();
                }
                app.refresh(store)?;
            }
        }
        Ok(())
    })();
    ratatui::restore();
    result
}
fn handle(app: &mut App, store: &mut Store, key: KeyEvent) -> Result<bool> {
    match &mut app.mode {
        Mode::Form(form) => match key.code {
            KeyCode::Esc => app.mode = Mode::Browse,
            KeyCode::Tab | KeyCode::Down => form.field = (form.field + 1) % 5,
            KeyCode::BackTab | KeyCode::Up => form.field = (form.field + 4) % 5,
            KeyCode::Left | KeyCode::Right if form.field == 2 => {
                let i = STAGES
                    .iter()
                    .position(|s| *s == form.values[2])
                    .unwrap_or(0);
                let next = if key.code == KeyCode::Right {
                    (i + 1) % STAGES.len()
                } else {
                    (i + STAGES.len() - 1) % STAGES.len()
                };
                form.values[2] = STAGES[next].into();
            }
            KeyCode::Enter => {
                let v = &form.values;
                match store.save(form.id, &v[0], &v[1], &v[2], &v[3], &v[4]) {
                    Ok(_) => {
                        app.mode = Mode::Browse;
                        app.tab = 1;
                        app.query.clear();
                        app.status = "Application saved".into();
                    }
                    Err(e) => form.error = e.to_string(),
                }
            }
            KeyCode::Backspace if form.field != 2 => {
                form.values[form.field].pop();
            }
            KeyCode::Char(c)
                if form.field != 2 && !key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                form.values[form.field].push(c)
            }
            _ => {}
        },
        Mode::Note(text) => match key.code {
            KeyCode::Esc => app.mode = Mode::Browse,
            KeyCode::Enter => {
                if let Some(a) = app.rows.get(app.selected) {
                    store.note(a.id, text)?;
                    app.status = "Note saved".into();
                }
                app.mode = Mode::Browse;
            }
            KeyCode::Backspace => {
                text.pop();
            }
            KeyCode::Char(c) => text.push(c),
            _ => {}
        },
        Mode::Search => match key.code {
            KeyCode::Esc => {
                app.query.clear();
                app.mode = Mode::Browse;
            }
            KeyCode::Enter => app.mode = Mode::Browse,
            KeyCode::Backspace => {
                app.query.pop();
                app.selected = 0;
            }
            KeyCode::Char(c) => {
                app.query.push(c);
                app.selected = 0;
            }
            _ => {}
        },
        Mode::Prepare(kind, fields, field, error, rec) => match key.code {
            KeyCode::Esc => app.mode = Mode::Browse,
            KeyCode::Tab | KeyCode::Down => *field = (*field + 1) % fields.len(),
            KeyCode::BackTab | KeyCode::Up => *field = (*field + fields.len() - 1) % fields.len(),
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::CONTROL) => {
                fields[*field].push('\n')
            }
            KeyCode::Enter => {
                let id = app.rows.get(app.selected).map(|a| a.id);
                if let Some(id) = id {
                    let res = match *kind {
                        0 => store.save_posting(id, &fields[0], &fields[1]),
                        1 => store.add_interview(
                            id, &fields[0], &fields[1], &fields[2], &fields[3], &fields[4],
                        ),
                        3 => store.update_interview(&Interview {
                            id: rec.unwrap_or_default(),
                            starts_at: fields[0].clone(),
                            timezone: fields[1].clone(),
                            kind: fields[2].clone(),
                            url: fields[3].clone(),
                            notes: fields[4].clone(),
                        }),
                        _ => store.add_document(id, &fields[0], &fields[1]),
                    };
                    match res {
                        Ok(()) => {
                            app.mode = Mode::Browse;
                            app.status = "Saved".into();
                        }
                        Err(e) => *error = e.to_string(),
                    }
                }
            }
            KeyCode::Backspace => {
                fields[*field].pop();
            }
            KeyCode::Char('e')
                if key.modifiers.contains(KeyModifiers::CONTROL) && multiline(*kind, *field) =>
            {
                app.editor = true
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                fields[*field].push(c)
            }
            _ => {}
        },
        Mode::Inspect(row, sel) => {
            let Some(aid) = app.rows.get(*row).map(|a| a.id) else {
                app.mode = Mode::Browse;
                return Ok(false);
            };
            let ivs = store.interviews(aid)?;
            let docs = store.documents(aid)?;
            let total = ivs.len() + docs.len();
            match key.code {
                KeyCode::Esc | KeyCode::Char('q' | 'v') | KeyCode::Enter => app.mode = Mode::Browse,
                KeyCode::Down | KeyCode::Char('j') => {
                    *sel = (*sel + 1).min(total.saturating_sub(1))
                }
                KeyCode::Up | KeyCode::Char('k') => *sel = sel.saturating_sub(1),
                KeyCode::Char('e') if *sel < ivs.len() => {
                    let i = &ivs[*sel];
                    app.mode = Mode::Prepare(
                        3,
                        vec![
                            i.starts_at.clone(),
                            i.timezone.clone(),
                            i.kind.clone(),
                            i.url.clone(),
                            i.notes.clone(),
                        ],
                        0,
                        String::new(),
                        Some(i.id),
                    );
                }
                KeyCode::Char('r') => {
                    if let Some(i) = ivs.get(*sel) {
                        app.mode = Mode::Remove(true, i.id, format!("{} {}", i.starts_at, i.kind));
                    } else if let Some(d) = docs.get(*sel - ivs.len()) {
                        app.mode = Mode::Remove(false, d.id, d.label.clone());
                    }
                }
                _ => {}
            }
        }
        Mode::Remove(is_interview, id, _) => match key.code {
            KeyCode::Char('y') => {
                let res = if *is_interview {
                    store.delete_interview(*id)
                } else {
                    store.delete_document(*id)
                };
                app.mode = Mode::Browse;
                res?;
                app.status = "Removed".into();
            }
            KeyCode::Esc | KeyCode::Char('n') => app.mode = Mode::Browse,
            _ => {}
        },
        Mode::Help => app.mode = Mode::Browse,
        Mode::Archive(id, state) => match key.code {
            KeyCode::Char('y') => {
                store.archive(*id, *state)?;
                app.mode = Mode::Browse;
                app.status = "Archive status updated".into();
            }
            KeyCode::Esc | KeyCode::Char('n') => app.mode = Mode::Browse,
            _ => {}
        },
        Mode::Browse => match key.code {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Tab => {
                app.tab = (app.tab + 1) % 3;
                app.selected = 0;
                app.scroll = 0;
            }
            KeyCode::Char('1' | '2' | '3') => {
                if let KeyCode::Char(c) = key.code {
                    app.tab = (c as u8 - b'1') as usize;
                }
                app.selected = 0;
                app.scroll = 0;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.selected = (app.selected + 1).min(app.rows.len().saturating_sub(1));
                app.scroll = 0;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.selected = app.selected.saturating_sub(1);
                app.scroll = 0;
            }
            KeyCode::PageDown => app.scroll = (app.scroll + 5).min(app.max_scroll.get()),
            KeyCode::PageUp => app.scroll = app.scroll.min(app.max_scroll.get()).saturating_sub(5),
            KeyCode::Char('/') => app.mode = Mode::Search,
            KeyCode::Esc => app.query.clear(),
            KeyCode::Char('a') => app.mode = Mode::Form(Form::new(None)),
            KeyCode::Char('e') => {
                if let Some(a) = app.rows.get(app.selected) {
                    app.mode = Mode::Form(Form::new(Some(a)));
                }
            }
            KeyCode::Char('n') if !app.rows.is_empty() => {
                app.mode = Mode::Note(String::new());
            }
            KeyCode::Char('x') => {
                if let Some(a) = app.rows.get(app.selected) {
                    store.complete(a.id)?;
                    app.status = "Action completed · press e to set the next one".into();
                }
            }
            KeyCode::Char('z') => {
                if let Some(a) = app.rows.get(app.selected) {
                    app.mode = Mode::Archive(a.id, !a.archived);
                }
            }
            KeyCode::Char('p') if !app.rows.is_empty() => {
                let (url, desc) = store.posting(app.rows[app.selected].id).unwrap_or_default();
                app.mode = Mode::Prepare(0, vec![url, desc], 0, String::new(), None);
            }
            KeyCode::Char('i') if !app.rows.is_empty() => {
                app.mode = Mode::Prepare(
                    1,
                    vec![
                        String::new(),
                        "Asia/Kolkata".into(),
                        String::new(),
                        String::new(),
                        String::new(),
                    ],
                    0,
                    String::new(),
                    None,
                )
            }
            KeyCode::Char('d') if !app.rows.is_empty() => {
                app.mode = Mode::Prepare(
                    2,
                    vec![String::new(), String::new()],
                    0,
                    String::new(),
                    None,
                )
            }
            KeyCode::Char('v') if !app.rows.is_empty() => app.mode = Mode::Inspect(app.selected, 0),
            KeyCode::Char('?') => app.mode = Mode::Help,
            _ => {}
        },
    }
    Ok(false)
}
fn draw(f: &mut Frame, app: &App, store: &Store) {
    let area = f.area();
    if area.width < 50 || area.height < 18 {
        f.render_widget(
            Paragraph::new(
                "Jobi needs at least 50 columns × 18 rows.\nResize the terminal. Ctrl+C exits.",
            ),
            area,
        );
        return;
    }
    let parts = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .split(area);
    let names = ["1 Today", "2 Applications", "3 Archived"];
    let mut spans = vec![Span::styled(" JOBI  ", Style::default().fg(ACCENT).bold())];
    for (i, n) in names.iter().enumerate() {
        spans.push(Span::styled(
            format!(" {n} "),
            if app.tab == i {
                Style::default().fg(Color::Black).bg(ACCENT).bold()
            } else {
                Style::default().fg(Color::Gray)
            },
        ));
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).block(Block::default().borders(Borders::BOTTOM)),
        parts[0],
    );
    let panes = if area.width >= 100 {
        Layout::horizontal([Constraint::Percentage(48), Constraint::Percentage(52)]).split(parts[1])
    } else {
        Layout::vertical([Constraint::Percentage(45), Constraint::Percentage(55)]).split(parts[1])
    };
    let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
    let items: Vec<ListItem> = app
        .rows
        .iter()
        .map(|a| {
            let due = if a.due.is_empty() {
                "No due date".into()
            } else if a.due < today {
                format!("OVERDUE {}", a.due)
            } else if a.due == today {
                "TODAY".into()
            } else {
                a.due.clone()
            };
            ListItem::new(vec![
                Line::from(format!("{} · {}", a.company, a.role)),
                Line::from(Span::styled(
                    format!("{}  |  {due}", a.stage),
                    Style::default().fg(if !a.due.is_empty() && a.due <= today {
                        Color::Yellow
                    } else {
                        Color::DarkGray
                    }),
                )),
            ])
        })
        .collect();
    let title = format!(
        " {} records · / {}{} ",
        app.rows.len(),
        app.query,
        if matches!(app.mode, Mode::Search) {
            "▏"
        } else {
            ""
        }
    );
    if items.is_empty() {
        f.render_widget(Paragraph::new(if app.query.is_empty(){"Nothing here yet.\n\n[a] Add an application\n[2] Browse all applications\n\nToday shows open actions, with overdue and upcoming dates first."}else{"No matching applications. Esc clears search."}).wrap(Wrap{trim:false}).block(Block::bordered().title(title)),panes[0]);
    } else {
        let mut state = ListState::default().with_selected(Some(app.selected));
        f.render_stateful_widget(
            List::new(items)
                .block(Block::bordered().title(title))
                .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White))
                .highlight_symbol("▌ "),
            panes[0],
            &mut state,
        );
    }
    let detail = if let Some(a) = app.rows.get(app.selected) {
        format!(
            "{}\n{}\n\nStage: {}\nNext action: {}\nDue: {}\n\n[p] Posting · [i] Interview · [d] Link document · [v] View details\n\nACTIVITY · newest first\n{}",
            a.company,
            a.role,
            a.stage,
            if a.action.is_empty() {
                "None — press e to add one"
            } else {
                &a.action
            },
            if a.due.is_empty() { "Not set" } else { &a.due },
            app.history.join("\n\n")
        )
    } else {
        "Your next career move, one action at a time.\n\nEverything stays on this computer.\nPress ? for keyboard help.".into()
    };
    let details = Paragraph::new(detail).wrap(Wrap { trim: false });
    let inner = Block::bordered().inner(panes[1]);
    let total = u16::try_from(details.line_count(inner.width)).unwrap_or(u16::MAX);
    let max_scroll = total.saturating_sub(inner.height);
    app.max_scroll.set(max_scroll);
    f.render_widget(
        details
            .scroll((app.scroll.min(max_scroll), 0))
            .block(Block::bordered().title(if max_scroll > 0 {
                " Details & history · PgUp/PgDn scroll "
            } else {
                " Details & history "
            })),
        panes[1],
    );
    f.render_widget(
        Paragraph::new(format!(
            "a Add · e Edit · n Note · x Done · z Archive/restore · ? Help · q Quit\n{}",
            app.status
        ))
        .style(Style::default().fg(ACCENT)),
        parts[2],
    );
    match &app.mode {
        Mode::Form(form) => {
            let r = popup(f, 70, 17);
            let labels = [
                "Company *",
                "Role *",
                "Stage ←/→",
                "Next action",
                "Due YYYY-MM-DD",
            ];
            let mut lines = vec![
                Line::from(Span::styled(
                    if form.id.is_some() {
                        "Edit application"
                    } else {
                        "New application"
                    },
                    Style::default().bold().fg(ACCENT),
                )),
                Line::from(""),
            ];
            for (i, label) in labels.iter().enumerate() {
                lines.push(Line::from(Span::styled(
                    format!(
                        "{label}: {}{}",
                        form.values[i],
                        if form.field == i { " ▏" } else { "" }
                    ),
                    if form.field == i {
                        Style::default().fg(ACCENT).bold()
                    } else {
                        Style::default()
                    },
                )));
                lines.push(Line::from(""));
            }
            lines.push(Line::from(
                "Tab/↑/↓ Field · ←/→ Stage · Enter Save · Esc Cancel",
            ));
            lines.push(Line::from(Span::styled(
                &form.error,
                Style::default().fg(Color::Red),
            )));
            f.render_widget(
                Paragraph::new(lines)
                    .block(Block::bordered().title(" Jobi "))
                    .wrap(Wrap { trim: false }),
                r,
            );
        }
        Mode::Note(text) => {
            let r = popup(f, 70, 8);
            f.render_widget(
                Paragraph::new(format!("{text}▏\n\nEnter Save · Esc Cancel"))
                    .wrap(Wrap { trim: false })
                    .block(Block::bordered().title(" Add a note ")),
                r,
            );
        }
        Mode::Help => {
            let r = popup(f, 70, 16);
            f.render_widget(
                Paragraph::new(
                    "1/2/3, Tab  Switch views\nj/k, ↑/↓    Select application\n/           Search (Esc clears)\na / e       Add / edit application\nn           Add a dated note\nx           Complete next action\nz           Archive / restore\np / i / d   Posting / interview / link document\nv           Details: j/k select, e edit interview,\n            r remove interview / unlink document\nCtrl+E      In a form: edit long text in $EDITOR\nPgUp/PgDn   Scroll details and history\nq, Ctrl+C   Exit\nAny key closes. History times are UTC.",
                )
                .block(Block::bordered().title(" Keyboard help ")),
                r,
            );
        }
        Mode::Prepare(kind, fields, field, error, _) => {
            let (title, labels) = match *kind {
                0 => (
                    "Job posting",
                    vec!["URL", "Description (Ctrl+E opens your editor)"],
                ),
                1 | 3 => (
                    if *kind == 3 {
                        "Edit interview"
                    } else {
                        "Schedule interview"
                    },
                    vec![
                        "Starts at RFC3339 e.g. 2026-11-20T14:00:00+05:30",
                        "Timezone",
                        "Interview type",
                        "Meeting URL",
                        "Notes (Ctrl+E opens your editor)",
                    ],
                ),
                _ => ("Link document", vec!["Label", "Existing file path"]),
            };
            let r = popup(f, 76, 18);
            let mut lines = vec![Line::from("Tab changes fields · Enter saves · Esc cancels")];
            for (i, label) in labels.iter().enumerate() {
                lines.push(Line::from(format!(
                    "{label}: {}{}",
                    fields[i].replace('\n', " ⏎ "),
                    if *field == i { " ▏" } else { "" }
                )));
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                error,
                Style::default().fg(Color::Red),
            )));
            f.render_widget(
                Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .block(Block::bordered().title(title)),
                r,
            );
        }
        Mode::Inspect(index, sel) => {
            let r = popup(f, 76, 24);
            if let Some(a) = app.rows.get(*index) {
                let posting = store.posting(a.id).unwrap_or_default();
                let ivs = store.interviews(a.id).unwrap_or_default();
                let docs = store.documents(a.id).unwrap_or_default();
                let mark = |n: usize| if n == *sel { "▌ " } else { "  " };
                let mut c = format!("{} · {}\n\nJOB POSTING\n{}\n", a.company, a.role, posting.0);
                let desc: Vec<&str> = posting.1.lines().collect();
                c += &desc.iter().take(4).copied().collect::<Vec<_>>().join("\n");
                if desc.len() > 4 {
                    c += "\n…";
                }
                c += "\n\nINTERVIEWS\n";
                if ivs.is_empty() {
                    c += "  none\n";
                }
                for (n, i) in ivs.iter().enumerate() {
                    c += &format!(
                        "{}{} {} {}\n    {} {}\n",
                        mark(n),
                        i.starts_at,
                        i.timezone,
                        i.kind,
                        i.url,
                        i.notes.replace('\n', " ⏎ ")
                    );
                }
                c += "\nDOCUMENTS\n";
                if docs.is_empty() {
                    c += "  none\n";
                }
                for (n, d) in docs.iter().enumerate() {
                    c += &format!(
                        "{}{}  {}{}\n",
                        mark(ivs.len() + n),
                        d.label,
                        d.path,
                        if d.missing() { "  (FILE MISSING)" } else { "" }
                    );
                }
                c += "\nj/k select · e edit interview · r remove/unlink · Esc close";
                f.render_widget(
                    Paragraph::new(c)
                        .wrap(Wrap { trim: false })
                        .block(Block::bordered().title("Application details")),
                    r,
                );
            }
        }
        Mode::Remove(is_interview, _, label) => {
            let r = popup(f, 60, 7);
            f.render_widget(
                Paragraph::new(format!(
                    "{} {label}?\n{}\n\ny Confirm · n / Esc Cancel",
                    if *is_interview {
                        "Remove interview"
                    } else {
                        "Unlink document"
                    },
                    if *is_interview {
                        "This cannot be undone."
                    } else {
                        "The file itself is not deleted."
                    },
                ))
                .wrap(Wrap { trim: false })
                .block(Block::bordered().title(" Confirm ")),
                r,
            );
        }
        Mode::Archive(_, state) => {
            let r = popup(f, 60, 6);
            f.render_widget(Paragraph::new(format!("{} this application?\nHistory will be preserved.\n\ny Confirm · n / Esc Cancel",if *state{"Archive"}else{"Restore"})).block(Block::bordered().title(" Confirm ")),r);
        }
        _ => {}
    }
}
fn popup(f: &mut Frame, width: u16, height: u16) -> Rect {
    let a = f.area();
    let w = width.min(a.width);
    let h = height.min(a.height);
    let r = Rect::new((a.width - w) / 2, (a.height - h) / 2, w, h);
    f.render_widget(Clear, r);
    r
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actions_are_ordered_and_empty_actions_excluded() {
        let make = |id, action: &str, due: &str| Application {
            id,
            company: "A".into(),
            role: "R".into(),
            stage: "Applied".into(),
            action: action.into(),
            due: due.into(),
            archived: false,
        };
        let mut rows = vec![
            make(1, "Call", ""),
            make(2, "Follow up", "2026-11-20"),
            make(3, "", ""),
            make(4, "Prepare", "2026-11-18"),
        ];
        order_actions(&mut rows);
        assert_eq!(rows.iter().map(|a| a.id).collect::<Vec<_>>(), vec![4, 2, 1]);
    }
    #[test]
    fn layouts_and_form_render_at_supported_sizes() -> Result<()> {
        use ratatui::backend::TestBackend;
        for (width, height) in [(110, 32), (50, 18), (30, 10)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            let store = Store(rusqlite::Connection::open_in_memory()?);
            let mut app = App {
                tab: 0,
                query: String::new(),
                selected: 0,
                rows: vec![],
                history: vec![],
                scroll: 0,
                mode: Mode::Browse,
                status: String::new(),
                editor: false,
                max_scroll: Cell::new(0),
            };
            terminal.draw(|f| draw(f, &app, &store))?;
            let content: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect();
            assert!(content.contains(if width < 50 { "Resize" } else { "JOBI" }));
            if width >= 50 {
                app.mode = Mode::Form(Form::new(None));
                terminal.draw(|f| draw(f, &app, &store))?;
                let content: String = terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .map(|c| c.symbol())
                    .collect();
                assert!(content.contains("Company *"));
                assert!(content.contains("Save"));
            }
        }
        Ok(())
    }
    #[test]
    fn external_editor_roundtrip_and_failure() {
        // Only this test touches VISUAL, so setting it here cannot race other tests.
        unsafe { std::env::set_var("VISUAL", "printf 'a\\nb\\n' >") };
        assert_eq!(edit_externally("old").unwrap(), "a\nb");
        unsafe { std::env::set_var("VISUAL", "false #") };
        assert!(edit_externally("old").is_err());
        assert!(multiline(0, 1) && multiline(3, 4) && !multiline(0, 0) && !multiline(2, 1));
    }
    #[test]
    fn inspect_remove_help_render_and_details_scroll_is_clamped() -> Result<()> {
        use ratatui::backend::TestBackend;
        let mut store = Store::initialize(rusqlite::Connection::open_in_memory()?)?;
        let id = store.save(None, "Acme", "Eng", "Applied", "", "")?;
        for n in 0..30 {
            store.note(id, &format!("note {n}"))?;
        }
        store.add_interview(
            id,
            "2026-11-20T14:00:00+05:30",
            "Asia/Kolkata",
            "phone",
            "",
            "a\nb",
        )?;
        store
            .add_document(id, "Gone", "/definitely/missing/file.pdf")
            .ok();
        let mut app = App {
            tab: 1,
            query: String::new(),
            selected: 0,
            rows: store.list(false)?,
            history: store.history(id)?,
            scroll: 9999,
            mode: Mode::Browse,
            status: String::new(),
            editor: false,
            max_scroll: Cell::new(0),
        };
        for (w, h) in [(110, 32), (50, 18)] {
            let mut t = Terminal::new(TestBackend::new(w, h))?;
            app.mode = Mode::Browse;
            t.draw(|f| draw(f, &app, &store))?;
            let max = app.max_scroll.get();
            assert!(
                max > 0,
                "30 history lines must overflow the pane at {w}x{h}"
            );
            // Handler paging never goes past the measured maximum.
            let key = KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE);
            for _ in 0..50 {
                handle(&mut app, &mut store, key)?;
            }
            assert_eq!(app.scroll, max);
            for mode in [
                Mode::Inspect(0, 0),
                Mode::Remove(true, 1, "x".into()),
                Mode::Help,
            ] {
                app.mode = mode;
                t.draw(|f| draw(f, &app, &store))?;
            }
        }
        // Inspect: remove the selected interview through the keyboard flow.
        app.mode = Mode::Inspect(0, 0);
        handle(
            &mut app,
            &mut store,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
        )?;
        assert!(matches!(app.mode, Mode::Remove(true, _, _)));
        handle(
            &mut app,
            &mut store,
            KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
        )?;
        assert!(store.interviews(id)?.is_empty());
        Ok(())
    }
}
