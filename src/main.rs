mod config;
mod dates;
mod extras;
mod ical;
mod portable;
mod stats;
mod store;
mod ui;
use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use store::Store;

#[derive(Parser)]
#[command(version, about = "Jobi — your next career move, one action at a time")]
struct Cli {
    /// Override the local database location (also useful for demos)
    #[arg(long, global = true)]
    db: Option<PathBuf>,
    /// Machine-readable JSON output for list, today, history, interviews, documents, status and stats
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Subcommand)]
enum TaskCmd {
    /// Add a task to an application's checklist
    Add {
        id: i64,
        title: String,
        /// YYYY-MM-DD, today, tomorrow, +3d, +2w or a weekday
        #[arg(long, default_value = "")]
        due: String,
    },
    /// List an application's tasks, with task ids
    List { id: i64 },
    /// Mark a task done
    Done { task_id: i64 },
    /// Mark a task not done
    Reopen { task_id: i64 },
    /// Delete a task
    Rm { task_id: i64 },
}
#[derive(Subcommand)]
enum ContactCmd {
    /// Add a recruiter, referrer or interviewer to an application
    Add {
        id: i64,
        name: String,
        #[arg(long, default_value = "")]
        role: String,
        #[arg(long, default_value = "")]
        email: String,
        #[arg(long, default_value = "")]
        phone: String,
        #[arg(long, default_value = "")]
        notes: String,
        /// Date you last spoke (today, YYYY-MM-DD, ...)
        #[arg(long, default_value = "")]
        last_contacted: String,
    },
    /// List an application's contacts, with contact ids
    List { id: i64 },
    /// Delete a contact
    Rm { contact_id: i64 },
}
#[derive(Subcommand)]
enum OfferCmd {
    /// Record or replace an application's offer
    Set {
        id: i64,
        /// Yearly base pay in whole currency units, e.g. 120000
        #[arg(long)]
        base: i64,
        /// 3-letter currency code, e.g. USD, EUR, INR
        #[arg(long)]
        currency: String,
        #[arg(long, default_value_t = 0)]
        bonus: i64,
        #[arg(long, default_value = "")]
        equity: String,
        /// Date the offer expires
        #[arg(long, default_value = "")]
        deadline: String,
        #[arg(long, default_value = "")]
        notes: String,
    },
    /// Show an application's offer
    Show { id: i64 },
    /// Delete an application's offer
    Rm { id: i64 },
}
#[derive(Subcommand)]
enum Command {
    /// Manage an application's task checklist
    Task {
        #[command(subcommand)]
        cmd: TaskCmd,
    },
    /// Manage an application's contacts
    Contact {
        #[command(subcommand)]
        cmd: ContactCmd,
    },
    /// Record and show an application's offer
    Offer {
        #[command(subcommand)]
        cmd: OfferCmd,
    },
    /// Compare all recorded offers side by side
    Offers,
    /// Show applications with open actions, earliest due first
    Today,
    /// One-line summary of overdue and due-today actions (for shell prompts)
    Status {
        /// Print nothing when no action is overdue or due today
        #[arg(long)]
        quiet: bool,
    },
    /// Pipeline statistics: stage counts, progress and applications added per week
    Stats,
    /// Load example applications into an empty database
    Sample,
    /// Print a shell completion script (bash, zsh, fish, elvish, powershell)
    Completions { shell: clap_complete::Shell },
    /// Print the man page (roff) to stdout
    Man,
    /// Send a desktop notification when actions are overdue or due today (cron-friendly)
    Notify,
    /// List applications
    List {
        #[arg(long)]
        archived: bool,
    },
    /// Add an application (omit company/role to open the TUI form)
    Add {
        #[arg(long, requires = "role")]
        company: Option<String>,
        #[arg(long, requires = "company")]
        role: Option<String>,
        /// Defaults to `default_stage` from config.toml (Applied)
        #[arg(long)]
        stage: Option<String>,
        #[arg(long, default_value = "")]
        action: String,
        /// YYYY-MM-DD, today, tomorrow, +3d, +2w or a weekday like fri
        #[arg(long, default_value = "")]
        due: String,
    },
    /// Show an application's activity history
    History { id: i64 },
    /// Mark the current next action complete
    Done { id: i64 },
    /// Append a dated note
    Note { id: i64, text: String },
    /// Archive an application; use --restore to bring it back
    Archive {
        id: i64,
        #[arg(long)]
        restore: bool,
    },
    /// Save or update a job posting for an application
    Posting {
        id: i64,
        #[arg(long, default_value = "")]
        url: String,
        #[arg(long, default_value = "")]
        description: String,
    },
    /// Schedule an interview; datetime must include UTC offset and timezone is IANA
    Interview {
        id: i64,
        #[arg(long)]
        at: String,
        /// IANA timezone; defaults to config.toml or the system timezone
        #[arg(long)]
        timezone: Option<String>,
        #[arg(long, default_value = "")]
        kind: String,
        #[arg(long, default_value = "")]
        url: String,
        #[arg(long, default_value = "")]
        notes: String,
    },
    /// List an application's interviews with their ids
    Interviews { id: i64 },
    /// Edit an interview by its id (see `jobi interviews`); only given fields change
    InterviewEdit {
        id: i64,
        #[arg(long)]
        at: Option<String>,
        #[arg(long)]
        timezone: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        notes: Option<String>,
    },
    /// Remove an interview by its id
    InterviewRemove { id: i64 },
    /// Unlink a document by its id (see `jobi documents`); the file is not deleted
    Unlink { id: i64 },
    /// Link an existing document file to an application
    Attach {
        id: i64,
        path: PathBuf,
        #[arg(long)]
        label: Option<String>,
        /// Keep a private copy in Jobi's storage instead of linking your file
        #[arg(long)]
        copy: bool,
    },
    /// List documents linked to an application
    Documents { id: i64 },
    /// Export all applications (with postings) to CSV
    Export {
        path: PathBuf,
        /// Overwrite the file if it exists
        #[arg(long)]
        force: bool,
    },
    /// Validate a CSV and show a preview; add --apply to import it (all or nothing)
    Import {
        path: PathBuf,
        #[arg(long)]
        apply: bool,
        /// Import rows even when the company and role are already tracked
        #[arg(long)]
        allow_duplicates: bool,
    },
    /// Write a complete JSON backup (applications, history, postings, interviews, document links)
    Backup {
        path: PathBuf,
        /// Overwrite the file if it exists
        #[arg(long)]
        force: bool,
    },
    /// Restore a JSON backup into an empty database (or add it to existing data with --merge)
    Restore {
        path: PathBuf,
        /// Add to the current database, skipping applications whose company and role already exist
        #[arg(long)]
        merge: bool,
    },
    /// Export interviews (and with --actions, next-action due dates) to an .ics calendar file
    Ical {
        path: PathBuf,
        /// Also add each open next action as an all-day event
        #[arg(long)]
        actions: bool,
        /// Overwrite the file if it exists
        #[arg(long)]
        force: bool,
    },
    /// Print the database location
    Path,
}
fn default_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("JOBI_DB") {
        return Ok(path.into());
    }
    Ok(config::data_dir()?.join("jobi.sqlite3"))
}
fn today() -> chrono::NaiveDate {
    chrono::Local::now().date_naive()
}
fn main() -> Result<()> {
    let cli = Cli::parse();
    let path = match cli.db {
        Some(p) => p,
        None => default_path()?,
    };
    match &cli.command {
        Some(Command::Completions { shell }) => {
            clap_complete::generate(
                *shell,
                &mut <Cli as clap::CommandFactory>::command(),
                "jobi",
                &mut std::io::stdout(),
            );
            return Ok(());
        }
        Some(Command::Man) => {
            clap_mangen::Man::new(<Cli as clap::CommandFactory>::command())
                .render(&mut std::io::stdout())?;
            return Ok(());
        }
        _ => {}
    }
    if matches!(cli.command, Some(Command::Path)) {
        println!("{}", path.display());
        return Ok(());
    }
    let config = config::Config::load()?;
    let mut store = Store::open(&path)?;
    match cli.command {
        None => ui::run(&mut store, false, &config)?,
        Some(Command::Add {
            company,
            role,
            stage,
            action,
            due,
        }) => match (company, role) {
            (Some(c), Some(r)) => {
                let due = dates::resolve_due(&due, today())?;
                let stage = stage.unwrap_or_else(|| config.default_stage.clone());
                let (action, due) = config.with_follow_up(&stage, &action, &due, today());
                let id = store.save(None, &c, &r, &stage, &action, &due)?;
                println!("Added application #{id}");
                if !action.is_empty() && !due.is_empty() {
                    println!("Next action: {action} (due {due})");
                }
            }
            _ => ui::run(&mut store, true, &config)?,
        },
        Some(Command::Status { quiet }) => {
            let rows = store.list(false)?;
            let stale = store.stale_ids(config.stale_days)?.len();
            let (line, urgent) = due_summary(&rows, stale);
            if cli.json {
                let (overdue, due_today) =
                    store::due_counts(&rows, &today().format("%Y-%m-%d").to_string());
                let open = rows.iter().filter(|a| !a.action.is_empty()).count();
                println!(
                    "{}",
                    serde_json::json!({"overdue": overdue, "due_today": due_today, "open": open, "stale": stale})
                );
            } else if urgent > 0 || !quiet {
                println!("{line}");
            }
        }
        Some(Command::Task { cmd }) => match cmd {
            TaskCmd::Add { id, title, due } => {
                store.add_task(id, &title, &dates::resolve_due(&due, today())?)?;
                println!("Task added to #{id}");
            }
            TaskCmd::List { id } => {
                let tasks = store.tasks(id)?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&tasks)?);
                }
                for t in tasks.iter().filter(|_| !cli.json) {
                    let mark = if t.done { "[x]" } else { "[ ]" };
                    println!("{}\t{mark}\t{}\t{}", t.id, t.due, t.title);
                }
            }
            TaskCmd::Done { task_id } => {
                store.set_task_done(task_id, true)?;
                println!("Task #{task_id} done");
            }
            TaskCmd::Reopen { task_id } => {
                store.set_task_done(task_id, false)?;
                println!("Task #{task_id} reopened");
            }
            TaskCmd::Rm { task_id } => {
                store.delete_task(task_id)?;
                println!("Task #{task_id} deleted");
            }
        },
        Some(Command::Contact { cmd }) => match cmd {
            ContactCmd::Add {
                id,
                name,
                role,
                email,
                phone,
                notes,
                last_contacted,
            } => {
                let c = extras::Contact {
                    id: 0,
                    name,
                    role,
                    email,
                    phone,
                    notes,
                    last_contacted: dates::resolve_due(&last_contacted, today())?,
                };
                store.add_contact(id, &c)?;
                println!("Contact added to #{id}");
            }
            ContactCmd::List { id } => {
                let cs = store.contacts(id)?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&cs)?);
                }
                for c in cs.iter().filter(|_| !cli.json) {
                    println!(
                        "{}\t{}\t{}\t{}\t{}\t{}",
                        c.id, c.name, c.role, c.email, c.phone, c.last_contacted
                    );
                }
            }
            ContactCmd::Rm { contact_id } => {
                store.delete_contact(contact_id)?;
                println!("Contact #{contact_id} deleted");
            }
        },
        Some(Command::Offer { cmd }) => match cmd {
            OfferCmd::Set {
                id,
                base,
                currency,
                bonus,
                equity,
                deadline,
                notes,
            } => {
                store.save_offer(&extras::Offer {
                    application_id: id,
                    base,
                    currency,
                    bonus,
                    equity,
                    deadline: dates::resolve_due(&deadline, today())?,
                    notes,
                })?;
                println!("Offer saved for #{id}");
            }
            OfferCmd::Show { id } => match store.offer(id)? {
                Some(o) if cli.json => println!("{}", serde_json::to_string_pretty(&o)?),
                Some(o) => {
                    println!("{}", o.summary());
                    if !o.notes.is_empty() {
                        println!("{}", o.notes);
                    }
                }
                None => bail!("Application #{id} has no offer"),
            },
            OfferCmd::Rm { id } => {
                store.delete_offer(id)?;
                println!("Offer removed from #{id}");
            }
        },
        Some(Command::Offers) => {
            let all = store.offers()?;
            if cli.json {
                let v: Vec<_> = all
                    .iter()
                    .map(|(a, o)| serde_json::json!({"application": a, "offer": o}))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else if all.is_empty() {
                println!("No offers recorded. Use `jobi offer set ID --base N --currency USD`.");
            } else {
                println!("ID\tCOMPANY\tROLE\tOFFER");
                for (a, o) in &all {
                    println!("{}\t{}\t{}\t{}", a.id, a.company, a.role, o.summary());
                }
                println!("(sorted by currency, then base pay; no currency conversion)");
            }
        }
        Some(Command::Stats) => {
            let st = stats::compute(&store)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&st)?);
            } else {
                print!("{}", stats::render(&st));
            }
        }
        Some(Command::Sample) => {
            println!(
                "Loaded {} sample applications",
                stats::load_sample(&mut store, today())?
            );
        }
        Some(Command::Completions { .. } | Command::Man) => unreachable!(),
        Some(Command::Notify) => {
            let (line, urgent) = due_summary(&store.list(false)?, 0);
            if urgent > 0 {
                notify("Jobi", &line);
            }
        }
        Some(Command::Today) => {
            let mut rows = store.list(false)?;
            ui::order_actions(&mut rows);
            print_rows(&rows, cli.json)?;
        }
        Some(Command::List { archived }) => print_rows(&store.list(archived)?, cli.json)?,
        Some(Command::History { id }) => {
            let lines = store.history(id)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&lines)?);
            } else {
                for line in lines {
                    println!("{line}");
                }
            }
        }
        Some(Command::Done { id }) => {
            store.complete(id)?;
            println!("Completed action for #{id}");
        }
        Some(Command::Note { id, text }) => {
            store.note(id, &text)?;
            println!("Note saved");
        }
        Some(Command::Archive { id, restore }) => {
            store.archive(id, !restore)?;
            println!("{} #{id}", if restore { "Restored" } else { "Archived" });
        }
        Some(Command::Posting {
            id,
            url,
            description,
        }) => {
            store.save_posting(id, &url, &description)?;
            println!("Posting saved for #{id}");
        }
        Some(Command::Interview {
            id,
            at,
            timezone,
            kind,
            url,
            notes,
        }) => {
            let timezone = timezone.unwrap_or_else(|| config.timezone());
            store.add_interview(id, &at, &timezone, &kind, &url, &notes)?;
            println!("Interview scheduled for #{id}");
        }
        Some(Command::Attach {
            id,
            path,
            label,
            copy,
        }) => {
            let label = label.unwrap_or_else(|| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
            if copy {
                store.add_document_copy(id, &label, &path.to_string_lossy())?;
                println!("Document copied into Jobi's storage for #{id}");
            } else {
                store.add_document(id, &label, &path.to_string_lossy())?;
                println!("Document linked to #{id}");
            }
        }
        Some(Command::Documents { id }) => {
            let docs = store.documents(id)?;
            if cli.json {
                let v: Vec<_> = docs
                    .iter()
                    .map(|d| serde_json::json!({"id": d.id, "label": d.label, "path": d.path, "missing": d.missing(), "managed": d.managed}))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&v)?);
            }
            for d in docs.iter().filter(|_| !cli.json) {
                let note = if d.missing() { "\t(file missing)" } else { "" };
                println!("{}\t{}\t{}{note}", d.id, d.label, d.path);
            }
        }
        Some(Command::Interviews { id }) => {
            let ivs = store.interviews(id)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&ivs)?);
            }
            for i in ivs.iter().filter(|_| !cli.json) {
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}",
                    i.id,
                    i.starts_at,
                    i.timezone,
                    i.kind,
                    i.url,
                    i.notes.replace('\n', " ")
                );
            }
        }
        Some(Command::InterviewEdit {
            id,
            at,
            timezone,
            kind,
            url,
            notes,
        }) => {
            let mut i = store.interview(id)?;
            i.starts_at = at.unwrap_or(i.starts_at);
            i.timezone = timezone.unwrap_or(i.timezone);
            i.kind = kind.unwrap_or(i.kind);
            i.url = url.unwrap_or(i.url);
            i.notes = notes.unwrap_or(i.notes);
            store.update_interview(&i)?;
            println!("Interview #{id} updated");
        }
        Some(Command::InterviewRemove { id }) => {
            store.delete_interview(id)?;
            println!("Interview #{id} removed");
        }
        Some(Command::Unlink { id }) => {
            store.delete_document(id)?;
            println!("Document link #{id} removed (file not deleted)");
        }
        Some(Command::Export { path, force }) => {
            let n = portable::export_csv(&store, &path, force)?;
            println!("Exported {n} application(s) to {}", path.display());
        }
        Some(Command::Import {
            path,
            apply,
            allow_duplicates,
        }) => {
            let (rows, errors) = portable::parse_csv(&path)?;
            for r in rows.iter().take(10) {
                println!("line {}: {} · {} [{}]", r.line, r.company, r.role, r.stage);
            }
            if rows.len() > 10 {
                println!("... and {} more", rows.len() - 10);
            }
            for e in &errors {
                eprintln!("error: {e}");
            }
            if !errors.is_empty() {
                bail!(
                    "{} invalid row(s); nothing was imported. Fix the file and retry.",
                    errors.len()
                );
            }
            let (rows, dups) = if allow_duplicates {
                (rows, vec![])
            } else {
                portable::split_duplicates(&store, rows)?
            };
            for (line, why) in &dups {
                println!("line {line}: skipped duplicate ({why})");
            }
            if apply {
                portable::import_rows(&mut store, &rows)?;
                println!(
                    "Imported {} application(s); skipped {} duplicate(s)",
                    rows.len(),
                    dups.len()
                );
            } else {
                println!(
                    "{} new row(s), {} duplicate(s) would be skipped. Preview only; re-run with --apply to import.",
                    rows.len(),
                    dups.len()
                );
            }
        }
        Some(Command::Backup { path, force }) => {
            let (n, missing) = portable::backup(&store, &path, force)?;
            println!("Backed up {n} application(s) to {}", path.display());
            report_missing(&missing);
        }
        Some(Command::Restore { path, merge }) => {
            let r = portable::restore(&mut store, &path, merge)?;
            println!("Restored {} application(s)", r.restored);
            for name in &r.skipped {
                println!("skipped (already tracked): {name}");
            }
            report_missing(&r.missing_documents);
        }
        Some(Command::Ical {
            path,
            actions,
            force,
        }) => {
            let n = ical::export(&store, &path, actions, force)?;
            println!("Exported {n} calendar event(s) to {}", path.display());
        }
        Some(Command::Path) => unreachable!(),
    }
    Ok(())
}
fn report_missing(missing: &[String]) {
    if !missing.is_empty() {
        eprintln!(
            "warning: {} linked document(s) not found on this computer:",
            missing.len()
        );
        for m in missing {
            eprintln!("  {m}");
        }
    }
}
/// ("2 overdue · 1 due today · 5 open", overdue + due today)
fn due_summary(rows: &[store::Application], stale: usize) -> (String, usize) {
    let (overdue, due_today) = store::due_counts(rows, &today().format("%Y-%m-%d").to_string());
    let open = rows.iter().filter(|a| !a.action.is_empty()).count();
    let mut parts = vec![];
    if overdue > 0 {
        parts.push(format!("{overdue} overdue"));
    }
    if due_today > 0 {
        parts.push(format!("{due_today} due today"));
    }
    if parts.is_empty() {
        parts.push("nothing due today".into());
    }
    parts.push(format!("{open} open"));
    if stale > 0 {
        parts.push(format!("{stale} stale"));
    }
    (parts.join(" · "), overdue + due_today)
}
/// Desktop notification via the platform tool; falls back to printing.
fn notify(title: &str, body: &str) {
    use std::process::Command as Proc;
    #[cfg(target_os = "macos")]
    let sent = Proc::new("osascript")
        .args([
            "-e",
            "on run argv",
            "-e",
            "display notification (item 2 of argv) with title (item 1 of argv)",
            "-e",
            "end run",
            title,
            body,
        ])
        .status();
    #[cfg(not(target_os = "macos"))]
    let sent = Proc::new("notify-send").args([title, body]).status();
    if !matches!(sent, Ok(s) if s.success()) {
        println!("{title}: {body}");
    }
}
fn print_rows(rows: &[store::Application], json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(rows)?);
        return Ok(());
    }
    if rows.is_empty() {
        println!("Nothing here yet. Run `jobi add` to get started.");
        return Ok(());
    }
    println!("ID\tCOMPANY\tROLE\tSTAGE\tDUE\tNEXT ACTION");
    for a in rows {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            a.id, a.company, a.role, a.stage, a.due, a.action
        );
    }
    Ok(())
}
