mod config;
mod dates;
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
enum Command {
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
    /// Restore a JSON backup into an empty database
    Restore { path: PathBuf },
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
                println!(
                    "Added application #{}",
                    store.save(None, &c, &r, &stage, &action, &due)?
                );
            }
            _ => ui::run(&mut store, true, &config)?,
        },
        Some(Command::Status { quiet }) => {
            let rows = store.list(false)?;
            let (line, urgent) = due_summary(&rows);
            if cli.json {
                let (overdue, due_today) =
                    store::due_counts(&rows, &today().format("%Y-%m-%d").to_string());
                let open = rows.iter().filter(|a| !a.action.is_empty()).count();
                println!(
                    "{}",
                    serde_json::json!({"overdue": overdue, "due_today": due_today, "open": open})
                );
            } else if urgent > 0 || !quiet {
                println!("{line}");
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
            let (line, urgent) = due_summary(&store.list(false)?);
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
        Some(Command::Attach { id, path, label }) => {
            let label = label.unwrap_or_else(|| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
            store.add_document(id, &label, &path.to_string_lossy())?;
            println!("Document linked to #{id}");
        }
        Some(Command::Documents { id }) => {
            let docs = store.documents(id)?;
            if cli.json {
                let v: Vec<_> = docs
                    .iter()
                    .map(|d| serde_json::json!({"id": d.id, "label": d.label, "path": d.path, "missing": d.missing()}))
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
        Some(Command::Restore { path }) => {
            let (n, missing) = portable::restore(&mut store, &path)?;
            println!("Restored {n} application(s)");
            report_missing(&missing);
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
fn due_summary(rows: &[store::Application]) -> (String, usize) {
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
