//! Pipeline statistics and sample data.
use crate::store::Store;
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
    let stages = store.stages();
    let mut by_stage: Vec<(String, usize)> = stages
        .all
        .iter()
        .map(|s| (s.clone(), count(&[s.as_str()])))
        .collect();
    // Stages found in the data that are no longer configured stay visible.
    let mut legacy: Vec<&str> = rows
        .iter()
        .map(|a| a.stage.as_str())
        .filter(|s| !stages.contains(s))
        .collect();
    legacy.sort_unstable();
    legacy.dedup();
    by_stage.extend(
        legacy
            .into_iter()
            .map(|s| (format!("{s} (not configured)"), count(&[s]))),
    );
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
        submitted: rows.iter().filter(|a| a.stage != stages.all[0]).count(),
        advanced: rows
            .iter()
            .filter(|a| stages.advanced.contains(&a.stage))
            .count(),
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
    let width = s
        .by_stage
        .iter()
        .map(|(n, _)| n.chars().count())
        .max()
        .unwrap_or(10)
        .max(10);
    for (stage, n) in &s.by_stage {
        out += &format!("{stage:<width$} {n:>3} {}\n", bar(*n, max));
    }
    if s.submitted > 0 {
        out += &format!(
            "\nAdvanced past application: {} of {} submitted ({}%)\n(currently in one of the advanced stages)\n",
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
    // Example stages are picked by position so they exist in any configured stage list.
    let pick = |name: &str| match name {
        "Wishlist" => 0,
        "Applied" => 1,
        "Screening" => 2,
        _ => 3,
    };
    for (company, role, stage, action, due) in &rows {
        let all = &store.stages().all;
        let stage = all[pick(stage).min(all.len() - 1)].clone();
        store.save(None, company, role, &stage, action, due)?;
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

    #[test]
    fn stats_use_configured_stages_and_show_leftovers() -> Result<()> {
        let mut s = Store::initialize(rusqlite::Connection::open_in_memory()?)?;
        s.save(None, "A", "R", "Applied", "", "")?; // will become a leftover
        s.save(None, "B", "R", "Interview", "", "")?;
        let custom = crate::config::Config::parse(
            "stages = [\"Idea\", \"Sent\", \"Call\", \"Hired\"]\ndefault_stage = \"Sent\"",
        )?
        .stages()?;
        s.set_stages(custom);
        s.save(None, "C", "R", "Call", "", "")?;
        let st = compute(&s)?;
        let names: Vec<_> = st.by_stage.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "Idea",
                "Sent",
                "Call",
                "Hired",
                "Applied (not configured)",
                "Interview (not configured)"
            ]
        );
        assert_eq!((st.submitted, st.advanced), (3, 1)); // only "Call" is configured as advanced
        // The sample data adapts to the configured list.
        let mut fresh = Store::initialize(rusqlite::Connection::open_in_memory()?)?;
        fresh.set_stages(s.stages().clone());
        assert_eq!(
            load_sample(&mut fresh, NaiveDate::from_ymd_opt(2026, 11, 17).unwrap())?,
            4
        );
        assert!(
            fresh
                .list(false)?
                .iter()
                .all(|a| fresh.stages().contains(&a.stage))
        );
        Ok(())
    }
}
