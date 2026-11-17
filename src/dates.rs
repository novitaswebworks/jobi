//! Friendly due-date input ("tomorrow", "+3d", "fri") resolved to YYYY-MM-DD.
use crate::store::validate_due;
use anyhow::{Result, bail};
use chrono::{Datelike, Duration, NaiveDate, Weekday};

const HELP: &str = "Use YYYY-MM-DD, today, tomorrow, +3d, +2w, or a weekday like fri";

fn weekday(s: &str) -> Option<Weekday> {
    let full = [
        "monday",
        "tuesday",
        "wednesday",
        "thursday",
        "friday",
        "saturday",
        "sunday",
    ];
    let i = full
        .iter()
        .position(|d| *d == s || (s.len() == 3 && d.starts_with(s)))?;
    Some(Weekday::try_from(i as u8).expect("0..7"))
}

/// Resolves relative dates against `today`; strict dates are passed through validated.
/// A weekday means its next occurrence, never today.
pub fn resolve_due(input: &str, today: NaiveDate) -> Result<String> {
    let s = input.trim().to_lowercase();
    let date = match s.as_str() {
        "" => return Ok(String::new()),
        "today" => today,
        "tomorrow" | "tom" => today + Duration::days(1),
        _ => {
            if let Some(rest) = s.strip_prefix('+') {
                let (n, unit) = rest.split_at(rest.len().saturating_sub(1));
                let n: i64 = n.parse().map_err(|_| anyhow::anyhow!(HELP))?;
                if !(0..=3650).contains(&n) {
                    bail!("Offset out of range (0-3650). {HELP}");
                }
                match unit {
                    "d" => today + Duration::days(n),
                    "w" => today + Duration::days(n * 7),
                    _ => bail!(HELP),
                }
            } else if let Some(wd) = weekday(&s) {
                let ahead =
                    (wd.num_days_from_monday() + 7 - today.weekday().num_days_from_monday()) % 7;
                today + Duration::days(if ahead == 0 { 7 } else { ahead.into() })
            } else {
                validate_due(&s).map_err(|e| anyhow::anyhow!("{e}. {HELP}"))?;
                return Ok(s);
            }
        }
    };
    Ok(date.format("%Y-%m-%d").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn friendly_dates() {
        let wed = NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();
        let r = |s| resolve_due(s, wed).unwrap();
        assert_eq!(r(""), "");
        assert_eq!(r("today"), "2026-11-18");
        assert_eq!(r("Tomorrow"), "2026-11-19");
        assert_eq!(r("+3d"), "2026-11-21");
        assert_eq!(r("+2w"), "2026-12-02");
        assert_eq!(r("fri"), "2026-11-20");
        assert_eq!(r("wednesday"), "2026-11-25"); // same weekday means next week
        assert_eq!(r("mon"), "2026-11-23");
        assert_eq!(r("2026-12-31"), "2026-12-31");
        for bad in ["2026-02-30", "soon", "+x", "+3m", "+99999d", "2026-1-1"] {
            assert!(resolve_due(bad, wed).is_err(), "{bad}");
        }
    }
}
