//! Per-application tasks, contacts and offers.
use crate::store::{Application, Store, validate_due};
use anyhow::{Result, bail};
use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub due: String,
    pub done: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contact {
    pub id: i64,
    pub name: String,
    pub role: String,
    pub email: String,
    pub phone: String,
    pub notes: String,
    pub last_contacted: String,
}
/// One offer per application. Amounts are whole currency units per year.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Offer {
    pub application_id: i64,
    pub base: i64,
    pub currency: String,
    pub bonus: i64,
    pub equity: String,
    pub deadline: String,
    pub notes: String,
}

impl Offer {
    pub fn summary(&self) -> String {
        let mut s = format!("{} {} base", group(self.base), self.currency);
        if self.bonus > 0 {
            s += &format!(" + {} bonus", group(self.bonus));
        }
        if !self.equity.is_empty() {
            s += &format!(" + {}", self.equity);
        }
        if !self.deadline.is_empty() {
            s += &format!(" · decide by {}", self.deadline);
        }
        s
    }
}
fn group(n: i64) -> String {
    let digits = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

impl Store {
    pub fn add_task(&self, id: i64, title: &str, due: &str) -> Result<()> {
        let (title, due) = (title.trim(), due.trim());
        if title.is_empty() {
            bail!("Task title cannot be empty");
        }
        validate_due(due)?;
        self.require_app(id)?;
        self.0.execute(
            "INSERT INTO tasks(application_id,title,due) VALUES(?,?,?)",
            params![id, title, due],
        )?;
        Ok(())
    }
    pub fn tasks(&self, id: i64) -> Result<Vec<Task>> {
        let mut q = self.0.prepare(
            "SELECT id,title,due,done FROM tasks WHERE application_id=? ORDER BY done, due='' , due, id",
        )?;
        Ok(q.query_map([id], |r| {
            Ok(Task {
                id: r.get(0)?,
                title: r.get(1)?,
                due: r.get(2)?,
                done: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn set_task_done(&self, id: i64, done: bool) -> Result<()> {
        if self
            .0
            .execute("UPDATE tasks SET done=? WHERE id=?", params![done, id])?
            == 0
        {
            bail!("Task #{id} not found");
        }
        Ok(())
    }
    pub fn delete_task(&self, id: i64) -> Result<()> {
        if self.0.execute("DELETE FROM tasks WHERE id=?", [id])? == 0 {
            bail!("Task #{id} not found");
        }
        Ok(())
    }

    pub fn add_contact(&self, id: i64, c: &Contact) -> Result<()> {
        if c.name.trim().is_empty() {
            bail!("Contact name cannot be empty");
        }
        if !c.email.trim().is_empty() && !c.email.contains('@') {
            bail!("That does not look like an email address");
        }
        validate_due(c.last_contacted.trim())?;
        self.require_app(id)?;
        self.0.execute(
            "INSERT INTO contacts(application_id,name,role,email,phone,notes,last_contacted) VALUES(?,?,?,?,?,?,?)",
            params![id, c.name.trim(), c.role.trim(), c.email.trim(), c.phone.trim(), c.notes, c.last_contacted.trim()],
        )?;
        Ok(())
    }
    pub fn contacts(&self, id: i64) -> Result<Vec<Contact>> {
        let mut q = self.0.prepare(
            "SELECT id,name,role,email,phone,notes,last_contacted FROM contacts WHERE application_id=? ORDER BY id",
        )?;
        Ok(q.query_map([id], |r| {
            Ok(Contact {
                id: r.get(0)?,
                name: r.get(1)?,
                role: r.get(2)?,
                email: r.get(3)?,
                phone: r.get(4)?,
                notes: r.get(5)?,
                last_contacted: r.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn delete_contact(&self, id: i64) -> Result<()> {
        if self.0.execute("DELETE FROM contacts WHERE id=?", [id])? == 0 {
            bail!("Contact #{id} not found");
        }
        Ok(())
    }

    /// Creates or replaces the application's offer and records it in history.
    pub fn save_offer(&mut self, o: &Offer) -> Result<()> {
        if o.base < 0 || o.bonus < 0 {
            bail!("Amounts cannot be negative");
        }
        let currency = o.currency.trim().to_uppercase();
        if currency.len() != 3 || !currency.chars().all(|c| c.is_ascii_alphabetic()) {
            bail!("Currency must be a 3-letter code such as USD or EUR");
        }
        validate_due(o.deadline.trim())?;
        self.require_app(o.application_id)?;
        let tx = self.0.transaction()?;
        tx.execute(
            "INSERT INTO offers(application_id,base,currency,bonus,equity,deadline,notes) VALUES(?,?,?,?,?,?,?)
             ON CONFLICT(application_id) DO UPDATE SET base=excluded.base,currency=excluded.currency,
             bonus=excluded.bonus,equity=excluded.equity,deadline=excluded.deadline,notes=excluded.notes",
            params![o.application_id, o.base, currency, o.bonus, o.equity.trim(), o.deadline.trim(), o.notes],
        )?;
        let saved = Offer {
            currency,
            ..o.clone()
        };
        tx.execute(
            "INSERT INTO activity(application_id,message) VALUES(?,?)",
            params![
                o.application_id,
                format!("Offer recorded: {}", saved.summary())
            ],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn offer(&self, id: i64) -> Result<Option<Offer>> {
        let mut q = self.0.prepare(
            "SELECT application_id,base,currency,bonus,equity,deadline,notes FROM offers WHERE application_id=?",
        )?;
        let mut rows = q.query_map([id], offer_row)?;
        Ok(rows.next().transpose()?)
    }
    pub fn delete_offer(&self, id: i64) -> Result<()> {
        if self
            .0
            .execute("DELETE FROM offers WHERE application_id=?", [id])?
            == 0
        {
            bail!("Application #{id} has no offer");
        }
        Ok(())
    }
    /// Every offer with its application, highest base first within each currency.
    pub fn offers(&self) -> Result<Vec<(Application, Offer)>> {
        let mut out = vec![];
        for a in self.list(false)?.into_iter().chain(self.list(true)?) {
            if let Some(o) = self.offer(a.id)? {
                out.push((a, o));
            }
        }
        out.sort_by(|x, y| (&x.1.currency, y.1.base).cmp(&(&y.1.currency, x.1.base)));
        Ok(out)
    }
}
fn offer_row(r: &rusqlite::Row) -> rusqlite::Result<Offer> {
    Ok(Offer {
        application_id: r.get(0)?,
        base: r.get(1)?,
        currency: r.get(2)?,
        bonus: r.get(3)?,
        equity: r.get(4)?,
        deadline: r.get(5)?,
        notes: r.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn store() -> Store {
        Store::initialize(rusqlite::Connection::open_in_memory().unwrap()).unwrap()
    }
    #[test]
    fn tasks_contacts_and_offers() -> Result<()> {
        let mut s = store();
        let a = s.save(None, "Acme", "Eng", "Offer", "", "")?;
        let b = s.save(None, "Beta", "Eng", "Offer", "", "")?;
        s.add_task(a, "Send thank-you", "2026-11-20")?;
        s.add_task(a, "Prepare questions", "")?;
        assert!(s.add_task(a, " ", "").is_err() && s.add_task(a, "x", "2026-02-30").is_err());
        assert!(s.add_task(99, "x", "").is_err());
        let t = s.tasks(a)?;
        assert_eq!(t[0].title, "Send thank-you"); // dated before undated
        s.set_task_done(t[0].id, true)?;
        assert!(s.tasks(a)?[1].done); // done tasks sink
        s.delete_task(t[0].id)?;
        assert!(s.delete_task(t[0].id).is_err());

        let c = Contact {
            id: 0,
            name: "Jane".into(),
            role: "Recruiter".into(),
            email: "j@x.com".into(),
            phone: String::new(),
            notes: String::new(),
            last_contacted: "2026-11-01".into(),
        };
        s.add_contact(a, &c)?;
        assert!(
            s.add_contact(
                a,
                &Contact {
                    email: "nope".into(),
                    ..c.clone()
                }
            )
            .is_err()
        );
        assert!(
            s.add_contact(
                a,
                &Contact {
                    name: " ".into(),
                    ..c.clone()
                }
            )
            .is_err()
        );
        assert_eq!(s.contacts(a)?[0].role, "Recruiter");
        s.delete_contact(s.contacts(a)?[0].id)?;

        let offer = |app, base, cur: &str| Offer {
            application_id: app,
            base,
            currency: cur.into(),
            bonus: 5000,
            equity: String::new(),
            deadline: "2026-12-01".into(),
            notes: String::new(),
        };
        s.save_offer(&offer(a, 120000, "usd"))?;
        s.save_offer(&offer(b, 150000, "USD"))?;
        s.save_offer(&offer(a, 125000, "USD"))?; // replaces
        assert!(s.save_offer(&offer(a, -1, "USD")).is_err());
        assert!(s.save_offer(&offer(a, 1, "DOLLARS")).is_err());
        let all = s.offers()?;
        assert_eq!(
            all.iter()
                .map(|(x, _)| x.company.as_str())
                .collect::<Vec<_>>(),
            ["Beta", "Acme"]
        );
        assert_eq!(
            all[1].1.summary(),
            "125,000 USD base + 5,000 bonus · decide by 2026-12-01"
        );
        assert!(s.history(a)?[0].contains("Offer recorded"));
        s.delete_offer(a)?;
        assert!(s.offer(a)?.is_none() && s.delete_offer(a).is_err());
        Ok(())
    }
}
