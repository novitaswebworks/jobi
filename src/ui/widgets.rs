//! Small drawing helpers shared by the main view and the popups.
use super::theme::Palette;
use ratatui::{
    prelude::*,
    widgets::{Block, BorderType, Clear, Padding},
};

/// Rounded frame with an accent title and an optional hint line along the bottom edge.
pub(super) fn frame<'a>(pal: &Palette, title: &str, hint: Option<Line<'a>>) -> Block<'a> {
    let mut b = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(pal.border())
        .padding(Padding::horizontal(1))
        .title(Span::styled(format!(" {title} "), pal.accent().bold()));
    if let Some(h) = hint {
        b = b.title_bottom(h);
    }
    b
}

/// ` a ` styled as a keycap.
pub(super) fn keycap(pal: &Palette, k: &str) -> Span<'static> {
    Span::styled(format!(" {k} "), pal.key())
}

/// "a Add  e Edit  ..." with each key drawn as a keycap and each label dimmed.
pub(super) fn hints(pal: &Palette, pairs: &[(&str, &str)]) -> Line<'static> {
    let mut spans = vec![];
    for (i, (k, label)) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" "));
        }
        spans.push(keycap(pal, k));
        spans.push(Span::styled(format!(" {label}"), pal.dim()));
    }
    Line::from(spans)
}

/// "TASKS ────────" filling `width` columns.
pub(super) fn rule(pal: &Palette, title: &str, width: u16) -> Line<'static> {
    let used = title.chars().count() + 1;
    Line::from(vec![
        Span::styled(title.to_string(), pal.accent().bold()),
        Span::styled(
            format!(" {}", "─".repeat((width as usize).saturating_sub(used + 1))),
            pal.border(),
        ),
    ])
}

/// Clears and returns a centred rectangle no larger than the screen.
pub(super) fn popup(f: &mut Frame, width: u16, height: u16) -> Rect {
    let a = f.area();
    let w = width.min(a.width);
    let h = height.min(a.height);
    let r = Rect::new((a.width - w) / 2, (a.height - h) / 2, w, h);
    f.render_widget(Clear, r);
    r
}

/// A labelled input: label on one line, the value on the next with a text cursor when focused.
pub(super) fn field(pal: &Palette, label: &str, value: &str, focused: bool) -> Vec<Line<'static>> {
    let shown = value.replace('\n', " ⏎ ");
    let label_style = if focused {
        pal.accent().bold()
    } else {
        pal.dim()
    };
    let (marker, value_style) = if focused {
        ("▸ ", Style::new().bold())
    } else {
        ("  ", Style::new())
    };
    let mut v = vec![
        Span::styled(marker, pal.accent()),
        Span::styled(shown.clone(), value_style),
    ];
    if focused {
        v.push(Span::styled("▏", pal.accent()));
    } else if shown.is_empty() {
        v.push(Span::styled("—", pal.dim()));
    }
    vec![
        Line::from(Span::styled(label.to_string(), label_style)),
        Line::from(v),
    ]
}
