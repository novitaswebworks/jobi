use crate::config::Config;
use crate::dates::resolve_due;
use crate::extras::{Contact, Offer, Task};
use crate::store::{Application, Document, Interview, STAGES, Store, due_counts};
use anyhow::{Context, Result, bail};
use chrono::Local;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
mod draw;
mod input;
#[cfg(test)]
mod tests;
mod theme;
use draw::draw;
use input::{handle, handle_mouse};
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
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
    Help(u16),
    Archive(i64, bool),
    /// kind (see `spec`), field values, focused field, error, record id
    Prepare(usize, Vec<String>, usize, String, Option<i64>),
    /// application row, selected interview/document
    Inspect(usize, usize),
    Remove(RemoveKind, i64, String),
    /// rendered pipeline statistics
    Stats(String),
}
#[derive(Clone, Copy, PartialEq, Debug)]
enum RemoveKind {
    Interview,
    Document,
    Task,
    Contact,
}
/// One selectable row of the details view.
enum Item {
    Interview(Interview),
    Document(Document),
    Task(Task),
    Contact(Contact),
}
fn inspect_items(store: &Store, id: i64) -> Result<Vec<Item>> {
    let mut v: Vec<Item> = store
        .interviews(id)?
        .into_iter()
        .map(Item::Interview)
        .collect();
    v.extend(store.documents(id)?.into_iter().map(Item::Document));
    v.extend(store.tasks(id)?.into_iter().map(Item::Task));
    v.extend(store.contacts(id)?.into_iter().map(Item::Contact));
    Ok(v)
}
/// Title and field labels of each `Mode::Prepare` form.
fn spec(kind: usize) -> (&'static str, &'static [&'static str]) {
    const NOTES: &str = "Notes (Ctrl+E opens your editor)";
    match kind {
        0 => (
            "Job posting",
            &["URL", "Description (Ctrl+E opens your editor)"],
        ),
        1 | 3 => (
            if kind == 3 {
                "Edit interview"
            } else {
                "Schedule interview"
            },
            &[
                "Starts at RFC3339 e.g. 2026-11-20T14:00:00+05:30",
                "Timezone",
                "Interview type",
                "Meeting URL",
                NOTES,
            ],
        ),
        2 => ("Link document", &["Label", "Existing file path"]),
        4 => (
            "Offer",
            &[
                "Base pay per year, whole units (e.g. 120000)",
                "Currency (USD, EUR, INR...)",
                "Bonus per year (optional)",
                "Equity (optional)",
                "Decide by (optional: 2026-12-01, +7d, fri)",
                NOTES,
            ],
        ),
        5 => (
            "Add contact",
            &[
                "Name",
                "Role (recruiter, interviewer...)",
                "Email",
                "Phone",
                "Last contacted (optional: today, 2026-11-01)",
                NOTES,
            ],
        ),
        _ => ("Add task", &["Title", "Due (optional: tomorrow, +3d, fri)"]),
    }
}
/// Fields that hold long text and can be edited in $VISUAL/$EDITOR.
fn multiline(kind: usize, field: usize) -> bool {
    matches!((kind, field), (0, 1) | (1 | 3, 4) | (4 | 5, 5))
}
/// Search syntax: free words must all appear in company, role, stage or action;
/// `stage:interview` matches a stage prefix; `due:overdue|today|week|none` filters by date.
fn matches_query(a: &Application, query: &str, today: &str, week: &str) -> bool {
    let hay = format!("{} {} {} {}", a.company, a.role, a.stage, a.action).to_lowercase();
    query.to_lowercase().split_whitespace().all(|t| {
        if let Some(s) = t.strip_prefix("stage:") {
            a.stage.to_lowercase().starts_with(s)
        } else if let Some(d) = t.strip_prefix("due:") {
            match d {
                "overdue" => !a.due.is_empty() && a.due.as_str() < today,
                "today" => a.due == today,
                "week" => !a.due.is_empty() && a.due.as_str() >= today && a.due.as_str() <= week,
                "none" => a.due.is_empty(),
                _ => false,
            }
        } else {
            hay.contains(t)
        }
    })
}
/// Where the last draw put things, so mouse clicks can be mapped back to rows.
#[derive(Default)]
struct Hit {
    list: Rect,
    details: Rect,
    offset: usize,
    /// per list item: (application row if selectable, height in lines)
    items: Vec<(Option<usize>, u16)>,
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
    hit: RefCell<Hit>,
    /// extras for the selected application
    tasks: Vec<Task>,
    contacts: Vec<Contact>,
    offer: Option<Offer>,
    /// active applications with no recent activity
    stale: HashSet<i64>,
}
impl App {
    fn new(config: &Config, mode: Mode) -> Self {
        Self {
            tab: 0,
            query: String::new(),
            selected: 0,
            rows: vec![],
            history: vec![],
            scroll: 0,
            mode,
            status: String::new(),
            config: config.clone(),
            pal: Palette::new(config),
            overdue: 0,
            due_today: 0,
            undo: None,
            editor: false,
            max_scroll: Cell::new(0),
            hit: RefCell::default(),
            tasks: vec![],
            contacts: vec![],
            offer: None,
            stale: HashSet::new(),
        }
    }
    fn refresh(&mut self, store: &Store) -> Result<()> {
        self.rows = store.list(self.tab == 2)?;
        if self.tab == 0 {
            order_actions(&mut self.rows);
        }
        let now = Local::now().date_naive();
        let today = now.format("%Y-%m-%d").to_string();
        let week = (now + chrono::Duration::days(7))
            .format("%Y-%m-%d")
            .to_string();
        self.rows
            .retain(|a| matches_query(a, &self.query, &today, &week));
        self.stale = store.stale_ids(self.config.stale_days)?;
        (self.overdue, self.due_today) = due_counts(&store.list(false)?, &today);
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
        let id = self.rows.get(self.selected).map(|a| a.id);
        self.history = id.map_or(Ok(vec![]), |id| store.history(id))?;
        self.tasks = id.map_or(Ok(vec![]), |id| store.tasks(id))?;
        self.contacts = id.map_or(Ok(vec![]), |id| store.contacts(id))?;
        self.offer = id.map_or(Ok(None), |id| store.offer(id))?;
        Ok(())
    }
}
pub fn run(store: &mut Store, add: bool, config: &Config) -> Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!(
            "The interactive app requires a terminal. Use `jobi list`, `jobi today`, or `jobi add --company NAME --role ROLE`."
        );
    }
    let mut app = App::new(
        config,
        if add {
            Mode::Form(Form::new(None, &config.default_stage))
        } else {
            Mode::Browse
        },
    );
    app.status = "Local storage · no account required".into();
    app.refresh(store)?;
    // SIGTERM/SIGHUP must leave the loop normally so the terminal is restored.
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    #[cfg(unix)]
    for sig in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGHUP] {
        signal_hook::flag::register(sig, stop.clone())?;
    }
    let mut terminal = ratatui::init();
    // Mouse support is best effort: ignore terminals that refuse it.
    let _ = crossterm::execute!(std::io::stdout(), EnableMouseCapture);
    let result = (|| -> Result<()> {
        loop {
            terminal.draw(|f| draw(f, &app, store))?;
            if stop.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            if !event::poll(Duration::from_millis(250))? {
                continue;
            }
            match event::read()? {
                Event::Mouse(m) => {
                    handle_mouse(&mut app, m);
                    app.refresh(store)?;
                }
                Event::Key(key) => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        break;
                    }
                    match handle(&mut app, store, key) {
                        Ok(true) => break,
                        Ok(false) => {}
                        Err(e) => app.status = format!("Error: {e}"),
                    }
                    if std::mem::take(&mut app.editor) {
                        let _ = crossterm::execute!(std::io::stdout(), DisableMouseCapture);
                        ratatui::restore();
                        match &mut app.mode {
                            Mode::Prepare(_, fields, field, error, _) => {
                                match edit_externally(&fields[*field]) {
                                    Ok(t) => fields[*field] = t,
                                    Err(e) => *error = e.to_string(),
                                }
                            }
                            Mode::Note(text) => match edit_externally(text) {
                                Ok(t) => *text = t,
                                Err(e) => app.status = format!("Error: {e}"),
                            },
                            _ => {}
                        }
                        terminal = ratatui::init();
                        let _ = crossterm::execute!(std::io::stdout(), EnableMouseCapture);
                    }
                    app.refresh(store)?;
                }
                _ => {}
            }
        }
        Ok(())
    })();
    let _ = crossterm::execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}
