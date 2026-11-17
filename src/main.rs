mod config;
mod dates;
mod portable;
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
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Subcommand)]
enum Command {
    /// Show applications with open actions, earliest due first
    Today,
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
        Some(Command::Today) => {
            let mut rows = store.list(false)?;
            ui::order_actions(&mut rows);
            print_rows(&rows);
        }
        Some(Command::List { archived }) => print_rows(&store.list(archived)?),
        Some(Command::History { id }) => {
            for line in store.history(id)? {
                println!("{line}");
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
            for d in store.documents(id)? {
                let note = if d.missing() { "\t(file missing)" } else { "" };
                println!("{}\t{}\t{}{note}", d.id, d.label, d.path);
            }
        }
        Some(Command::Interviews { id }) => {
            for i in store.interviews(id)? {
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
        Some(Command::Import { path, apply }) => {
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
            if apply {
                portable::import_rows(&mut store, &rows)?;
                println!("Imported {} application(s)", rows.len());
            } else {
                println!(
                    "{} row(s) valid. Preview only; re-run with --apply to import.",
                    rows.len()
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
fn print_rows(rows: &[store::Application]) {
    if rows.is_empty() {
        println!("Nothing here yet. Run `jobi add` to get started.");
        return;
    }
    println!("ID\tCOMPANY\tROLE\tSTAGE\tDUE\tNEXT ACTION");
    for a in rows {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            a.id, a.company, a.role, a.stage, a.due, a.action
        );
    }
}
