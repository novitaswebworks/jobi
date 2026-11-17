//! iCalendar (.ics) export of interviews and, optionally, next-action due dates.
use crate::portable::write_new;
use crate::store::Store;
use anyhow::Result;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use std::path::Path;

/// RFC 5545 text escaping.
fn esc(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace("\r\n", "\\n")
        .replace('\n', "\\n")
}

/// Folds a content line to at most 75 octets per physical line, never splitting a character.
fn fold(line: &str) -> String {
    let (mut out, mut len) = (String::new(), 0);
    for c in line.chars() {
        if len + c.len_utf8() > 75 {
            out.push_str("\r\n ");
            len = 1;
        }
        out.push(c);
        len += c.len_utf8();
    }
    out
}

pub fn render(store: &Store, include_actions: bool, now: DateTime<Utc>) -> Result<(String, usize)> {
    let stamp = now.format("%Y%m%dT%H%M%SZ").to_string();
    let mut lines: Vec<String> = vec![
        "BEGIN:VCALENDAR".into(),
        "VERSION:2.0".into(),
        "PRODID:-//Jobi//Job application tracker//EN".into(),
        "CALSCALE:GREGORIAN".into(),
    ];
    let mut events = 0;
    for a in store.list(false)?.into_iter() {
        for i in store.interviews(a.id)? {
            let Ok(start) = DateTime::parse_from_rfc3339(&i.starts_at) else {
                continue;
            };
            let start = start.with_timezone(&Utc);
            let end = start + Duration::hours(1);
            let kind = if i.kind.is_empty() {
                "Interview".to_string()
            } else {
                i.kind.clone()
            };
            lines.push("BEGIN:VEVENT".into());
            lines.push(format!("UID:jobi-interview-{}@jobi.local", i.id));
            lines.push(format!("DTSTAMP:{stamp}"));
            lines.push(format!("DTSTART:{}", start.format("%Y%m%dT%H%M%SZ")));
            lines.push(format!("DTEND:{}", end.format("%Y%m%dT%H%M%SZ")));
            lines.push(format!(
                "SUMMARY:{}",
                esc(&format!("{kind}: {} ({})", a.company, a.role))
            ));
            let mut desc = i.notes.clone();
            if !i.timezone.is_empty() {
                desc = format!("Timezone: {}\n{desc}", i.timezone);
            }
            lines.push(format!("DESCRIPTION:{}", esc(desc.trim_end())));
            if !i.url.is_empty() {
                lines.push(format!("URL:{}", esc(&i.url)));
            }
            lines.push("END:VEVENT".into());
            events += 1;
        }
        if include_actions
            && !a.action.is_empty()
            && let Ok(day) = NaiveDate::parse_from_str(&a.due, "%Y-%m-%d")
        {
            lines.push("BEGIN:VEVENT".into());
            lines.push(format!("UID:jobi-action-{}-{}@jobi.local", a.id, a.due));
            lines.push(format!("DTSTAMP:{stamp}"));
            lines.push(format!("DTSTART;VALUE=DATE:{}", day.format("%Y%m%d")));
            lines.push(format!(
                "DTEND;VALUE=DATE:{}",
                (day + Duration::days(1)).format("%Y%m%d")
            ));
            lines.push(format!(
                "SUMMARY:{}",
                esc(&format!("{}: {} ({})", a.action, a.company, a.role))
            ));
            lines.push("TRANSP:TRANSPARENT".into());
            lines.push("END:VEVENT".into());
            events += 1;
        }
    }
    lines.push("END:VCALENDAR".into());
    let body: String = lines.iter().map(|l| fold(l) + "\r\n").collect();
    Ok((body, events))
}

pub fn export(store: &Store, path: &Path, include_actions: bool, force: bool) -> Result<usize> {
    let (body, n) = render(store, include_actions, Utc::now())?;
    write_new(path, body.as_bytes(), force)?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interviews_and_actions_become_events() -> Result<()> {
        let mut s = Store::initialize(rusqlite::Connection::open_in_memory()?)?;
        let id = s.save(
            None,
            "Acme, Inc",
            "Eng",
            "Interview",
            "Send CV",
            "2026-11-20",
        )?;
        s.add_interview(
            id,
            "2026-11-20T14:00:00+05:30",
            "Asia/Kolkata",
            "System design",
            "https://meet.example/x",
            "Bring; diagrams\nand notes",
        )?;
        let now = DateTime::parse_from_rfc3339("2026-11-17T00:00:00Z")?.with_timezone(&Utc);
        let (ics, n) = render(&s, true, now)?;
        assert_eq!(n, 2);
        assert!(ics.starts_with("BEGIN:VCALENDAR\r\n") && ics.ends_with("END:VCALENDAR\r\n"));
        assert!(ics.contains("DTSTART:20261120T083000Z\r\n")); // 14:00 at +05:30 is 08:30 UTC
        assert!(ics.contains("DTEND:20261120T093000Z\r\n"));
        assert!(ics.contains("SUMMARY:System design: Acme\\, Inc (Eng)"));
        assert!(ics.contains("Bring\\; diagrams\\nand notes"));
        assert!(ics.contains("DTSTART;VALUE=DATE:20261120\r\n"));
        let (_, n) = render(&s, false, now)?;
        assert_eq!(n, 1);
        // Every physical line, including folded ones, stays within 75 octets.
        assert!(ics.split("\r\n").all(|l| l.len() <= 75), "{ics}");
        Ok(())
    }
    #[test]
    fn folding_never_splits_characters() {
        let long = format!("SUMMARY:{}", "é".repeat(60));
        let folded = fold(&long);
        assert!(folded.split("\r\n").all(|l| l.len() <= 75));
        assert_eq!(folded.replace("\r\n ", ""), long);
    }
}
