//! Optional user configuration (`config.toml`) and platform directories.
use crate::store::STAGES;
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::path::PathBuf;

pub const THEMES: &[&str] = &["dark", "light", "mono"];

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// IANA timezone used as the interview default; falls back to the system timezone.
    pub default_timezone: Option<String>,
    /// Stage preselected when adding an application.
    pub default_stage: String,
    /// "dark", "light" or "mono" (no colour; also used when NO_COLOR is set).
    pub theme: String,
    /// Active applications with no activity for this many days are flagged as stale.
    pub stale_days: u32,
    /// Days until the automatic follow-up on a new Applied application that has no action.
    pub follow_up_days: u32,
    /// Add a "Follow up" action to new Applied applications that have none.
    pub auto_follow_up: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_timezone: None,
            default_stage: "Applied".into(),
            theme: "dark".into(),
            stale_days: 14,
            follow_up_days: 7,
            auto_follow_up: true,
        }
    }
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

pub fn data_dir() -> Result<PathBuf> {
    let home = home().context("Set JOBI_DB or pass --db to choose storage")?;
    #[cfg(target_os = "macos")]
    let root = home.join("Library/Application Support");
    #[cfg(windows)]
    let root = std::env::var_os("APPDATA").map_or(home.join("AppData/Roaming"), PathBuf::from);
    #[cfg(not(any(target_os = "macos", windows)))]
    let root = std::env::var_os("XDG_DATA_HOME").map_or(home.join(".local/share"), PathBuf::from);
    Ok(root.join("jobi"))
}

pub fn config_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("JOBI_CONFIG") {
        return Some(p.into());
    }
    #[cfg(any(target_os = "macos", windows))]
    return data_dir().ok().map(|d| d.join("config.toml"));
    #[cfg(not(any(target_os = "macos", windows)))]
    return Some(
        std::env::var_os("XDG_CONFIG_HOME")
            .map_or(home()?.join(".config"), PathBuf::from)
            .join("jobi/config.toml"),
    );
}

impl Config {
    pub fn parse(text: &str) -> Result<Self> {
        let c: Config = toml::from_str(text).context("Invalid config")?;
        if !STAGES.contains(&c.default_stage.as_str()) {
            bail!("default_stage must be one of: {}", STAGES.join(", "));
        }
        if !THEMES.contains(&c.theme.as_str()) {
            bail!("theme must be one of: {}", THEMES.join(", "));
        }
        if let Some(tz) = &c.default_timezone {
            tz.parse::<chrono_tz::Tz>()
                .map_err(|_| anyhow::anyhow!("default_timezone '{tz}' is not an IANA timezone"))?;
        }
        Ok(c)
    }

    /// Missing file means defaults; a broken file is an error naming the path.
    pub fn load() -> Result<Self> {
        let Some(path) = config_path() else {
            return Ok(Self::default());
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text).with_context(|| format!("In {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("Cannot read {}", path.display())),
        }
    }

    /// Returns (action, due) for a new application, adding the automatic follow-up when
    /// it was added as Applied with neither an action nor a due date.
    pub fn with_follow_up(
        &self,
        stage: &str,
        action: &str,
        due: &str,
        today: chrono::NaiveDate,
    ) -> (String, String) {
        if self.auto_follow_up
            && stage == "Applied"
            && action.trim().is_empty()
            && due.trim().is_empty()
        {
            let when = today + chrono::Duration::days(self.follow_up_days.into());
            return ("Follow up".into(), when.format("%Y-%m-%d").to_string());
        }
        (action.to_string(), due.to_string())
    }

    pub fn timezone(&self) -> String {
        self.default_timezone
            .clone()
            .or_else(|| iana_time_zone::get_timezone().ok())
            .filter(|tz| tz.parse::<chrono_tz::Tz>().is_ok())
            .unwrap_or_else(|| "UTC".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_defaults_and_validation() {
        let c = Config::parse("").unwrap();
        assert_eq!((c.default_stage.as_str(), c.stale_days), ("Applied", 14));
        let c = Config::parse("default_timezone = \"Europe/Berlin\"\ntheme = \"light\"").unwrap();
        assert_eq!(c.timezone(), "Europe/Berlin");
        for bad in [
            "theme = \"neon\"",
            "default_stage = \"Nope\"",
            "default_timezone = \"Mars/Base\"",
            "unknown_key = 1",
        ] {
            assert!(Config::parse(bad).is_err(), "{bad}");
        }
        assert!(!Config::default().timezone().is_empty());
        let today = chrono::NaiveDate::from_ymd_opt(2026, 11, 17).unwrap();
        let d = Config::default();
        assert_eq!(
            d.with_follow_up("Applied", "", "", today),
            ("Follow up".into(), "2026-11-24".into())
        );
        assert_eq!(
            d.with_follow_up("Applied", "Call", "", today),
            ("Call".into(), String::new())
        );
        assert_eq!(
            d.with_follow_up("Wishlist", "", "", today),
            (String::new(), String::new())
        );
        let off = Config::parse("auto_follow_up = false").unwrap();
        assert_eq!(
            off.with_follow_up("Applied", "", "", today),
            (String::new(), String::new())
        );
    }
}
