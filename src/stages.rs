//! The ordered list of application stages, configurable in `config.toml`.
use anyhow::{Result, bail};

pub const DEFAULT_STAGES: &[&str] = &[
    "Wishlist",
    "Applied",
    "Screening",
    "Interview",
    "Offer",
    "Accepted",
    "Rejected",
    "Withdrawn",
];

/// Optional overrides read from the config file; anything left out is derived.
#[derive(Debug, Default, Clone)]
pub struct StageOptions {
    pub stages: Option<Vec<String>>,
    pub closed: Option<Vec<String>>,
    pub stale: Option<Vec<String>>,
    pub advanced: Option<Vec<String>>,
    pub follow_up: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stages {
    /// Every stage, in order. The first is the "not yet applied" stage.
    pub all: Vec<String>,
    /// Finished outcomes (rejected, withdrawn...): shown as closed, never stale.
    pub closed: Vec<String>,
    /// Stages in which an application with no recent activity is flagged stale.
    pub stale: Vec<String>,
    /// Stages that count as "advanced past application" in statistics.
    pub advanced: Vec<String>,
    /// Stage in which a new application gets the automatic follow-up.
    pub follow_up: String,
}

impl Default for Stages {
    fn default() -> Self {
        Self::build(&StageOptions::default()).expect("built-in stages are valid")
    }
}

fn names(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

impl Stages {
    /// Validates the options and derives whatever is not given:
    /// closed = Rejected/Withdrawn if present; stale = every stage after the first that
    /// is neither closed nor the last open one; advanced = the third stage onwards that
    /// is not closed; follow-up = the second stage.
    pub fn build(o: &StageOptions) -> Result<Self> {
        let all = match &o.stages {
            Some(v) => v.iter().map(|s| s.trim().to_string()).collect(),
            None => names(DEFAULT_STAGES),
        };
        if all.len() < 2 {
            bail!("stages needs at least two entries");
        }
        for (i, s) in all.iter().enumerate() {
            if s.is_empty() || s.chars().count() > 24 || s.chars().any(char::is_control) {
                bail!("stage names must be 1-24 printable characters: '{s}'");
            }
            if all[..i].iter().any(|t| t.eq_ignore_ascii_case(s)) {
                bail!("stage '{s}' is listed twice");
            }
        }
        let subset = |key: &str, v: &[String]| -> Result<Vec<String>> {
            v.iter()
                .map(|s| {
                    all.iter()
                        .find(|a| a.eq_ignore_ascii_case(s.trim()))
                        .cloned()
                        .ok_or_else(|| {
                            anyhow::anyhow!("{key}: '{s}' is not in stages ({})", all.join(", "))
                        })
                })
                .collect()
        };
        let closed = match &o.closed {
            Some(v) => subset("closed_stages", v)?,
            None => all
                .iter()
                .filter(|s| ["rejected", "withdrawn"].contains(&s.to_lowercase().as_str()))
                .cloned()
                .collect(),
        };
        if closed.len() >= all.len() {
            bail!("closed_stages cannot include every stage");
        }
        let open: Vec<&String> = all.iter().filter(|s| !closed.contains(s)).collect();
        let stale = match &o.stale {
            Some(v) => subset("stale_stages", v)?,
            None => open
                .iter()
                .skip(1)
                .take(open.len().saturating_sub(2))
                .map(|s| s.to_string())
                .collect(),
        };
        let advanced = match &o.advanced {
            Some(v) => subset("advanced_stages", v)?,
            None => open.iter().skip(2).map(|s| s.to_string()).collect(),
        };
        let follow_up = match &o.follow_up {
            Some(s) => subset("follow_up_stage", std::slice::from_ref(s))?.remove(0),
            None => all[1].clone(),
        };
        Ok(Self {
            all,
            closed,
            stale,
            advanced,
            follow_up,
        })
    }

    /// The configured spelling of `name`, ignoring case, if it is a known stage.
    pub fn canonical(&self, name: &str) -> Option<&str> {
        self.all
            .iter()
            .find(|s| s.eq_ignore_ascii_case(name.trim()))
            .map(String::as_str)
    }
    pub fn contains(&self, name: &str) -> bool {
        self.all.iter().any(|s| s == name)
    }
    pub fn is_closed(&self, name: &str) -> bool {
        self.closed.iter().any(|s| s == name)
    }
    pub fn position(&self, name: &str) -> Option<usize> {
        self.all.iter().position(|s| s == name)
    }
    /// Stages shown in the pipeline bar: everything not closed, in order.
    pub fn pipeline(&self) -> Vec<&str> {
        self.all
            .iter()
            .filter(|s| !self.closed.contains(s))
            .map(String::as_str)
            .collect()
    }
    pub fn check(&self, name: &str) -> Result<()> {
        if !self.contains(name) {
            bail!(
                "Unknown application stage '{name}'. Stages: {}",
                self.all.join(", ")
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn opts(stages: &[&str]) -> StageOptions {
        StageOptions {
            stages: Some(names(stages)),
            ..Default::default()
        }
    }
    #[test]
    fn defaults_match_the_original_eight_stages() {
        let s = Stages::default();
        assert_eq!(s.all.len(), 8);
        assert_eq!(s.closed, ["Rejected", "Withdrawn"]);
        assert_eq!(s.stale, ["Applied", "Screening", "Interview", "Offer"]);
        assert_eq!(s.advanced, ["Screening", "Interview", "Offer", "Accepted"]);
        assert_eq!(s.follow_up, "Applied");
        assert_eq!(
            s.pipeline(),
            [
                "Wishlist",
                "Applied",
                "Screening",
                "Interview",
                "Offer",
                "Accepted"
            ]
        );
    }
    #[test]
    fn custom_lists_derive_sensible_roles() {
        let s = Stages::build(&opts(&[
            "Idea", "Sent", "Call", "Onsite", "Hired", "Declined",
        ]))
        .unwrap();
        assert!(s.closed.is_empty());
        // Without closed_stages the last stage counts as the final outcome, not "stale".
        assert_eq!(s.stale, ["Sent", "Call", "Onsite", "Hired"]);
        assert_eq!(s.advanced, ["Call", "Onsite", "Hired", "Declined"]);
        assert_eq!(s.follow_up, "Sent");
        let s = Stages::build(&StageOptions {
            stages: Some(names(&["Idea", "Sent", "Call", "Hired", "Declined"])),
            closed: Some(names(&["declined"])), // case-insensitive, stored as configured
            follow_up: Some("idea".into()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(s.closed, ["Declined"]);
        assert_eq!(s.follow_up, "Idea");
        assert_eq!(s.canonical(" call "), Some("Call"));
    }
    #[test]
    fn invalid_configurations_are_rejected_with_clear_messages() {
        let err = |o: StageOptions| Stages::build(&o).unwrap_err().to_string();
        assert!(err(opts(&["Only"])).contains("at least two"));
        assert!(err(opts(&["A", "a"])).contains("twice"));
        assert!(err(opts(&["A", ""])).contains("1-24"));
        assert!(err(opts(&["A", &"x".repeat(25)])).contains("1-24"));
        assert!(err(opts(&["A", "B\u{7}"])).contains("1-24"));
        let bad_ref = StageOptions {
            closed: Some(names(&["Nope"])),
            ..Default::default()
        };
        assert!(err(bad_ref).contains("closed_stages: 'Nope' is not in stages"));
        let all_closed = StageOptions {
            stages: Some(names(&["A", "B"])),
            closed: Some(names(&["A", "B"])),
            ..Default::default()
        };
        assert!(err(all_closed).contains("every stage"));
    }
}
