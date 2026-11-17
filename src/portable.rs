//! CSV import/export and complete JSON backup/restore.
use crate::store::{Store, validate_app, validate_interview};
use anyhow::{Context, Result, bail};
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
const BACKUP_VERSION: u32 = 1;

/// Writes `data` to a new file; refuses to replace an existing one without `force`.
fn write_new(path: &Path, data: &[u8], force: bool) -> Result<()> {
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
struct Backup {
    format: String,
    version: u32,
    exported_at: String,
    applications: Vec<AppRec>,
    activity: Vec<ActivityRec>,
    postings: Vec<PostingRec>,
    interviews: Vec<InterviewRec>,
    documents: Vec<DocumentRec>,
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

fn missing_documents(docs: &[DocumentRec]) -> Vec<String> {
    docs.iter()
        .filter(|d| !Path::new(&d.path).is_file())
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
    })
}

/// Writes a full backup; returns (applications, missing linked documents).
pub fn backup(store: &Store, path: &Path, force: bool) -> Result<(usize, Vec<String>)> {
    // One read transaction so every table comes from the same snapshot.
    store.0.execute_batch("BEGIN")?;
    let data = read_all(store);
    store.0.execute_batch("COMMIT")?;
    let data = data?;
    write_new(path, &serde_json::to_vec_pretty(&data)?, force)?;
    Ok((data.applications.len(), missing_documents(&data.documents)))
}

/// Restores a backup into an EMPTY database, all-or-nothing, preserving ids.
/// Returns (applications, missing linked documents).
pub fn restore(store: &mut Store, path: &Path) -> Result<(usize, Vec<String>)> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("Cannot read {}", path.display()))?;
    let b: Backup = serde_json::from_str(&raw).context("Not a valid Jobi backup file")?;
    if b.format != BACKUP_FORMAT {
        bail!("Not a Jobi backup (format '{}')", b.format);
    }
    if b.version != BACKUP_VERSION {
        bail!(
            "Unsupported backup version {} (expected {BACKUP_VERSION})",
            b.version
        );
    }
    let existing: i64 = store
        .0
        .query_row("SELECT COUNT(*) FROM applications", [], |r| r.get(0))?;
    if existing > 0 {
        bail!(
            "Refusing to restore into a database that already has {existing} application(s). Use an empty database, e.g. `jobi --db new.sqlite3 restore FILE`"
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
    let tx = store.0.transaction()?;
    for a in &b.applications {
        tx.execute(
            "INSERT INTO applications(id,company,role,stage,action,due,archived) VALUES(?,?,?,?,?,?,?)",
            params![a.id, a.company, a.role, a.stage, a.action, a.due, a.archived],
        )?;
    }
    for a in &b.activity {
        tx.execute(
            "INSERT INTO activity(id,application_id,at,message) VALUES(?,?,?,?)",
            params![a.id, a.application_id, a.at, a.message],
        )?;
    }
    for p in &b.postings {
        tx.execute(
            "INSERT INTO postings(application_id,url,description) VALUES(?,?,?)",
            params![p.application_id, p.url, p.description],
        )?;
    }
    for i in &b.interviews {
        tx.execute(
            "INSERT INTO interviews(id,application_id,starts_at,timezone,kind,meeting_url,notes) VALUES(?,?,?,?,?,?,?)",
            params![i.id, i.application_id, i.starts_at, i.timezone, i.kind, i.meeting_url, i.notes],
        )?;
    }
    for d in &b.documents {
        tx.execute(
            "INSERT INTO documents(id,application_id,label,path,managed) VALUES(?,?,?,?,?)",
            params![d.id, d.application_id, d.label, d.path, d.managed],
        )?;
    }
    tx.commit()?;
    Ok((b.applications.len(), missing_documents(&b.documents)))
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
        let (n, missing) = backup(&a, &p, false)?;
        assert_eq!((n, missing.len()), (1, 0));
        fs::remove_file(&doc)?;
        let mut b = mem();
        let (n, missing) = restore(&mut b, &p)?;
        assert_eq!(n, 1);
        assert_eq!(missing.len(), 1); // linked file was deleted
        assert_eq!(b.history(id)?, a.history(id)?);
        assert_eq!(b.posting(id)?, a.posting(id)?);
        assert_eq!(b.interviews(id)?, a.interviews(id)?);
        assert_eq!(b.documents(id)?.len(), 1);
        assert!(b.list(true)?[0].archived);
        assert!(restore(&mut b, &p).is_err()); // non-empty target refused
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
        assert!(restore(&mut s, &p).is_err());
        assert!(s.list(false)?.is_empty());
        fs::remove_file(p)?;
        Ok(())
    }
}
