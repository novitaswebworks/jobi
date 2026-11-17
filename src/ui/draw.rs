//! Rendering: layout, list, details pane and popups.
use super::*;

pub(super) fn draw(f: &mut Frame, app: &App, store: &Store) {
    let area = f.area();
    if area.width < 50 || area.height < 18 {
        f.render_widget(
            Paragraph::new(
                "Jobi needs at least 50 columns × 18 rows.\nResize the terminal. Ctrl+C exits.",
            ),
            area,
        );
        return;
    }
    let parts = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .split(area);
    let pal = app.pal;
    let names = ["1 Today", "2 Applications", "3 Archived"];
    let mut spans = vec![Span::styled(" JOBI  ", pal.accent().bold())];
    for (i, n) in names.iter().enumerate() {
        spans.push(Span::styled(
            format!(" {n} "),
            if app.tab == i {
                pal.tab_active()
            } else {
                pal.tab_idle()
            },
        ));
    }
    if app.overdue + app.due_today > 0 {
        spans.push(Span::raw("   "));
        if app.overdue > 0 {
            spans.push(Span::styled(
                format!("{} overdue", app.overdue),
                pal.danger(),
            ));
        }
        if app.overdue > 0 && app.due_today > 0 {
            spans.push(Span::styled(" · ", pal.dim()));
        }
        if app.due_today > 0 {
            spans.push(Span::styled(
                format!("{} due today", app.due_today),
                pal.warn(),
            ));
        }
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).block(Block::default().borders(Borders::BOTTOM)),
        parts[0],
    );
    let panes = if area.width >= 100 {
        Layout::horizontal([Constraint::Percentage(48), Constraint::Percentage(52)]).split(parts[1])
    } else {
        Layout::vertical([Constraint::Percentage(45), Constraint::Percentage(55)]).split(parts[1])
    };
    let now = Local::now().date_naive();
    let today = now.format("%Y-%m-%d").to_string();
    let week = (now + chrono::Duration::days(7))
        .format("%Y-%m-%d")
        .to_string();
    let mut items: Vec<ListItem> = vec![];
    let mut shown_selected = 0;
    let mut group = "";
    for (i, a) in app.rows.iter().enumerate() {
        // The Today view is ordered by due date, so groups appear as contiguous runs.
        if app.tab == 0 {
            let g = if a.due.is_empty() {
                "No date"
            } else if a.due < today {
                "Overdue"
            } else if a.due == today {
                "Today"
            } else if a.due <= week {
                "This week"
            } else {
                "Later"
            };
            if g != group {
                group = g;
                items.push(ListItem::new(Line::from(Span::styled(
                    format!("── {g} ──"),
                    pal.dim(),
                ))));
            }
        }
        if i == app.selected {
            shown_selected = items.len();
        }
        let (due, due_style) = if a.due.is_empty() {
            ("No due date".to_string(), pal.dim())
        } else if a.due < today {
            (format!("OVERDUE {}", a.due), pal.danger())
        } else if a.due == today {
            ("TODAY".to_string(), pal.warn())
        } else {
            (a.due.clone(), pal.dim())
        };
        items.push(ListItem::new(vec![
            Line::from(format!("{} · {}", a.company, a.role)),
            Line::from(vec![
                Span::styled(a.stage.clone(), pal.stage(&a.stage)),
                Span::styled("  |  ", pal.dim()),
                Span::styled(due, due_style),
            ]),
        ]));
    }
    let title = format!(
        " {} records · / {}{} ",
        app.rows.len(),
        app.query,
        if matches!(app.mode, Mode::Search) {
            "▏"
        } else {
            ""
        }
    );
    if items.is_empty() {
        f.render_widget(Paragraph::new(if app.query.is_empty(){"Nothing here yet.\n\n[a] Add an application\n[L] Load sample data (empty database only)\n[2] Browse all applications\n\nToday shows open actions, with overdue and upcoming dates first."}else{"No matching applications. Esc clears search."}).wrap(Wrap{trim:false}).block(Block::bordered().title(title)),panes[0]);
    } else {
        let mut state = ListState::default().with_selected(Some(shown_selected));
        f.render_stateful_widget(
            List::new(items)
                .block(Block::bordered().title(title))
                .highlight_style(pal.selected())
                .highlight_symbol("▌ "),
            panes[0],
            &mut state,
        );
    }
    let detail: Vec<Line> = if let Some(a) = app.rows.get(app.selected) {
        let mut lines = vec![
            Line::from(Span::styled(a.company.clone(), Style::new().bold())),
            Line::from(a.role.clone()),
            Line::from(""),
            pipeline(&pal, &a.stage),
            Line::from(format!(
                "Next action: {}",
                if a.action.is_empty() {
                    "None — press e to add one"
                } else {
                    &a.action
                }
            )),
            Line::from(vec![
                Span::raw("Due: "),
                if a.due.is_empty() {
                    Span::raw("Not set")
                } else if a.due < today && !a.action.is_empty() {
                    Span::styled(format!("{} (overdue)", a.due), pal.danger())
                } else if a.due == today && !a.action.is_empty() {
                    Span::styled(format!("{} (today)", a.due), pal.warn())
                } else {
                    Span::raw(a.due.clone())
                },
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "[p] Posting · [i] Interview · [d] Link document · [v] View details · [s] Stage",
                pal.dim(),
            )),
            Line::from(""),
            Line::from(Span::styled("ACTIVITY · newest first", pal.dim())),
        ];
        for h in &app.history {
            lines.push(Line::from(h.clone()));
            lines.push(Line::from(""));
        }
        lines.pop();
        lines
    } else {
        vec![
            Line::from("Your next career move, one action at a time."),
            Line::from(""),
            Line::from("Everything stays on this computer."),
            Line::from("Press ? for keyboard help."),
        ]
    };
    let details = Paragraph::new(detail).wrap(Wrap { trim: false });
    let inner = Block::bordered().inner(panes[1]);
    let total = u16::try_from(details.line_count(inner.width)).unwrap_or(u16::MAX);
    let max_scroll = total.saturating_sub(inner.height);
    app.max_scroll.set(max_scroll);
    f.render_widget(
        details
            .scroll((app.scroll.min(max_scroll), 0))
            .block(Block::bordered().title(if max_scroll > 0 {
                " Details & history · PgUp/PgDn scroll "
            } else {
                " Details & history "
            })),
        panes[1],
    );
    f.render_widget(
        Paragraph::new(format!(
            "a Add · e Edit · n Note · x Done · s Stage · u Undo · z Archive · ? Help · q Quit\n{}",
            app.status
        ))
        .wrap(Wrap { trim: true })
        .style(pal.accent()),
        parts[2],
    );
    match &app.mode {
        Mode::Form(form) => {
            let r = popup(f, 70, 17);
            let labels = [
                "Company *",
                "Role *",
                "Stage ←/→",
                "Next action",
                "Due (2026-12-01, tomorrow, +3d, fri)",
            ];
            let mut lines = vec![
                Line::from(Span::styled(
                    if form.id.is_some() {
                        "Edit application"
                    } else {
                        "New application"
                    },
                    pal.accent().bold(),
                )),
                Line::from(""),
            ];
            for (i, label) in labels.iter().enumerate() {
                lines.push(Line::from(Span::styled(
                    format!(
                        "{label}: {}{}",
                        form.values[i],
                        if form.field == i { " ▏" } else { "" }
                    ),
                    if form.field == i {
                        pal.accent().bold()
                    } else {
                        Style::default()
                    },
                )));
                lines.push(Line::from(""));
            }
            lines.push(Line::from(
                "Tab/↑/↓ Field · ←/→ Stage · Enter Save · Esc Cancel",
            ));
            lines.push(Line::from(Span::styled(&form.error, pal.danger())));
            f.render_widget(
                Paragraph::new(lines)
                    .block(Block::bordered().title(" Jobi "))
                    .wrap(Wrap { trim: false }),
                r,
            );
        }
        Mode::Note(text) => {
            let r = popup(f, 70, 8);
            f.render_widget(
                Paragraph::new(format!("{text}▏\n\nEnter Save · Esc Cancel"))
                    .wrap(Wrap { trim: false })
                    .block(Block::bordered().title(" Add a note ")),
                r,
            );
        }
        Mode::Help => {
            let r = popup(f, 70, 18);
            f.render_widget(
                Paragraph::new(
                    "1/2/3, Tab  Switch views\nj/k, ↑/↓    Select application\n/           Search (Esc clears)\na / e       Add / edit application\nn / x       Add a note / complete next action\ns / S       Next / previous stage\nu           Undo last x, z or s\nz           Archive / restore\nt           Pipeline statistics\np / i / d   Posting / interview / link document\nv           Details: j/k select, e edit interview,\n            r remove interview / unlink document\nCtrl+E      In a form: edit long text in $EDITOR\nPgUp/PgDn   Scroll details and history\nq, Ctrl+C   Exit\nAny key closes. History times are UTC.",
                )
                .block(Block::bordered().title(" Keyboard help ")),
                r,
            );
        }
        Mode::Prepare(kind, fields, field, error, _) => {
            let (title, labels) = match *kind {
                0 => (
                    "Job posting",
                    vec!["URL", "Description (Ctrl+E opens your editor)"],
                ),
                1 | 3 => (
                    if *kind == 3 {
                        "Edit interview"
                    } else {
                        "Schedule interview"
                    },
                    vec![
                        "Starts at RFC3339 e.g. 2026-11-20T14:00:00+05:30",
                        "Timezone",
                        "Interview type",
                        "Meeting URL",
                        "Notes (Ctrl+E opens your editor)",
                    ],
                ),
                _ => ("Link document", vec!["Label", "Existing file path"]),
            };
            let r = popup(f, 76, 18);
            let mut lines = vec![Line::from("Tab changes fields · Enter saves · Esc cancels")];
            for (i, label) in labels.iter().enumerate() {
                lines.push(Line::from(format!(
                    "{label}: {}{}",
                    fields[i].replace('\n', " ⏎ "),
                    if *field == i { " ▏" } else { "" }
                )));
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(error, pal.danger())));
            f.render_widget(
                Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .block(Block::bordered().title(title)),
                r,
            );
        }
        Mode::Inspect(index, sel) => {
            let r = popup(f, 76, 24);
            if let Some(a) = app.rows.get(*index) {
                let posting = store.posting(a.id).unwrap_or_default();
                let ivs = store.interviews(a.id).unwrap_or_default();
                let docs = store.documents(a.id).unwrap_or_default();
                let mark = |n: usize| if n == *sel { "▌ " } else { "  " };
                let mut c = format!("{} · {}\n\nJOB POSTING\n{}\n", a.company, a.role, posting.0);
                let desc: Vec<&str> = posting.1.lines().collect();
                c += &desc.iter().take(4).copied().collect::<Vec<_>>().join("\n");
                if desc.len() > 4 {
                    c += "\n…";
                }
                c += "\n\nINTERVIEWS\n";
                if ivs.is_empty() {
                    c += "  none\n";
                }
                for (n, i) in ivs.iter().enumerate() {
                    c += &format!(
                        "{}{} {} {}\n    {} {}\n",
                        mark(n),
                        i.starts_at,
                        i.timezone,
                        i.kind,
                        i.url,
                        i.notes.replace('\n', " ⏎ ")
                    );
                }
                c += "\nDOCUMENTS\n";
                if docs.is_empty() {
                    c += "  none\n";
                }
                for (n, d) in docs.iter().enumerate() {
                    c += &format!(
                        "{}{}  {}{}\n",
                        mark(ivs.len() + n),
                        d.label,
                        d.path,
                        if d.missing() { "  (FILE MISSING)" } else { "" }
                    );
                }
                c += "\nj/k select · e edit interview · r remove/unlink · Esc close";
                f.render_widget(
                    Paragraph::new(c)
                        .wrap(Wrap { trim: false })
                        .block(Block::bordered().title("Application details")),
                    r,
                );
            }
        }
        Mode::Stats(text) => {
            let r = popup(f, 60, 24);
            f.render_widget(
                Paragraph::new(format!("{text}\nAny key closes"))
                    .block(Block::bordered().title(" Pipeline statistics ")),
                r,
            );
        }
        Mode::Remove(is_interview, _, label) => {
            let r = popup(f, 60, 7);
            f.render_widget(
                Paragraph::new(format!(
                    "{} {label}?\n{}\n\ny Confirm · n / Esc Cancel",
                    if *is_interview {
                        "Remove interview"
                    } else {
                        "Unlink document"
                    },
                    if *is_interview {
                        "This cannot be undone."
                    } else {
                        "The file itself is not deleted."
                    },
                ))
                .wrap(Wrap { trim: false })
                .block(Block::bordered().title(" Confirm ")),
                r,
            );
        }
        Mode::Archive(_, state) => {
            let r = popup(f, 60, 6);
            f.render_widget(Paragraph::new(format!("{} this application?\nHistory will be preserved.\n\ny Confirm · n / Esc Cancel",if *state{"Archive"}else{"Restore"})).block(Block::bordered().title(" Confirm ")),r);
        }
        _ => {}
    }
}
fn popup(f: &mut Frame, width: u16, height: u16) -> Rect {
    let a = f.area();
    let w = width.min(a.width);
    let h = height.min(a.height);
    let r = Rect::new((a.width - w) / 2, (a.height - h) / 2, w, h);
    f.render_widget(Clear, r);
    r
}

/// "Wishlist › Applied › [Screening] › ..." with the current stage highlighted.
fn pipeline(pal: &super::theme::Palette, stage: &str) -> Line<'static> {
    let main = &STAGES[..6];
    let Some(at) = main.iter().position(|s| *s == stage) else {
        return Line::from(vec![
            Span::styled(format!("● {stage}"), pal.stage(stage).bold()),
            Span::styled("  (closed)", pal.dim()),
        ]);
    };
    let mut spans = vec![];
    for (i, name) in main.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" › ", pal.dim()));
        }
        spans.push(if i == at {
            Span::styled(format!("[{name}]"), pal.stage(stage).bold())
        } else {
            Span::styled(name.to_string(), pal.dim())
        });
    }
    Line::from(spans)
}
