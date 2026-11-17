//! Popups drawn over the main view: forms, review, statistics and confirmations.
use super::help::draw_help;
use super::widgets::{field, frame, hints, popup, rule};
use super::*;

pub(super) fn draw_popup(f: &mut Frame, app: &App, store: &Store) {
    let pal = app.pal;
    match &app.mode {
        Mode::Form(form) => {
            let r = popup(f, 66, 18);
            let title = if form.id.is_some() {
                "Edit application"
            } else {
                "New application"
            };
            let mut lines = vec![];
            lines.extend(field(&pal, "Company *", &form.values[0], form.field == 0));
            lines.extend(field(&pal, "Role *", &form.values[1], form.field == 1));
            let focused = form.field == 2;
            lines.push(Line::from(Span::styled(
                "Stage  (← → to change)",
                if focused {
                    pal.accent().bold()
                } else {
                    pal.dim()
                },
            )));
            lines.push(Line::from(vec![
                Span::styled(if focused { "▸ ◂ " } else { "  ◂ " }, pal.accent()),
                pal.chip(&app.stages, &form.values[2]),
                Span::styled(" ▸", pal.accent()),
            ]));
            lines.extend(field(&pal, "Next action", &form.values[3], form.field == 3));
            lines.extend(field(
                &pal,
                "Due  (2026-12-01 · tomorrow · +3d · fri)",
                &form.values[4],
                form.field == 4,
            ));
            lines.push(Line::from(Span::styled(form.error.clone(), pal.danger())));
            f.render_widget(
                Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .block(frame(
                        &pal,
                        title,
                        Some(hints(
                            &pal,
                            &[("Tab", "next"), ("Enter", "save"), ("Esc", "cancel")],
                        )),
                    )),
                r,
            );
        }
        Mode::Note(text) => {
            let r = popup(f, 66, 9);
            f.render_widget(
                Paragraph::new(vec![Line::from(vec![
                    Span::raw(text.replace('\n', " ⏎ ")),
                    Span::styled("▏", pal.accent()),
                ])])
                .wrap(Wrap { trim: false })
                .block(frame(
                    &pal,
                    "Add a note",
                    Some(hints(
                        &pal,
                        &[("Enter", "save"), ("Ctrl+E", "editor"), ("Esc", "cancel")],
                    )),
                )),
                r,
            );
        }
        Mode::Help(scroll) => draw_help(f, app, *scroll),
        Mode::Prepare(kind, fields, focus, error, _) => {
            let (title, labels) = spec(*kind);
            let r = popup(f, 76, (labels.len() as u16 * 2 + 4).clamp(8, 24));
            let mut lines = vec![];
            for (i, label) in labels.iter().enumerate() {
                lines.extend(field(&pal, label, &fields[i], *focus == i));
            }
            lines.push(Line::from(Span::styled(error.clone(), pal.danger())));
            f.render_widget(
                Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .block(frame(
                        &pal,
                        title,
                        Some(hints(
                            &pal,
                            &[("Tab", "next"), ("Enter", "save"), ("Esc", "cancel")],
                        )),
                    )),
                r,
            );
        }
        Mode::Inspect(index, sel) => draw_inspect(f, app, store, *index, *sel),
        Mode::Stats(s) => {
            let r = popup(f, 64, 26);
            let mut lines = vec![Line::from(format!(
                "Applications: {} ({} active, {} archived)",
                s.total, s.active, s.archived
            ))];
            lines.push(Line::from(""));
            let max = s.by_stage.iter().map(|(_, n)| *n).max().unwrap_or(0);
            let width = s
                .by_stage
                .iter()
                .map(|(n, _)| n.chars().count())
                .max()
                .unwrap_or(10)
                .max(10);
            for (stage, n) in &s.by_stage {
                let base = stage.split(" (not configured)").next().unwrap_or(stage);
                let style = pal.stage_in(&app.stages, base);
                let bar = "━".repeat(if max == 0 { 0 } else { (n * 24).div_ceil(max) });
                lines.push(Line::from(vec![
                    Span::raw(format!(" {stage:<width$} {n:>3} ")),
                    Span::styled(bar, style),
                ]));
            }
            if s.submitted > 0 {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::raw(format!(
                        "Advanced past application: {} of {} submitted ",
                        s.advanced, s.submitted
                    )),
                    Span::styled(format!("({}%)", s.advanced * 100 / s.submitted), pal.ok()),
                ]));
            }
            if !s.weekly.is_empty() {
                lines.push(Line::from(""));
                lines.push(rule(&pal, "ADDED PER WEEK", r.width.saturating_sub(4)));
                let wmax = s.weekly.iter().map(|(_, n)| *n).max().unwrap_or(0);
                for (w, n) in &s.weekly {
                    let bar = "━".repeat(if wmax == 0 {
                        0
                    } else {
                        (n * 24).div_ceil(wmax)
                    });
                    lines.push(Line::from(vec![
                        Span::raw(format!(" {w} {n:>3} ")),
                        Span::styled(bar, pal.accent()),
                    ]));
                }
            }
            f.render_widget(
                Paragraph::new(lines).block(frame(
                    &pal,
                    "Pipeline statistics",
                    Some(Line::from(Span::styled(" any key closes ", pal.dim()))),
                )),
                r,
            );
        }
        Mode::Remove(kind, _, label) => {
            let r = popup(f, 62, 8);
            let (verb, note) = match kind {
                RemoveKind::Interview => ("Remove interview", "This cannot be undone."),
                RemoveKind::Document => (
                    "Unlink document",
                    "Your own file is never deleted; a copy made by Jobi is.",
                ),
                RemoveKind::Task => ("Delete task", "This cannot be undone."),
                RemoveKind::Contact => ("Delete contact", "This cannot be undone."),
            };
            f.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled(format!("{verb}?"), pal.danger())),
                    Line::from(Span::styled(label.clone(), Style::new().bold())),
                    Line::from(""),
                    Line::from(Span::styled(note, pal.dim())),
                ])
                .wrap(Wrap { trim: false })
                .block(frame(
                    &pal,
                    "Confirm",
                    Some(hints(&pal, &[("y", "confirm"), ("n", "cancel")])),
                )),
                r,
            );
        }
        Mode::Archive(_, state) => {
            let r = popup(f, 62, 7);
            f.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled(
                        format!(
                            "{} this application?",
                            if *state { "Archive" } else { "Restore" }
                        ),
                        Style::new().bold(),
                    )),
                    Line::from(""),
                    Line::from(Span::styled("History is always preserved.", pal.dim())),
                ])
                .block(frame(
                    &pal,
                    "Confirm",
                    Some(hints(&pal, &[("y", "confirm"), ("n", "cancel")])),
                )),
                r,
            );
        }
        Mode::Browse | Mode::Search => {}
    }
}

fn draw_inspect(f: &mut Frame, app: &App, store: &Store, index: usize, sel: usize) {
    let pal = app.pal;
    let r = popup(f, 82, 28);
    let Some(a) = app.rows.get(index) else {
        return;
    };
    let width = r.width.saturating_sub(4);
    let items = inspect_items(store, a.id).unwrap_or_default();
    let posting = store.posting(a.id).unwrap_or_default();
    let mut lines = vec![
        Line::from(vec![
            Span::styled(a.company.clone(), Style::new().bold()),
            Span::styled(format!(" · {}", a.role), pal.dim()),
        ]),
        Line::from(""),
        rule(&pal, "JOB POSTING", width),
        Line::from(if posting.0.is_empty() {
            Span::styled("none · press p in the main view", pal.dim())
        } else {
            Span::raw(posting.0.clone())
        }),
    ];
    let desc: Vec<&str> = posting.1.lines().collect();
    lines.extend(desc.iter().take(4).map(|l| Line::from(l.to_string())));
    if desc.len() > 4 {
        lines.push(Line::from(Span::styled("…", pal.dim())));
    }
    let mut sel_line = 0;
    let mut n = 0;
    for (heading, kind) in [
        ("INTERVIEWS", 0),
        ("DOCUMENTS", 1),
        ("TASKS", 2),
        ("CONTACTS", 3),
    ] {
        lines.push(Line::from(""));
        lines.push(rule(&pal, heading, width));
        let mut any = false;
        while let Some(item) = items.get(n) {
            let this = match item {
                Item::Interview(_) => 0,
                Item::Document(_) => 1,
                Item::Task(_) => 2,
                Item::Contact(_) => 3,
            };
            if this != kind {
                break;
            }
            any = true;
            if n == sel {
                sel_line = lines.len();
            }
            let mark = if n == sel { "▌ " } else { "  " };
            match item {
                Item::Interview(i) => {
                    lines.push(Line::from(vec![
                        Span::styled(mark, pal.accent()),
                        Span::styled(
                            format!("{} {}", i.starts_at, i.timezone),
                            Style::new().bold(),
                        ),
                        Span::raw(format!("  {}", i.kind)),
                    ]));
                    lines.push(Line::from(Span::styled(
                        format!("    {} {}", i.url, i.notes.replace('\n', " ⏎ ")),
                        pal.dim(),
                    )));
                }
                Item::Document(d) => {
                    let mut spans = vec![
                        Span::styled(mark, pal.accent()),
                        Span::raw(d.label.clone()),
                        Span::styled(format!("  {}", d.path), pal.dim()),
                    ];
                    if d.managed {
                        spans.push(Span::styled("  (private copy)", pal.dim()));
                    }
                    if d.missing() {
                        spans.push(Span::styled("  FILE MISSING", pal.danger()));
                    }
                    lines.push(Line::from(spans));
                }
                Item::Task(t) => {
                    let due = if t.due.is_empty() {
                        String::new()
                    } else {
                        format!("  ({})", t.due)
                    };
                    lines.push(Line::from(vec![
                        Span::styled(mark, pal.accent()),
                        if t.done {
                            Span::styled(format!("☑ {}{due}", t.title), pal.dim())
                        } else {
                            Span::raw(format!("☐ {}{due}", t.title))
                        },
                    ]));
                }
                Item::Contact(c) => {
                    lines.push(Line::from(vec![
                        Span::styled(mark, pal.accent()),
                        Span::styled(c.name.clone(), Style::new().bold()),
                        Span::styled(format!("  {} · {} {}", c.role, c.email, c.phone), pal.dim()),
                    ]));
                    if !c.notes.is_empty() || !c.last_contacted.is_empty() {
                        lines.push(Line::from(Span::styled(
                            format!(
                                "    last contacted {} · {}",
                                c.last_contacted,
                                c.notes.replace('\n', " ⏎ ")
                            ),
                            pal.dim(),
                        )));
                    }
                }
            }
            n += 1;
        }
        if !any {
            lines.push(Line::from(Span::styled("  none", pal.dim())));
        }
    }
    lines.push(Line::from(""));
    lines.push(rule(&pal, "OFFER", width));
    lines.push(match &app.offer {
        Some(o) => Line::from(Span::styled(format!("  {}", o.summary()), pal.ok())),
        None => Line::from(Span::styled("  none · press o in the main view", pal.dim())),
    });
    let inner_h = r.height.saturating_sub(2) as usize;
    let scroll = sel_line.saturating_sub(inner_h.saturating_sub(3)) as u16;
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0))
            .block(frame(
                &pal,
                "Review",
                Some(hints(
                    &pal,
                    &[
                        ("j k", "select"),
                        ("Space", "done"),
                        ("e", "edit"),
                        ("r", "remove"),
                        ("Esc", "close"),
                    ],
                )),
            )),
        r,
    );
}
