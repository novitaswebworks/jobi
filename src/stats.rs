//! Pipeline statistics and sample data.
use crate::store::{STAGES, Store};
use anyhow::{Result, bail};
use chrono::{Duration, NaiveDate};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Stats {
    pub total: usize,
    pub active: usize,
    pub archived: usize,
    pub by_stage: Vec<(String, usize)>,
    /// Applications that left Wishlist.
    pub submitted: usize,
    /// Of those, currently at Screening, Interview, Offer or Accepted.
    pub advanced: usize,
    /// (year-week, applications added) for the most recent weeks.
    pub weekly: Vec<(String, usize)>,
}

pub fn compute(store: &Store) -> Result<Stats> {
    let mut rows = store.list(false)?;
    let active = rows.len();
    rows.extend(store.list(true)?);
    let count = |names: &[&str]| {
        rows.iter()
            .filter(|a| names.contains(&a.stage.as_str()))
            .count()
    };
    let by_stage = STAGES
        .iter()
        .map(|s| (s.to_string(), count(&[s])))
        .collect();
    let mut q = store.0.prepare(
        "SELECT strftime('%Y-W%W', at) AS w, COUNT(*) FROM activity
         WHERE message LIKE 'Created application%' OR message LIKE 'Imported from CSV%'
         GROUP BY w ORDER BY w DESC LIMIT 8",
    )?;
    let mut weekly = q
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, usize>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    weekly.reverse();
    Ok(Stats {
        total: rows.len(),
        active,
        archived: rows.len() - active,
        by_stage,
        submitted: rows.iter().filter(|a| a.stage != "Wishlist").count(),
        advanced: count(&["Screening", "Interview", "Offer", "Accepted"]),
        weekly,
    })
}

fn bar(n: usize, max: usize) -> String {
    "█".repeat(if max == 0 { 0 } else { (n * 20).div_ceil(max) })
}

pub fn render(s: &Stats) -> String {
    let mut out = format!(
        "Applications: {} ({} active, {} archived)\n\n",
        s.total, s.active, s.archived
    );
    let max = s.by_stage.iter().map(|(_, n)| *n).max().unwrap_or(0);
    for (stage, n) in &s.by_stage {
        out += &format!("{stage:<10} {n:>3} {}\n", bar(*n, max));
    }
    if s.submitted > 0 {
        out += &format!(
            "\nAdvanced past application: {} of {} submitted ({}%)\n(currently Screening, Interview, Offer or Accepted)\n",
            s.advanced,
            s.submitted,
            s.advanced * 100 / s.submitted
        );
    }
    if !s.weekly.is_empty() {
        out += "\nAdded per week\n";
        let max = s.weekly.iter().map(|(_, n)| *n).max().unwrap_or(0);
        for (w, n) in &s.weekly {
            out += &format!("{w} {n:>3} {}\n", bar(*n, max));
        }
    }
    out
}

/// Loads a few example applications into an empty database.
pub fn load_sample(store: &mut Store, today: NaiveDate) -> Result<usize> {
    if !store.list(false)?.is_empty() || !store.list(true)?.is_empty() {
        bail!("Sample data only loads into an empty database");
    }
    let day = |n: i64| (today + Duration::days(n)).format("%Y-%m-%d").to_string();
    let rows = [
        (
            "Acme Robotics",
            "Backend Engineer",
            "Applied",
            "Follow up with recruiter",
            day(-1),
        ),
        (
            "Northwind",
            "Platform Engineer",
            "Interview",
            "Prepare system design round",
            day(0),
        ),
        (
            "Lumen Labs",
            "Rust Developer",
            "Screening",
            "Send portfolio link",
            day(3),
        ),
        (
            "Fjord Systems",
            "Site Reliability Engineer",
            "Wishlist",
            "",
            String::new(),
        ),
    ];
    for (company, role, stage, action, due) in &rows {
        store.save(None, company, role, stage, action, due)?;
    }
    store.note(
        1,
        "Sample data: delete or archive these once you add your own",
    )?;
    Ok(rows.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stats_and_sample_data() -> Result<()> {
        let mut s = Store::initialize(rusqlite::Connection::open_in_memory()?)?;
        let today = NaiveDate::from_ymd_opt(2026, 11, 17).unwrap();
        assert_eq!(load_sample(&mut s, today)?, 4);
        assert!(load_sample(&mut s, today).is_err());
        s.archive(4, true)?;
        let st = compute(&s)?;
        assert_eq!((st.total, st.active, st.archived), (4, 3, 1));
        assert_eq!((st.submitted, st.advanced), (3, 2));
        assert_eq!(st.by_stage[3], ("Interview".into(), 1));
        assert_eq!(st.weekly.iter().map(|(_, n)| n).sum::<usize>(), 4);
        let text = render(&st);
        assert!(text.contains("2 of 3 submitted (66%)") && text.contains("Interview"));
        Ok(())
    }
}
