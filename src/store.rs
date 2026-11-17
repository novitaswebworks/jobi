use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use rusqlite::{Connection, params};
use std::path::Path;

pub const STAGES: &[&str] = &[
    "Wishlist",
    "Applied",
    "Screening",
    "Interview",
    "Offer",
    "Accepted",
    "Rejected",
    "Withdrawn",
];
#[derive(Debug, Clone)]
pub struct Application {
    pub id: i64,
    pub company: String,
    pub role: String,
    pub stage: String,
    pub action: String,
    pub due: String,
    pub archived: bool,
}
/// Schema history. Entry N-1 upgrades a database from version N-1 to N and must only
/// ever be appended to: released entries are never edited.
const MIGRATIONS: &[&str] = &[
    // 1: original schema
    "CREATE TABLE IF NOT EXISTS applications (
       id INTEGER PRIMARY KEY, company TEXT NOT NULL, role TEXT NOT NULL,
       stage TEXT NOT NULL DEFAULT 'Wishlist', action TEXT NOT NULL DEFAULT '',
       due TEXT NOT NULL DEFAULT '', archived INTEGER NOT NULL DEFAULT 0);
     CREATE TABLE IF NOT EXISTS activity (
       id INTEGER PRIMARY KEY, application_id INTEGER NOT NULL REFERENCES applications(id),
       at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')), message TEXT NOT NULL);",
    // 2: interview preparation
    "CREATE TABLE IF NOT EXISTS interviews (
       id INTEGER PRIMARY KEY, application_id INTEGER NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
       starts_at TEXT NOT NULL, timezone TEXT NOT NULL, kind TEXT NOT NULL DEFAULT '',
       meeting_url TEXT NOT NULL DEFAULT '', notes TEXT NOT NULL DEFAULT '');
     CREATE TABLE IF NOT EXISTS documents (
       id INTEGER PRIMARY KEY, application_id INTEGER NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
       label TEXT NOT NULL, path TEXT NOT NULL, managed INTEGER NOT NULL DEFAULT 0);
     CREATE TABLE IF NOT EXISTS postings (
       application_id INTEGER PRIMARY KEY REFERENCES applications(id) ON DELETE CASCADE,
       url TEXT NOT NULL DEFAULT '', description TEXT NOT NULL DEFAULT '');",
];
/// Upgrades `db` to `target`, one transactional step per version. Refuses databases
/// written by a newer Jobi instead of silently downgrading them.
fn migrate(db: &Connection, target: usize) -> Result<()> {
    let found: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if found > MIGRATIONS.len() as i64 {
        bail!(
            "This database was created by a newer Jobi (schema {found}, this build supports {}). Upgrade Jobi instead of opening it with this version.",
            MIGRATIONS.len()
        );
    }
    let from = found.max(0) as usize;
    for (v, sql) in MIGRATIONS.iter().enumerate().take(target).skip(from) {
        let tx = db.unchecked_transaction()?;
        tx.execute_batch(sql)
            .with_context(|| format!("Upgrading database to schema {}", v + 1))?;
        tx.execute_batch(&format!("PRAGMA user_version={}", v + 1))?;
        tx.commit()?;
    }
    Ok(())
}
pub struct Store(pub Connection);
#[derive(Debug, Clone, PartialEq)]
pub struct Interview {
    pub id: i64,
    pub starts_at: String,
    pub timezone: String,
    pub kind: String,
    pub url: String,
    pub notes: String,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub id: i64,
    pub label: String,
    pub path: String,
}
impl Document {
    /// Documents are linked by path, so the file may have moved or been deleted.
    pub fn missing(&self) -> bool {
        !Path::new(&self.path).is_file()
    }
}
fn interview_from_row(r: &rusqlite::Row) -> rusqlite::Result<Interview> {
    Ok(Interview {
        id: r.get(0)?,
        starts_at: r.get(1)?,
        timezone: r.get(2)?,
        kind: r.get(3)?,
        url: r.get(4)?,
        notes: r.get(5)?,
    })
}
impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let db = Connection::open(path)
            .with_context(|| format!("Cannot open Jobi database at {}", path.display()))?;
        Self::initialize(db)
    }
    pub(crate) fn initialize(db: Connection) -> Result<Self> {
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        db.execute_batch("PRAGMA foreign_keys=ON;")?;
        migrate(&db, MIGRATIONS.len())?;
        Ok(Self(db))
    }
    fn require_app(&self, id: i64) -> Result<()> {
        let n: i64 =
            self.0
                .query_row("SELECT COUNT(*) FROM applications WHERE id=?", [id], |r| {
                    r.get(0)
                })?;
        if n == 0 {
            bail!("Application #{id} not found");
        }
        Ok(())
    }
    pub fn save_posting(&self, id: i64, url: &str, description: &str) -> Result<()> {
        self.require_app(id)?;
        self.0.execute("INSERT INTO postings(application_id,url,description) VALUES(?,?,?) ON CONFLICT(application_id) DO UPDATE SET url=excluded.url,description=excluded.description",params![id,url.trim(),description])?;
        Ok(())
    }
    pub fn posting(&self, id: i64) -> Result<(String, String)> {
        Ok(self
            .0
            .query_row(
                "SELECT url,description FROM postings WHERE application_id=?",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or_default())
    }
    pub fn add_document(&self, id: i64, label: &str, path: &str) -> Result<()> {
        self.require_app(id)?;
        let path = std::path::Path::new(path)
            .canonicalize()
            .context("Document path does not exist")?;
        if !path.is_file() {
            bail!("Document must be a file");
        }
        self.0.execute(
            "INSERT INTO documents(application_id,label,path) VALUES(?,?,?)",
            params![id, label.trim(), path.to_string_lossy()],
        )?;
        Ok(())
    }
    pub fn documents(&self, id: i64) -> Result<Vec<Document>> {
        let mut q = self
            .0
            .prepare("SELECT id,label,path FROM documents WHERE application_id=? ORDER BY id")?;
        Ok(q.query_map([id], |r| {
            Ok(Document {
                id: r.get(0)?,
                label: r.get(1)?,
                path: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?)
    }
    /// Removes only the link; the file itself is never touched.
    pub fn delete_document(&self, id: i64) -> Result<()> {
        if self.0.execute("DELETE FROM documents WHERE id=?", [id])? == 0 {
            bail!("Document link #{id} not found");
        }
        Ok(())
    }
    pub fn add_interview(
        &self,
        id: i64,
        starts_at: &str,
        timezone: &str,
        kind: &str,
        url: &str,
        notes: &str,
    ) -> Result<()> {
        validate_interview(starts_at, timezone)?;
        self.require_app(id)?;
        self.0.execute("INSERT INTO interviews(application_id,starts_at,timezone,kind,meeting_url,notes) VALUES(?,?,?,?,?,?)",params![id,starts_at,timezone,kind,url,notes])?;
        Ok(())
    }
    pub fn interviews(&self, id: i64) -> Result<Vec<Interview>> {
        let mut q=self.0.prepare("SELECT id,starts_at,timezone,kind,meeting_url,notes FROM interviews WHERE application_id=? ORDER BY starts_at,id")?;
        Ok(q.query_map([id], interview_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn interview(&self, id: i64) -> Result<Interview> {
        self.0
            .query_row(
                "SELECT id,starts_at,timezone,kind,meeting_url,notes FROM interviews WHERE id=?",
                [id],
                interview_from_row,
            )
            .map_err(|_| anyhow::anyhow!("Interview #{id} not found"))
    }
    pub fn update_interview(&self, i: &Interview) -> Result<()> {
        validate_interview(&i.starts_at, &i.timezone)?;
        let n = self.0.execute(
            "UPDATE interviews SET starts_at=?,timezone=?,kind=?,meeting_url=?,notes=? WHERE id=?",
            params![i.starts_at, i.timezone, i.kind, i.url, i.notes, i.id],
        )?;
        if n == 0 {
            bail!("Interview #{} not found", i.id);
        }
        Ok(())
    }
    pub fn delete_interview(&self, id: i64) -> Result<()> {
        if self.0.execute("DELETE FROM interviews WHERE id=?", [id])? == 0 {
            bail!("Interview #{id} not found");
        }
        Ok(())
    }
    pub fn list(&self, archived: bool) -> Result<Vec<Application>> {
        let mut q=self.0.prepare("SELECT id,company,role,stage,action,due,archived FROM applications WHERE archived=? ORDER BY company COLLATE NOCASE,id")?;
        Ok(q.query_map([archived], |r| {
            Ok(Application {
                id: r.get(0)?,
                company: r.get(1)?,
                role: r.get(2)?,
                stage: r.get(3)?,
                action: r.get(4)?,
                due: r.get(5)?,
                archived: r.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn save(
        &mut self,
        id: Option<i64>,
        company: &str,
        role: &str,
        stage: &str,
        action: &str,
        due: &str,
    ) -> Result<i64> {
        let (company, role, action, due) = (company.trim(), role.trim(), action.trim(), due.trim());
        validate_app(company, role, stage, action, due)?;
        let tx = self.0.transaction()?;
        let result;
        let message;
        if let Some(id) = id {
            let old: (String, String, String, String, String) = tx.query_row(
                "SELECT company,role,stage,action,due FROM applications WHERE id=?",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )?;
            let mut changes = Vec::new();
            if old.0 != company {
                changes.push(format!("Company: {} → {company}", old.0));
            }
            if old.1 != role {
                changes.push(format!("Role: {} → {role}", old.1));
            }
            if old.2 != stage {
                changes.push(format!("Stage: {} → {stage}", old.2));
            }
            if old.3 != action || old.4 != due {
                changes.push(format!(
                    "Next action: {} [{}] → {action} [{due}]",
                    old.3, old.4
                ));
            }
            tx.execute(
                "UPDATE applications SET company=?,role=?,stage=?,action=?,due=? WHERE id=?",
                params![company, role, stage, action, due, id],
            )?;
            result = id;
            message = changes.join("; ");
        } else {
            tx.execute(
                "INSERT INTO applications(company,role,stage,action,due) VALUES(?,?,?,?,?)",
                params![company, role, stage, action, due],
            )?;
            result = tx.last_insert_rowid();
            message = format!("Created application · {stage}");
        }
        if !message.is_empty() {
            tx.execute(
                "INSERT INTO activity(application_id,message) VALUES(?,?)",
                params![result, message],
            )?;
        }
        tx.commit()?;
        Ok(result)
    }
    pub fn history(&self, id: i64) -> Result<Vec<String>> {
        let mut q=self.0.prepare("SELECT at || ' UTC  ' || message FROM activity WHERE application_id=? ORDER BY id DESC")?;
        Ok(q.query_map([id], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn note(&self, id: i64, note: &str) -> Result<()> {
        if note.trim().is_empty() {
            bail!("Note cannot be empty");
        }
        self.require_app(id)?;
        self.0.execute(
            "INSERT INTO activity(application_id,message) VALUES(?,?)",
            params![id, format!("Note: {}", note.trim())],
        )?;
        Ok(())
    }
    pub fn complete(&mut self, id: i64) -> Result<()> {
        let tx = self.0.transaction()?;
        let action: String = tx
            .query_row("SELECT action FROM applications WHERE id=?", [id], |r| {
                r.get(0)
            })
            .map_err(|_| anyhow::anyhow!("Application #{id} not found"))?;
        if action.is_empty() {
            bail!("This application has no open action");
        }
        tx.execute(
            "INSERT INTO activity(application_id,message) VALUES(?,?)",
            params![id, format!("Completed: {action}")],
        )?;
        tx.execute("UPDATE applications SET action='',due='' WHERE id=?", [id])?;
        tx.commit()?;
        Ok(())
    }
    pub fn archive(&mut self, id: i64, archived: bool) -> Result<()> {
        let tx = self.0.transaction()?;
        if tx.execute(
            "UPDATE applications SET archived=? WHERE id=?",
            params![archived, id],
        )? == 0
        {
            bail!("Application not found");
        }
        tx.execute(
            "INSERT INTO activity(application_id,message) VALUES(?,?)",
            params![id, if archived { "Archived" } else { "Restored" }],
        )?;
        tx.commit()?;
        Ok(())
    }
}
/// Validates already-trimmed application fields; shared by save, CSV import and restore.
pub fn validate_app(company: &str, role: &str, stage: &str, action: &str, due: &str) -> Result<()> {
    if company.is_empty() || role.is_empty() {
        bail!("Company and role are required");
    }
    if !STAGES.contains(&stage) {
        bail!("Unknown application stage '{stage}'");
    }
    validate_due(due)?;
    if !due.is_empty() && action.is_empty() {
        bail!("Add an action before setting its due date");
    }
    Ok(())
}
pub fn validate_interview(starts_at: &str, timezone: &str) -> Result<()> {
    chrono::DateTime::parse_from_rfc3339(starts_at)
        .context("Use RFC3339 datetime, e.g. 2026-11-20T14:00:00+05:30")?;
    timezone
        .parse::<chrono_tz::Tz>()
        .context("Use an IANA timezone, e.g. Asia/Kolkata")?;
    Ok(())
}
pub fn validate_due(due: &str) -> Result<()> {
    if !due.is_empty() {
        let d =
            NaiveDate::parse_from_str(due, "%Y-%m-%d").context("Use a valid date: YYYY-MM-DD")?;
        if d.format("%Y-%m-%d").to_string() != due {
            bail!("Use YYYY-MM-DD, including leading zeroes");
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lifecycle_preserves_history() -> Result<()> {
        let mut s = Store::initialize(Connection::open_in_memory()?)?;
        let id = s.save(
            None,
            "Acme",
            "Engineer",
            "Applied",
            "Follow up",
            "2026-11-20",
        )?;
        s.complete(id)?;
        assert!(s.list(false)?[0].action.is_empty());
        assert!(
            s.history(id)?
                .iter()
                .any(|x| x.contains("Completed: Follow up"))
        );
        s.save(Some(id), "Acme", "Engineer", "Interview", "Prepare", "")?;
        assert!(
            s.history(id)?
                .iter()
                .any(|x| x.contains("Applied → Interview"))
        );
        s.archive(id, true)?;
        assert!(s.list(false)?.is_empty());
        s.archive(id, false)?;
        assert_eq!(s.list(false)?.len(), 1);
        Ok(())
    }
    #[test]
    fn interview_edit_remove_and_document_unlink() -> Result<()> {
        let mut s = Store::initialize(Connection::open_in_memory()?)?;
        let id = s.save(None, "Acme", "Engineer", "Applied", "", "")?;
        s.add_interview(
            id,
            "2026-11-20T14:00:00+05:30",
            "Asia/Kolkata",
            "phone",
            "",
            "",
        )?;
        let mut i = s.interviews(id)?.remove(0);
        i.kind = "onsite".into();
        s.update_interview(&i)?;
        assert_eq!(s.interview(i.id)?.kind, "onsite");
        i.starts_at = "bad".into();
        assert!(s.update_interview(&i).is_err());
        assert_eq!(s.interview(i.id)?.starts_at, "2026-11-20T14:00:00+05:30");
        s.delete_interview(i.id)?;
        assert!(s.interviews(id)?.is_empty());
        assert!(s.delete_interview(i.id).is_err());
        let f = std::env::temp_dir().join(format!("jobi-doc-{}", std::process::id()));
        std::fs::write(&f, "x")?;
        s.add_document(id, "CV", &f.to_string_lossy())?;
        let d = s.documents(id)?.remove(0);
        assert!(!d.missing());
        std::fs::remove_file(&f)?;
        assert!(s.documents(id)?[0].missing());
        s.delete_document(d.id)?;
        assert!(s.documents(id)?.is_empty());
        assert!(s.delete_document(d.id).is_err());
        Ok(())
    }
    #[test]
    fn upgrades_from_every_older_schema_keep_data() -> Result<()> {
        for old in 1..MIGRATIONS.len() {
            let db = Connection::open_in_memory()?;
            db.execute_batch("PRAGMA foreign_keys=ON;")?;
            migrate(&db, old)?;
            db.execute_batch(
                "INSERT INTO applications(company,role,stage) VALUES('Old','Dev','Applied');
                 INSERT INTO activity(application_id,at,message) VALUES(1,'2026-01-02 03:04:05','kept');",
            )?;
            let s = Store::initialize(db)?;
            let v: i64 = s.0.query_row("PRAGMA user_version", [], |r| r.get(0))?;
            assert_eq!(v, MIGRATIONS.len() as i64, "from v{old}");
            assert_eq!(s.list(false)?[0].company, "Old");
            assert!(s.history(1)?[0].contains("kept"));
            s.save_posting(1, "https://x", "d")?; // new tables are usable
        }
        // Re-opening an up-to-date database changes nothing.
        let s = Store::initialize(Connection::open_in_memory()?)?;
        migrate(&s.0, MIGRATIONS.len())?;
        Ok(())
    }
    #[test]
    fn missing_application_and_newer_schema_give_clear_errors() -> Result<()> {
        let mut s = Store::initialize(Connection::open_in_memory()?)?;
        for e in [
            s.note(9, "x").unwrap_err(),
            s.save_posting(9, "", "").unwrap_err(),
            s.add_interview(9, "2026-11-20T14:00:00+05:30", "Asia/Kolkata", "", "", "")
                .unwrap_err(),
            s.complete(9).unwrap_err(),
        ] {
            assert_eq!(e.to_string(), "Application #9 not found");
        }
        let newer = Connection::open_in_memory()?;
        newer.execute_batch("PRAGMA user_version=99;")?;
        let err = Store::initialize(newer)
            .err()
            .expect("must refuse")
            .to_string();
        assert!(err.contains("newer Jobi"), "{err}");
        Ok(())
    }
    #[test]
    fn invalid_changes_do_not_mutate() -> Result<()> {
        let mut s = Store::initialize(Connection::open_in_memory()?)?;
        assert!(s.save(None, "", "Engineer", "Applied", "", "").is_err());
        assert!(
            s.save(None, "Acme", "Engineer", "Applied", "Call", "2026-02-30")
                .is_err()
        );
        assert!(
            s.save(None, "Acme", "Engineer", "Applied", "", "2026-11-20")
                .is_err()
        );
        assert!(s.list(false)?.is_empty());
        Ok(())
    }
    #[test]
    fn failed_history_write_rolls_back_application_update() -> Result<()> {
        let mut s = Store::initialize(Connection::open_in_memory()?)?;
        let id = s.save(None, "Acme", "Engineer", "Applied", "Call", "")?;
        s.0.execute_batch("CREATE TRIGGER reject_activity BEFORE INSERT ON activity BEGIN SELECT RAISE(ABORT, 'simulated failure'); END;")?;
        assert!(s.complete(id).is_err());
        assert_eq!(s.list(false)?[0].action, "Call");
        assert!(
            s.save(Some(id), "Acme", "Engineer", "Interview", "Prepare", "")
                .is_err()
        );
        assert_eq!(s.list(false)?[0].stage, "Applied");
        assert_eq!(s.history(id)?.len(), 1);
        Ok(())
    }
}
