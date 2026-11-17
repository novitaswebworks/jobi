//! The keyboard help page.
use super::widgets::{frame, popup};
use super::*;

type Section = (&'static str, &'static [(&'static str, &'static str)]);

const SECTIONS: &[Section] = &[
    (
        "MOVE AROUND",
        &[
            ("1 2 3  Tab", "Today · Applications · Archived"),
            ("j k  ↑ ↓", "Select an application (or click, or the wheel)"),
            ("PgUp PgDn", "Scroll the details pane"),
            ("/", "Search; Enter keeps the filter, Esc clears it"),
        ],
    ),
    (
        "APPLICATIONS",
        &[
            ("a  e", "Add · edit an application"),
            ("n", "Add a dated note (Ctrl+E writes it in your editor)"),
            ("x", "Complete the next action"),
            ("s  S", "Next · previous stage"),
            ("u", "Undo the last x, s, S or archive"),
            ("z", "Archive · restore"),
        ],
    ),
    (
        "INTERVIEW PREP",
        &[
            ("p", "Job posting"),
            ("i", "Schedule an interview"),
            ("d", "Link or copy a document"),
            ("o", "Record an offer"),
            ("c", "Add a contact"),
            ("T", "Add a task"),
            (
                "v",
                "Review all: j k select · Space done · e edit · r remove",
            ),
        ],
    ),
    (
        "OVERVIEW",
        &[
            ("t", "Pipeline statistics"),
            ("L", "Load sample data (empty database only)"),
            ("?", "This help"),
            ("q  Ctrl+C", "Quit"),
        ],
    ),
];

const TIPS: &[&str] = &[
    "Search words match company, role, stage and action.",
    "Filters: stage:interview   due:overdue|today|week|none",
    "Example: rust stage:screen due:week",
    "Dates: 2026-12-01 · today · tomorrow · +3d · +2w · fri",
    "In forms: Tab next field · Ctrl+E long text in $EDITOR",
];

/// Every line of the help page; its length decides how far the page can scroll.
pub(super) fn lines(pal: &super::theme::Palette) -> Vec<Line<'static>> {
    let mut out = vec![];
    for (title, rows) in SECTIONS {
        if !out.is_empty() {
            out.push(Line::from(""));
        }
        out.push(Line::from(Span::styled(*title, pal.accent().bold())));
        for (keys, what) in *rows {
            let keys = keys.split("  ").collect::<Vec<_>>().join(" · ");
            let pad = 20usize.saturating_sub(keys.chars().count()).max(1);
            out.push(Line::from(vec![
                Span::raw(" "),
                Span::styled(keys, pal.accent().bold()),
                Span::raw(" ".repeat(pad)),
                Span::raw(what.to_string()),
            ]));
        }
    }
    out.push(Line::from(""));
    out.push(Line::from(Span::styled("TIPS", pal.accent().bold())));
    for t in TIPS {
        out.push(Line::from(vec![
            Span::styled(" · ", pal.dim()),
            Span::raw(t.to_string()),
        ]));
    }
    out.push(Line::from(""));
    out.push(Line::from(Span::styled(
        format!(
            " Jobi {} · history times are UTC · everything stays on this computer",
            env!("CARGO_PKG_VERSION")
        ),
        pal.dim(),
    )));
    out
}

pub(super) fn draw_help(f: &mut Frame, app: &App, scroll: u16) {
    let pal = app.pal;
    let all = lines(&pal);
    let r = popup(f, 84, (all.len() as u16 + 2).min(30));
    let inner = r.height.saturating_sub(2);
    let max = (all.len() as u16).saturating_sub(inner);
    app.help_max.set(max);
    let more = if max > 0 { " · ↑/↓ scroll" } else { "" };
    f.render_widget(
        Paragraph::new(all)
            .scroll((scroll.min(max), 0))
            .block(frame(
                &pal,
                "Keyboard help",
                Some(Line::from(Span::styled(
                    format!(" any other key closes{more} "),
                    pal.dim(),
                ))),
            )),
        r,
    );
}
