//! CSV import/export and complete JSON backup/restore.
use crate::extras::Offer;
use crate::store::{Store, validate_app, validate_interview};
use anyhow::{Context, Result, bail};
use base64::Engine;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::Path};

const CSV_COLUMNS: [&str; 9] = [
    "company",
    "role",
    "stage",
    "action",
    "due",
    "archived",
    "posting_url",
    "posting_description",
    "id",
];
const BACKUP_FORMAT: &str = "jobi-backup";
/// Version 2 added tasks, contacts and offers; version 1 files are still readable.
const BACKUP_VERSION: u32 = 2;

/// Writes `data` to a new file; refuses to replace an existing one without `force`.
pub(crate) fn write_new(path: &Path, data: &[u8], force: bool) -> Result<()> {
    let mut opts = fs::OpenOptions::new();
    opts.write(true);
    if force {
        opts.create(true).truncate(true);
    } else {
        opts.create_new(true);
    }
    let mut f = opts
        .open(path)
        .with_context(|| format!("Cannot write {} (use --force to overwrite)", path.display()))?;
    f.write_all(data)?;
    Ok(())
}

// ---------- CSV ----------

#[derive(Debug, PartialEq)]
pub struct ImportRow {
    pub line: u64,
    pub company: String,
    pub role: String,
    pub stage: String,
    pub action: String,
    pub due: String,
    pub archived: bool,
    pub url: String,
    pub description: String,
}

pub fn export_csv(store: &Store, path: &Path, force: bool) -> Result<usize> {
    let mut w = csv::Writer::from_writer(vec![]);
    w.write_record(CSV_COLUMNS)?;
    let mut q = store.0.prepare(
        "SELECT a.company,a.role,a.stage,a.action,a.due,a.archived,
                COALESCE(p.url,''),COALESCE(p.description,''),a.id
         FROM applications a LEFT JOIN postings p ON p.application_id=a.id ORDER BY a.id",
    )?;
    let rows = q.query_map([], |r| {
        Ok([
            r.get::<_, String>(0)?,
            r.get(1)?,
            r.get(2)?,
            r.get(3)?,
            r.get(4)?,
            r.get::<_, bool>(5)?.to_string(),
            r.get(6)?,
            r.get(7)?,
            r.get::<_, i64>(8)?.to_string(),
        ])
    })?;
    let mut n = 0;
    for row in rows {
        w.write_record(row?)?;
        n += 1;
    }
    write_new(path, &w.into_inner()?, force)?;
    Ok(n)
}

fn parse_bool(v: &str) -> Result<bool> {
    match v.trim().to_lowercase().as_str() {
        "" | "false" | "0" | "no" => Ok(false),
        "true" | "1" | "yes" => Ok(true),
        _ => bail!("archived must be true or false, got '{v}'"),
    }
}

/// Parses and validates the whole file without touching the database.
/// Returns valid rows and one message per problem (`line N: ...`).
pub fn parse_csv(path: &Path) -> Result<(Vec<ImportRow>, Vec<String>)> {
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .from_path(path)
        .with_context(|| format!("Cannot read {}", path.display()))?;
    let headers: Vec<String> = rdr
        .headers()?
        .iter()
        .map(|h| h.trim().trim_start_matches('\u{feff}').to_lowercase())
        .collect();
    if let Some(bad) = headers.iter().find(|h| !CSV_COLUMNS.contains(&h.as_str())) {
        bail!(
            "Unknown column '{bad}'. Supported columns: {}",
            CSV_COLUMNS.join(", ")
        );
    }
    for need in ["company", "role"] {
        if !headers.iter().any(|h| h == need) {
            bail!("Missing required column '{need}'");
        }
    }
    let (mut rows, mut errors) = (vec![], vec![]);
    for rec in rdr.records() {
        let rec = match rec {
            Ok(r) => r,
            Err(e) => {
                errors.push(format!("{e}"));
                continue;
            }
        };
        let line = rec.position().map_or(0, |p| p.line());
        if rec.len() != headers.len() {
            errors.push(format!(
                "line {line}: expected {} fields, found {}",
                headers.len(),
                rec.len()
            ));
            continue;
        }
        let get = |name: &str| {
            headers
                .iter()
                .position(|h| h == name)
                .map_or("", |i| rec[i].trim())
                .to_string()
        };
        let mut row = ImportRow {
            line,
            company: get("company"),
            role: get("role"),
            stage: get("stage"),
            action: get("action"),
            due: get("due"),
            archived: false,
            url: get("posting_url"),
            description: get("posting_description"),
        };
        if row.stage.is_empty() {
            row.stage = "Applied".into();
        }
        let checked = parse_bool(&get("archived"))
            .map(|a| row.archived = a)
            .and_then(|_| validate_app(&row.company, &row.role, &row.stage, &row.action, &row.due));
        match checked {
            Ok(()) => rows.push(row),
            Err(e) => errors.push(format!("line {line}: {e}")),
        }
    }
    Ok((rows, errors))
}

/// (line number, why the row was skipped)
pub type Skipped = (u64, String);

fn dup_key(company: &str, role: &str) -> String {
    let norm = |s: &str| {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    format!("{}\u{1f}{}", norm(company), norm(role))
}

/// Separates rows already tracked (same company and role, active or archived) or repeated
/// earlier in the file. Returns (new rows, [(line, reason)]).
pub fn split_duplicates(
    store: &Store,
    rows: Vec<ImportRow>,
) -> Result<(Vec<ImportRow>, Vec<Skipped>)> {
    let mut known: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for a in store.list(false)?.into_iter().chain(store.list(true)?) {
        known.insert(
            dup_key(&a.company, &a.role),
            format!("already tracked as #{}", a.id),
        );
    }
    let (mut fresh, mut dups) = (vec![], vec![]);
    for r in rows {
        let key = dup_key(&r.company, &r.role);
        match known.get(&key) {
            Some(why) => dups.push((r.line, why.clone())),
            None => {
                known.insert(key, format!("same as line {}", r.line));
                fresh.push(r);
            }
        }
    }
    Ok((fresh, dups))
}

/// Inserts all rows in one transaction: either every row is imported or none.
pub fn import_rows(store: &mut Store, rows: &[ImportRow]) -> Result<()> {
    let tx = store.0.transaction()?;
    for r in rows {
        tx.execute(
            "INSERT INTO applications(company,role,stage,action,due,archived) VALUES(?,?,?,?,?,?)",
            params![r.company, r.role, r.stage, r.action, r.due, r.archived],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO activity(application_id,message) VALUES(?,?)",
            params![id, format!("Imported from CSV · {}", r.stage)],
        )?;
        if !r.url.is_empty() || !r.description.is_empty() {
            tx.execute(
                "INSERT INTO postings(application_id,url,description) VALUES(?,?,?)",
                params![id, r.url, r.description],
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}

// ---------- JSON backup / restore ----------

#[derive(Serialize, Deserialize)]
struct AppRec {
    id: i64,
    company: String,
    role: String,
    stage: String,
    action: String,
    due: String,
    archived: bool,
}
#[derive(Serialize, Deserialize)]
struct ActivityRec {
    id: i64,
    application_id: i64,
    at: String,
    message: String,
}
#[derive(Serialize, Deserialize)]
struct PostingRec {
    application_id: i64,
    url: String,
    description: String,
}
#[derive(Serialize, Deserialize)]
struct InterviewRec {
    id: i64,
    application_id: i64,
    starts_at: String,
    timezone: String,
    kind: String,
    meeting_url: String,
    notes: String,
}
#[derive(Serialize, Deserialize)]
struct DocumentRec {
    id: i64,
    application_id: i64,
    label: String,
    path: String,
    managed: bool,
}
#[derive(Serialize, Deserialize)]
struct TaskRec {
    id: i64,
    application_id: i64,
    title: String,
    due: String,
    done: bool,
}
#[derive(Serialize, Deserialize)]
struct ContactRec {
    id: i64,
    application_id: i64,
    name: String,
    role: String,
    email: String,
    phone: String,
    notes: String,
    last_contacted: String,
}
/// A private document copy embedded in the backup (only with --include-documents).
#[derive(Serialize, Deserialize)]
struct FileRec {
    document_id: i64,
    name: String,
    /// standard base64
    data: String,
}
#[derive(Serialize, Deserialize)]
struct Backup {
    format: String,
    version: u32,
    exported_at: String,
    applications: Vec<AppRec>,
    activity: Vec<ActivityRec>,
    postings: Vec<PostingRec>,
    interviews: Vec<InterviewRec>,
    documents: Vec<DocumentRec>,
    #[serde(default)]
    tasks: Vec<TaskRec>,
    #[serde(default)]
    contacts: Vec<ContactRec>,
    #[serde(default)]
    offers: Vec<Offer>,
    #[serde(default)]
    files: Vec<FileRec>,
}

fn collect<T>(
    store: &Store,
    sql: &str,
    f: impl FnMut(&rusqlite::Row) -> rusqlite::Result<T>,
) -> Result<Vec<T>> {
    Ok(store
        .0
        .prepare(sql)?
        .query_map([], f)?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

fn missing_documents<'a>(docs: impl Iterator<Item = &'a DocumentRec>) -> Vec<String> {
    docs.filter(|d| !Path::new(&d.path).is_file())
        .map(|d| format!("{} ({})", d.label, d.path))
        .collect()
}

fn read_all(store: &Store) -> Result<Backup> {
    Ok(Backup {
        format: BACKUP_FORMAT.into(),
        version: BACKUP_VERSION,
        exported_at: chrono::Utc::now().to_rfc3339(),
        applications: collect(
            store,
            "SELECT id,company,role,stage,action,due,archived FROM applications ORDER BY id",
            |r| {
                Ok(AppRec {
                    id: r.get(0)?,
                    company: r.get(1)?,
                    role: r.get(2)?,
                    stage: r.get(3)?,
                    action: r.get(4)?,
                    due: r.get(5)?,
                    archived: r.get(6)?,
                })
            },
        )?,
        activity: collect(
            store,
            "SELECT id,application_id,at,message FROM activity ORDER BY id",
            |r| {
                Ok(ActivityRec {
                    id: r.get(0)?,
                    application_id: r.get(1)?,
                    at: r.get(2)?,
                    message: r.get(3)?,
                })
            },
        )?,
        postings: collect(
            store,
            "SELECT application_id,url,description FROM postings ORDER BY application_id",
            |r| {
                Ok(PostingRec {
                    application_id: r.get(0)?,
                    url: r.get(1)?,
                    description: r.get(2)?,
                })
            },
        )?,
        interviews: collect(
            store,
            "SELECT id,application_id,starts_at,timezone,kind,meeting_url,notes FROM interviews ORDER BY id",
            |r| {
                Ok(InterviewRec {
                    id: r.get(0)?,
                    application_id: r.get(1)?,
                    starts_at: r.get(2)?,
                    timezone: r.get(3)?,
                    kind: r.get(4)?,
                    meeting_url: r.get(5)?,
                    notes: r.get(6)?,
                })
            },
        )?,
        documents: collect(
            store,
            "SELECT id,application_id,label,path,managed FROM documents ORDER BY id",
            |r| {
                Ok(DocumentRec {
                    id: r.get(0)?,
                    application_id: r.get(1)?,
                    label: r.get(2)?,
                    path: r.get(3)?,
                    managed: r.get(4)?,
                })
            },
        )?,
        tasks: collect(
            store,
            "SELECT id,application_id,title,due,done FROM tasks ORDER BY id",
            |r| {
                Ok(TaskRec {
                    id: r.get(0)?,
                    application_id: r.get(1)?,
                    title: r.get(2)?,
                    due: r.get(3)?,
                    done: r.get(4)?,
                })
            },
        )?,
        contacts: collect(
            store,
            "SELECT id,application_id,name,role,email,phone,notes,last_contacted FROM contacts ORDER BY id",
            |r| {
                Ok(ContactRec {
                    id: r.get(0)?,
                    application_id: r.get(1)?,
                    name: r.get(2)?,
                    role: r.get(3)?,
                    email: r.get(4)?,
                    phone: r.get(5)?,
                    notes: r.get(6)?,
                    last_contacted: r.get(7)?,
                })
            },
        )?,
        offers: collect(
            store,
            "SELECT application_id,base,currency,bonus,equity,deadline,notes FROM offers ORDER BY application_id",
            |r| {
                Ok(Offer {
                    application_id: r.get(0)?,
                    base: r.get(1)?,
                    currency: r.get(2)?,
                    bonus: r.get(3)?,
                    equity: r.get(4)?,
                    deadline: r.get(5)?,
                    notes: r.get(6)?,
                })
            },
        )?,
        files: vec![],
    })
}

/// What a backup wrote.
pub struct BackupReport {
    pub applications: usize,
    /// Private document copies embedded in the file.
    pub files: usize,
    pub missing_documents: Vec<String>,
}

/// Writes a full backup. With `include_documents`, private copies (`attach --copy`)
/// are embedded so a restore on another computer brings them along.
pub fn backup(
    store: &Store,
    path: &Path,
    force: bool,
    include_documents: bool,
) -> Result<BackupReport> {
    // One read transaction so every table comes from the same snapshot.
    store.0.execute_batch("BEGIN")?;
    let data = read_all(store);
    store.0.execute_batch("COMMIT")?;
    let mut data = data?;
    if include_documents {
        for d in data.documents.iter().filter(|d| d.managed) {
            let Ok(bytes) = fs::read(&d.path) else {
                continue;
            };
            let name = Path::new(&d.path)
                .file_name()
                .map_or_else(|| "document".into(), |n| n.to_string_lossy().into_owned());
            data.files.push(FileRec {
                document_id: d.id,
                name,
                data: base64::engine::general_purpose::STANDARD.encode(bytes),
            });
        }
    }
    write_new(path, &serde_json::to_vec_pretty(&data)?, force)?;
    Ok(BackupReport {
        applications: data.applications.len(),
        files: data.files.len(),
        missing_documents: missing_documents(data.documents.iter()),
    })
}

/// Deletes files written during a restore unless the restore completed.
struct WrittenFiles(Vec<std::path::PathBuf>, bool);
impl Drop for WrittenFiles {
    fn drop(&mut self) {
        if !self.1 {
            for f in &self.0 {
                let _ = fs::remove_file(f);
            }
        }
    }
}

/// What a restore did.
pub struct RestoreReport {
    pub restored: usize,
    /// Applications left out because the same company and role already exist (merge only).
    pub skipped: Vec<String>,
    pub missing_documents: Vec<String>,
    /// Private copies written back next to the database.
    pub files_restored: usize,
}

/// Restores a backup, all-or-nothing. Without `merge` the database must be empty and ids
/// are preserved. With `merge` the backup is added to existing data: applications already
/// tracked (same company and role) are skipped, the rest get new ids, and their history,
/// posting, interviews, documents, tasks, contacts and offer come along.
pub fn restore(store: &mut Store, path: &Path, merge: bool) -> Result<RestoreReport> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("Cannot read {}", path.display()))?;
    let b: Backup = serde_json::from_str(&raw).context("Not a valid Jobi backup file")?;
    if b.format != BACKUP_FORMAT {
        bail!("Not a Jobi backup (format '{}')", b.format);
    }
    if !(1..=BACKUP_VERSION).contains(&b.version) {
        bail!(
            "Unsupported backup version {} (this Jobi reads 1 to {BACKUP_VERSION})",
            b.version
        );
    }
    let existing: i64 = store
        .0
        .query_row("SELECT COUNT(*) FROM applications", [], |r| r.get(0))?;
    if existing > 0 && !merge {
        bail!(
            "Refusing to restore into a database that already has {existing} application(s). Use an empty database, e.g. `jobi --db new.sqlite3 restore FILE`, or add --merge"
        );
    }
    for a in &b.applications {
        validate_app(&a.company, &a.role, &a.stage, &a.action, &a.due)
            .with_context(|| format!("application #{}", a.id))?;
    }
    for i in &b.interviews {
        validate_interview(&i.starts_at, &i.timezone)
            .with_context(|| format!("interview #{}", i.id))?;
    }
    let mut known: std::collections::HashSet<String> = std::collections::HashSet::new();
    for a in store.list(false)?.into_iter().chain(store.list(true)?) {
        known.insert(dup_key(&a.company, &a.role));
    }
    // old application id -> id in this database; None when the application was skipped
    let mut map: std::collections::HashMap<i64, Option<i64>> = std::collections::HashMap::new();
    let mut skipped = vec![];
    let tx = store.0.transaction()?;
    for a in &b.applications {
        if merge && !known.insert(dup_key(&a.company, &a.role)) {
            skipped.push(format!("{} · {}", a.company, a.role));
            map.insert(a.id, None);
            continue;
        }
        let keep_id = (!merge).then_some(a.id);
        tx.execute(
            "INSERT INTO applications(id,company,role,stage,action,due,archived) VALUES(?,?,?,?,?,?,?)",
            params![keep_id, a.company, a.role, a.stage, a.action, a.due, a.archived],
        )?;
        map.insert(a.id, Some(tx.last_insert_rowid()));
    }
    // Resolves a child's parent: Ok(None) means "parent skipped, drop the row".
    let parent = |old: i64, what: &str| -> Result<Option<i64>> {
        map.get(&old)
            .copied()
            .with_context(|| format!("{what} refers to missing application #{old}"))
    };
    let child_id = |id: i64| (!merge).then_some(id);
    for a in &b.activity {
        if let Some(app) = parent(a.application_id, "history entry")? {
            tx.execute(
                "INSERT INTO activity(id,application_id,at,message) VALUES(?,?,?,?)",
                params![child_id(a.id), app, a.at, a.message],
            )?;
        }
    }
    for p in &b.postings {
        if let Some(app) = parent(p.application_id, "posting")? {
            tx.execute(
                "INSERT INTO postings(application_id,url,description) VALUES(?,?,?)",
                params![app, p.url, p.description],
            )?;
        }
    }
    for i in &b.interviews {
        if let Some(app) = parent(i.application_id, "interview")? {
            tx.execute(
                "INSERT INTO interviews(id,application_id,starts_at,timezone,kind,meeting_url,notes) VALUES(?,?,?,?,?,?,?)",
                params![child_id(i.id), app, i.starts_at, i.timezone, i.kind, i.meeting_url, i.notes],
            )?;
        }
    }
    let mut written = WrittenFiles(vec![], false);
    for d in &b.documents {
        if let Some(app) = parent(d.application_id, "document")? {
            let mut doc_path = d.path.clone();
            if let Some(f) = b.files.iter().find(|f| d.managed && f.document_id == d.id) {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(&f.data)
                    .with_context(|| format!("document #{} has damaged file data", d.id))?;
                // Only the file name is used, so a crafted backup cannot write elsewhere.
                let name = Path::new(&f.name)
                    .file_name()
                    .context("embedded document has no file name")?
                    .to_string_lossy();
                let db = tx.path().filter(|p| !p.is_empty());
                let db = db.context("Restoring document copies needs an on-disk database")?;
                let dir = Path::new(db)
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join("documents")
                    .join(app.to_string());
                fs::create_dir_all(&dir)?;
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |t| t.as_nanos());
                let dest = dir.join(format!("{nanos}-{}-{name}", written.0.len()));
                fs::write(&dest, bytes)?;
                doc_path = dest.to_string_lossy().into_owned();
                written.0.push(dest);
            }
            tx.execute(
                "INSERT INTO documents(id,application_id,label,path,managed) VALUES(?,?,?,?,?)",
                params![child_id(d.id), app, d.label, doc_path, d.managed],
            )?;
        }
    }
    for t in &b.tasks {
        if let Some(app) = parent(t.application_id, "task")? {
            tx.execute(
                "INSERT INTO tasks(id,application_id,title,due,done) VALUES(?,?,?,?,?)",
                params![child_id(t.id), app, t.title, t.due, t.done],
            )?;
        }
    }
    for c in &b.contacts {
        if let Some(app) = parent(c.application_id, "contact")? {
            tx.execute(
                "INSERT INTO contacts(id,application_id,name,role,email,phone,notes,last_contacted) VALUES(?,?,?,?,?,?,?,?)",
                params![child_id(c.id), app, c.name, c.role, c.email, c.phone, c.notes, c.last_contacted],
            )?;
        }
    }
    for o in &b.offers {
        if let Some(app) = parent(o.application_id, "offer")? {
            tx.execute(
                "INSERT INTO offers(application_id,base,currency,bonus,equity,deadline,notes) VALUES(?,?,?,?,?,?,?)",
                params![app, o.base, o.currency, o.bonus, o.equity, o.deadline, o.notes],
            )?;
        }
    }
    tx.commit()?;
    written.1 = true;
    let restored_ids: std::collections::HashSet<_> =
        b.files.iter().map(|f| f.document_id).collect();
    Ok(RestoreReport {
        restored: b.applications.len() - skipped.len(),
        skipped,
        // A copy that came back in the file is no longer missing.
        missing_documents: missing_documents(
            b.documents
                .iter()
                .filter(|d| !(d.managed && restored_ids.contains(&d.id))),
        ),
        files_restored: written.0.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn mem() -> Store {
        Store::initialize(Connection::open_in_memory().unwrap()).unwrap()
    }
    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("jobi-test-{}-{name}", std::process::id()));
        let _ = fs::remove_file(&d);
        d
    }

    #[test]
    fn csv_preview_reports_row_errors_and_import_is_all_or_nothing() -> Result<()> {
        let p = tmp("bad.csv");
        fs::write(
            &p,
            "company,role,stage,due\nAcme,Eng,Applied,\nBad,Eng,Nope,\nX,Y,Applied,2026-02-30\n\"Q, Inc\",Dev,,\n",
        )?;
        let (rows, errors) = parse_csv(&p)?;
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].company, "Q, Inc");
        assert_eq!(rows[1].stage, "Applied");
        assert_eq!(errors.len(), 2);
        assert!(errors[0].starts_with("line 3:") && errors[1].starts_with("line 4:"));
        // A failing insert mid-way must leave nothing behind.
        let mut s = mem();
        s.0.execute_batch(
            "CREATE TRIGGER t BEFORE INSERT ON postings BEGIN SELECT RAISE(ABORT,'boom'); END;",
        )?;
        let mut with_posting = rows;
        with_posting[1].url = "https://x".into();
        assert!(import_rows(&mut s, &with_posting).is_err());
        assert!(s.list(false)?.is_empty());
        fs::remove_file(p)?;
        Ok(())
    }

    #[test]
    fn duplicates_are_detected_against_database_and_file() -> Result<()> {
        let mut s = mem();
        s.save(None, "Acme", "Eng", "Applied", "", "")?;
        let id = s.save(None, "Old", "Dev", "Applied", "", "")?;
        s.archive(id, true)?;
        let p = tmp("dup.csv");
        fs::write(
            &p,
            "company,role\n  ACME ,eng\nOld,Dev\nNew,Role\nnew,  role\nOther,Role\n",
        )?;
        let (rows, errors) = parse_csv(&p)?;
        assert!(errors.is_empty());
        let (fresh, dups) = split_duplicates(&s, rows)?;
        assert_eq!(
            fresh.iter().map(|r| r.company.as_str()).collect::<Vec<_>>(),
            ["New", "Other"]
        );
        assert_eq!(
            dups,
            [
                (2, "already tracked as #1".to_string()),
                (3, "already tracked as #2".to_string()),
                (5, "same as line 4".to_string())
            ]
        );
        fs::remove_file(p)?;
        Ok(())
    }
    #[test]
    fn csv_rejects_bad_headers() -> Result<()> {
        let p = tmp("hdr.csv");
        fs::write(&p, "company,rol\nA,B\n")?;
        assert!(parse_csv(&p).is_err());
        fs::write(&p, "company,role,extra\nA,B,C\n")?;
        assert!(parse_csv(&p).is_err());
        fs::remove_file(p)?;
        Ok(())
    }

    #[test]
    fn csv_export_import_roundtrip_and_no_overwrite() -> Result<()> {
        let mut a = mem();
        let id = a.save(None, "Acme, \"Co\"", "Eng", "Applied", "Call", "2026-11-20")?;
        a.save_posting(id, "https://x", "line1\nline2")?;
        let p = tmp("rt.csv");
        assert_eq!(export_csv(&a, &p, false)?, 1);
        assert!(export_csv(&a, &p, false).is_err());
        let (rows, errors) = parse_csv(&p)?;
        assert!(errors.is_empty());
        let mut b = mem();
        import_rows(&mut b, &rows)?;
        let got = &b.list(false)?[0];
        assert_eq!(
            (got.company.as_str(), got.due.as_str()),
            ("Acme, \"Co\"", "2026-11-20")
        );
        assert_eq!(
            b.posting(got.id)?,
            ("https://x".into(), "line1\nline2".into())
        );
        fs::remove_file(p)?;
        Ok(())
    }

    #[test]
    fn json_backup_restore_preserves_everything() -> Result<()> {
        let mut a = mem();
        let id = a.save(None, "Acme", "Eng", "Applied", "Call", "2026-11-20")?;
        a.note(id, "hello")?;
        a.save_posting(id, "https://x", "desc")?;
        a.add_interview(
            id,
            "2026-11-20T14:00:00+05:30",
            "Asia/Kolkata",
            "phone",
            "",
            "n",
        )?;
        let doc = tmp("doc.txt");
        fs::write(&doc, "x")?;
        a.add_document(id, "CV", &doc.to_string_lossy())?;
        a.archive(id, true)?;
        let p = tmp("b.json");
        let r = backup(&a, &p, false, false)?;
        assert_eq!((r.applications, r.missing_documents.len()), (1, 0));
        fs::remove_file(&doc)?;
        let mut b = mem();
        let r = restore(&mut b, &p, false)?;
        assert_eq!(r.restored, 1);
        assert_eq!(r.missing_documents.len(), 1); // linked file was deleted
        assert_eq!(b.history(id)?, a.history(id)?);
        assert_eq!(b.posting(id)?, a.posting(id)?);
        assert_eq!(b.interviews(id)?, a.interviews(id)?);
        assert_eq!(b.documents(id)?.len(), 1);
        assert!(b.list(true)?[0].archived);
        assert!(restore(&mut b, &p, false).is_err()); // non-empty target refused
        fs::remove_file(p)?;
        Ok(())
    }

    #[test]
    fn corrupt_backup_leaves_database_untouched() -> Result<()> {
        let p = tmp("corrupt.json");
        fs::write(
            &p,
            r#"{"format":"jobi-backup","version":1,"exported_at":"x","applications":[{"id":1,"company":"A","role":"B","stage":"Applied","action":"","due":"","archived":false}],"activity":[{"id":1,"application_id":99,"at":"t","message":"m"}],"postings":[],"interviews":[],"documents":[]}"#,
        )?;
        let mut s = mem();
        assert!(restore(&mut s, &p, false).is_err());
        assert!(s.list(false)?.is_empty());
        fs::remove_file(p)?;
        Ok(())
    }

    fn rich_store() -> Result<(Store, i64)> {
        let mut a = mem();
        let id = a.save(None, "Acme", "Eng", "Offer", "Reply", "2026-11-20")?;
        a.note(id, "hello")?;
        a.add_task(id, "Send thank-you", "2026-11-21")?;
        a.add_contact(
            id,
            &crate::extras::Contact {
                id: 0,
                name: "Jane".into(),
                role: "Recruiter".into(),
                email: "j@x.com".into(),
                phone: String::new(),
                notes: String::new(),
                last_contacted: String::new(),
            },
        )?;
        a.save_offer(&Offer {
            application_id: id,
            base: 100000,
            currency: "USD".into(),
            bonus: 0,
            equity: String::new(),
            deadline: String::new(),
            notes: String::new(),
        })?;
        Ok((a, id))
    }

    #[test]
    fn backup_v2_roundtrips_tasks_contacts_offers_and_reads_v1() -> Result<()> {
        let (a, id) = rich_store()?;
        let p = tmp("v2.json");
        backup(&a, &p, false, false)?;
        assert!(fs::read_to_string(&p)?.contains("\"version\": 2"));
        let mut b = mem();
        restore(&mut b, &p, false)?;
        assert_eq!(b.tasks(id)?, a.tasks(id)?);
        assert_eq!(b.contacts(id)?, a.contacts(id)?);
        assert_eq!(b.offer(id)?, a.offer(id)?);
        // A version 1 file (no task/contact/offer keys) still restores.
        let v1 = fs::read_to_string(&p)?.replace("\"version\": 2", "\"version\": 1");
        fs::write(&p, v1)?;
        let mut c = mem();
        assert_eq!(restore(&mut c, &p, false)?.restored, 1);
        fs::remove_file(p)?;
        Ok(())
    }

    #[test]
    fn merge_restore_adds_new_skips_existing_and_remaps_ids() -> Result<()> {
        // Back up a store holding both Acme and Newco, merge into one holding Acme and Zed.
        let mut both = mem();
        both.save(None, "Acme", "Eng", "Offer", "Reply", "2026-11-20")?;
        both.save(None, "Newco", "Dev", "Applied", "", "")?;
        both.note(2, "from backup")?;
        both.add_task(2, "Do thing", "")?;
        both.add_task(1, "acme task", "")?;
        let p = tmp("merge.json");
        backup(&both, &p, false, false)?;
        let mut target = mem();
        target.save(None, "Zed", "Ops", "Applied", "", "")?; // id 1 collides with backup ids
        target.save(None, "ACME", "eng", "Applied", "", "")?; // same company and role as backup #1
        let r = restore(&mut target, &p, true)?;
        assert_eq!((r.restored, r.skipped.len()), (1, 1));
        let companies: Vec<_> = target.list(false)?.into_iter().map(|x| x.company).collect();
        assert_eq!(companies, ["ACME", "Newco", "Zed"]);
        let newco = target
            .list(false)?
            .into_iter()
            .find(|x| x.company == "Newco")
            .unwrap();
        assert!(newco.id > 2);
        assert_eq!(target.tasks(newco.id)?[0].title, "Do thing");
        assert!(
            target
                .history(newco.id)?
                .iter()
                .any(|h| h.contains("from backup"))
        );
        // The skipped duplicate brought nothing along.
        assert!(target.tasks(2)?.is_empty());
        // Without --merge a non-empty target is still refused.
        assert!(restore(&mut target, &p, false).is_err());
        fs::remove_file(p)?;
        Ok(())
    }

    #[test]
    fn backup_embeds_private_copies_and_restores_them_elsewhere() -> Result<()> {
        let base = std::env::temp_dir().join(format!("jobi-docs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let (src_dir, dst_dir) = (base.join("src"), base.join("dst"));
        fs::create_dir_all(&src_dir)?;
        fs::create_dir_all(&dst_dir)?;
        let mut a = Store::open(&src_dir.join("a.sqlite3"))?;
        let id = a.save(None, "Acme", "Eng", "Applied", "", "")?;
        let cv = base.join("cv.pdf");
        fs::write(&cv, "resume bytes")?;
        a.add_document_copy(id, "CV", &cv.to_string_lossy())?;
        a.add_document(id, "Link", &cv.to_string_lossy())?; // plain link, not embedded
        let json = base.join("b.json");
        let r = backup(&a, &json, false, true)?;
        assert_eq!((r.applications, r.files), (1, 1));
        let without = base.join("c.json");
        assert_eq!(backup(&a, &without, false, false)?.files, 0);

        // Restore on "another computer": a different database directory.
        let mut b = Store::open(&dst_dir.join("b.sqlite3"))?;
        let r = restore(&mut b, &json, false)?;
        assert_eq!(r.files_restored, 1);
        let docs = b.documents(id)?;
        let copy = docs.iter().find(|d| d.managed).unwrap();
        // Compare path components: temp dirs differ in spelling across platforms (8.3 names, /private).
        let parts: Vec<_> = Path::new(&copy.path)
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        assert!(
            parts.iter().any(|p| p == "dst") && !parts.iter().any(|p| p == "src"),
            "{}",
            copy.path
        );
        assert_eq!(fs::read_to_string(&copy.path)?, "resume bytes");
        assert!(r.missing_documents.is_empty());

        // A traversal attempt in the embedded name only ever uses the file name.
        let stored = a
            .documents(id)?
            .into_iter()
            .find(|d| d.managed)
            .unwrap()
            .path;
        let stored_name = Path::new(&stored)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let evil = fs::read_to_string(&json)?.replace(&stored_name, "../../escape.txt");
        fs::write(base.join("evil.json"), evil)?;
        let mut c = Store::open(&dst_dir.join("c.sqlite3"))?;
        restore(&mut c, &base.join("evil.json"), false)?;
        assert!(!base.join("escape.txt").exists() && !dst_dir.join("escape.txt").exists());

        // A failed restore removes the files it wrote.
        let bad = fs::read_to_string(&json)?.replacen("\"activity\": [", "\"activity\": [{\"id\": 77, \"application_id\": 99, \"at\": \"t\", \"message\": \"m\"},", 1);
        fs::write(base.join("bad.json"), bad)?;
        let mut d = Store::open(&dst_dir.join("d.sqlite3"))?;
        assert!(restore(&mut d, &base.join("bad.json"), false).is_err());
        assert!(d.list(false)?.is_empty());
        drop((a, b, c, d));
        fs::remove_dir_all(&base)?;
        Ok(())
    }

    mod props {
        use super::*;
        use proptest::prelude::*;

        // Printable text plus the awkward characters CSV and JSON must survive.
        fn text() -> impl Strategy<Value = String> {
            "[a-zA-Z0-9 ,;:\"'\\\\\n\t\u{e9}\u{4e2d}\u{1f600}-]{0,40}"
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(48))]

            #[test]
            fn csv_roundtrip_preserves_fields(company in text(), role in text(), desc in text()) {
                prop_assume!(!company.trim().is_empty() && !role.trim().is_empty());
                let mut a = mem();
                let id = a.save(None, &company, &role, "Applied", "", "").unwrap();
                a.save_posting(id, "https://x", &desc).unwrap();
                let p = tmp(&format!("prop-{}.csv", fastrand_name(&company, &role, &desc)));
                export_csv(&a, &p, true).unwrap();
                let (rows, errors) = parse_csv(&p).unwrap();
                fs::remove_file(&p).unwrap();
                prop_assert!(errors.is_empty(), "{errors:?}");
                prop_assert_eq!(rows.len(), 1);
                prop_assert_eq!(&rows[0].company, company.trim());
                prop_assert_eq!(&rows[0].role, role.trim());
                prop_assert_eq!(&rows[0].description, desc.trim());
            }

            #[test]
            fn backup_roundtrip_preserves_history_text(company in text(), role in text(), note in text()) {
                prop_assume!(!company.trim().is_empty() && !role.trim().is_empty() && !note.trim().is_empty());
                let mut a = mem();
                let id = a.save(None, &company, &role, "Applied", "", "").unwrap();
                a.note(id, &note).unwrap();
                let p = tmp(&format!("prop-{}.json", fastrand_name(&company, &role, &note)));
                backup(&a, &p, true, false).unwrap();
                let mut b = mem();
                restore(&mut b, &p, false).unwrap();
                fs::remove_file(&p).unwrap();
                prop_assert_eq!(b.history(id).unwrap(), a.history(id).unwrap());
                prop_assert_eq!(b.list(false).unwrap()[0].company.clone(), company.trim().to_string());
            }
        }

        /// A file name unique to these inputs, so parallel cases never share a file.
        fn fastrand_name(a: &str, b: &str, c: &str) -> u64 {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (a, b, c, std::thread::current().id()).hash(&mut h);
            h.finish()
        }
    }
}
