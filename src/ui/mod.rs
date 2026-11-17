use crate::config::Config;
use crate::dates::resolve_due;
use crate::store::{Application, Interview, STAGES, Store, due_counts};
use anyhow::{Context, Result, bail};
use chrono::Local;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
mod draw;
mod theme;
use draw::draw;
use std::{
    cell::Cell,
    io::{IsTerminal, Write},
    time::Duration,
};
use theme::Palette;

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
    fn new(a: Option<&Application>, default_stage: &str) -> Self {
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
                    default_stage.into(),
                    String::new(),
                    String::new(),
                ]),
            field: 0,
            error: String::new(),
        }
    }
}
/// The inverse of the last reversible change.
enum Undo {
    Stage(i64, String),
    Action(i64, String, String),
    Archive(i64, bool),
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
    /// rendered pipeline statistics
    Stats(String),
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
    config: Config,
    pal: Palette,
    /// open actions past due / due today across all active applications
    overdue: usize,
    due_today: usize,
    /// the last reversible change, for `u`
    undo: Option<Undo>,
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
        let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
        (self.overdue, self.due_today) = due_counts(&store.list(false)?, &today);
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
        self.history = match self.rows.get(self.selected) {
            Some(a) => store.history(a.id)?,
            None => vec![],
        };
        Ok(())
    }
}
pub fn run(store: &mut Store, add: bool, config: &Config) -> Result<()> {
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
            Mode::Form(Form::new(None, &config.default_stage))
        } else {
            Mode::Browse
        },
        status: "Local storage · no account required".into(),
        config: config.clone(),
        pal: Palette::new(config),
        overdue: 0,
        due_today: 0,
        undo: None,
        editor: false,
        max_scroll: Cell::new(0),
    };
    app.refresh(store)?;
    // SIGTERM/SIGHUP must leave the loop normally so the terminal is restored.
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    #[cfg(unix)]
    for sig in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGHUP] {
        signal_hook::flag::register(sig, stop.clone())?;
    }
    let mut terminal = ratatui::init();
    let result = (|| -> Result<()> {
        loop {
            terminal.draw(|f| draw(f, &app, store))?;
            if stop.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
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
                let due = resolve_due(&v[4], chrono::Local::now().date_naive());
                match due.and_then(|due| store.save(form.id, &v[0], &v[1], &v[2], &v[3], &due)) {
                    Ok(_) => {
                        app.scroll = 0;
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
                    app.scroll = 0;
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
        Mode::Stats(_) => app.mode = Mode::Browse,
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
                app.undo = Some(Undo::Archive(*id, !*state));
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
            KeyCode::Char('a') => app.mode = Mode::Form(Form::new(None, &app.config.default_stage)),
            KeyCode::Char('e') => {
                if let Some(a) = app.rows.get(app.selected) {
                    app.mode = Mode::Form(Form::new(Some(a), &app.config.default_stage));
                }
            }
            KeyCode::Char('n') if !app.rows.is_empty() => {
                app.mode = Mode::Note(String::new());
            }
            KeyCode::Char('x') => {
                if let Some(a) = app.rows.get(app.selected) {
                    let prior = Undo::Action(a.id, a.action.clone(), a.due.clone());
                    store.complete(a.id)?;
                    app.undo = Some(prior);
                    app.status = "Action completed · press e to set the next one".into();
                    app.scroll = 0;
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
                        app.config.timezone(),
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
            KeyCode::Char(c @ ('s' | 'S')) => {
                if let Some(a) = app.rows.get(app.selected) {
                    let i = STAGES.iter().position(|x| *x == a.stage).unwrap_or(0);
                    let next = if c == 's' { i + 1 } else { i.wrapping_sub(1) };
                    match STAGES.get(next) {
                        Some(stage) => {
                            let old = store.set_stage(a.id, stage)?;
                            app.undo = Some(Undo::Stage(a.id, old));
                            app.status = format!("Stage → {stage} · u to undo");
                        }
                        None => app.status = "No further stage in that direction".into(),
                    }
                }
            }
            KeyCode::Char('u') => match app.undo.take() {
                Some(Undo::Stage(id, stage)) => {
                    store.set_stage(id, &stage)?;
                    app.status = format!("Undone · stage back to {stage}");
                }
                Some(Undo::Action(id, action, due)) => {
                    store.reopen_action(id, &action, &due)?;
                    app.status = "Undone · action reopened".into();
                }
                Some(Undo::Archive(id, to)) => {
                    store.archive(id, to)?;
                    app.status = "Undone · archive status restored".into();
                }
                None => app.status = "Nothing to undo".into(),
            },
            KeyCode::Char('t') => {
                app.mode = Mode::Stats(crate::stats::render(&crate::stats::compute(store)?));
            }
            KeyCode::Char('L') => {
                let n = crate::stats::load_sample(store, Local::now().date_naive())?;
                app.tab = 0;
                app.status = format!("Loaded {n} sample applications");
            }
            KeyCode::Char('?') => app.mode = Mode::Help,
            _ => {}
        },
    }
    Ok(false)
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
                config: Config::default(),
                pal: Palette::new(&Config::default()),
                overdue: 0,
                due_today: 0,
                undo: None,
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
                app.mode = Mode::Form(Form::new(None, "Applied"));
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
            config: Config::default(),
            pal: Palette::new(&Config::default()),
            overdue: 0,
            due_today: 0,
            undo: None,
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
    fn text(t: &Terminal<ratatui::backend::TestBackend>) -> String {
        t.backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }
    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }
    #[test]
    fn today_groups_header_counts_stage_keys_and_undo() -> Result<()> {
        use ratatui::backend::TestBackend;
        let mut store = Store::initialize(rusqlite::Connection::open_in_memory()?)?;
        let today = Local::now().date_naive();
        let fmt = |d: i64| {
            (today + chrono::Duration::days(d))
                .format("%Y-%m-%d")
                .to_string()
        };
        store.save(None, "Late", "R", "Applied", "Call", &fmt(-2))?;
        store.save(None, "Now", "R", "Screening", "Call", &fmt(0))?;
        store.save(None, "Soon", "R", "Offer", "Call", &fmt(3))?;
        store.save(None, "Far", "R", "Rejected", "Call", &fmt(40))?;
        for theme in ["dark", "light", "mono"] {
            let config = Config::parse(&format!("theme = \"{theme}\""))?;
            let mut app = App {
                tab: 0,
                query: String::new(),
                selected: 0,
                rows: vec![],
                history: vec![],
                scroll: 0,
                mode: Mode::Browse,
                status: String::new(),
                config: config.clone(),
                pal: Palette::new(&config),
                overdue: 0,
                due_today: 0,
                undo: None,
                editor: false,
                max_scroll: Cell::new(0),
            };
            app.refresh(&store)?;
            assert_eq!((app.overdue, app.due_today), (1, 1));
            let mut t = Terminal::new(TestBackend::new(110, 34))?;
            t.draw(|f| draw(f, &app, &store))?;
            let out = text(&t);
            for want in [
                "1 overdue",
                "1 due today",
                "Overdue",
                "This week",
                "Later",
                "[Applied]",
            ] {
                assert!(out.contains(want), "{theme}: missing {want}");
            }
            if theme == "mono" {
                // No colour anywhere: only modifiers.
                assert!(
                    t.backend()
                        .buffer()
                        .content
                        .iter()
                        .all(|c| c.fg == Color::Reset && c.bg == Color::Reset
                            || c.modifier.contains(Modifier::REVERSED))
                );
            }
        }
        // s moves the selected application forward, u puts it back.
        let config = Config::default();
        let mut app = App {
            tab: 0,
            query: String::new(),
            selected: 0,
            rows: vec![],
            history: vec![],
            scroll: 0,
            mode: Mode::Browse,
            status: String::new(),
            config: config.clone(),
            pal: Palette::new(&config),
            overdue: 0,
            due_today: 0,
            undo: None,
            editor: false,
            max_scroll: Cell::new(0),
        };
        app.refresh(&store)?;
        let id = app.rows[0].id;
        handle(&mut app, &mut store, key('s'))?;
        assert_eq!(
            store
                .list(false)?
                .iter()
                .find(|a| a.id == id)
                .unwrap()
                .stage,
            "Screening"
        );
        handle(&mut app, &mut store, key('u'))?;
        assert_eq!(
            store
                .list(false)?
                .iter()
                .find(|a| a.id == id)
                .unwrap()
                .stage,
            "Applied"
        );
        handle(&mut app, &mut store, key('x'))?;
        assert!(
            store
                .list(false)?
                .iter()
                .find(|a| a.id == id)
                .unwrap()
                .action
                .is_empty()
        );
        handle(&mut app, &mut store, key('u'))?;
        assert_eq!(
            store
                .list(false)?
                .iter()
                .find(|a| a.id == id)
                .unwrap()
                .action,
            "Call"
        );
        handle(&mut app, &mut store, key('u'))?;
        assert_eq!(app.status, "Nothing to undo");
        Ok(())
    }
    #[test]
    fn stats_popup_and_sample_key_render() -> Result<()> {
        use ratatui::backend::TestBackend;
        let mut store = Store::initialize(rusqlite::Connection::open_in_memory()?)?;
        let config = Config::default();
        let mut app = App {
            tab: 0,
            query: String::new(),
            selected: 0,
            rows: vec![],
            history: vec![],
            scroll: 0,
            mode: Mode::Browse,
            status: String::new(),
            config: config.clone(),
            pal: Palette::new(&config),
            overdue: 0,
            due_today: 0,
            undo: None,
            editor: false,
            max_scroll: Cell::new(0),
        };
        app.refresh(&store)?;
        let mut t = Terminal::new(TestBackend::new(100, 30))?;
        t.draw(|f| draw(f, &app, &store))?;
        assert!(text(&t).contains("Load sample data"));
        handle(&mut app, &mut store, key('L'))?;
        app.refresh(&store)?;
        assert_eq!(app.rows.len(), 3);
        handle(&mut app, &mut store, key('t'))?;
        t.draw(|f| draw(f, &app, &store))?;
        let out = text(&t);
        assert!(out.contains("Pipeline statistics") && out.contains("Applications: 4"));
        // The sample loader refuses a database that already has data.
        app.mode = Mode::Browse;
        assert!(handle(&mut app, &mut store, key('L')).is_err());
        Ok(())
    }
}
