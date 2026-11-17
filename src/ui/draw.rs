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
    let mut badges: Vec<Span> = vec![];
    if app.overdue > 0 {
        badges.push(Span::styled(
            format!("{} overdue", app.overdue),
            pal.danger(),
        ));
    }
    if app.due_today > 0 {
        badges.push(Span::styled(
            format!("{} due today", app.due_today),
            pal.warn(),
        ));
    }
    if !app.stale.is_empty() {
        badges.push(Span::styled(
            format!("{} stale", app.stale.len()),
            pal.dim(),
        ));
    }
    for (i, b) in badges.into_iter().enumerate() {
        spans.push(if i == 0 {
            Span::raw("   ")
        } else {
            Span::styled(" · ", pal.dim())
        });
        spans.push(b);
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
    let mut meta: Vec<(Option<usize>, u16)> = vec![];
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
                meta.push((None, 1));
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
        let mut second = vec![
            Span::styled(a.stage.clone(), pal.stage(&a.stage)),
            Span::styled("  |  ", pal.dim()),
            Span::styled(due, due_style),
        ];
        if app.stale.contains(&a.id) {
            second.push(Span::styled("  · stale", pal.warn()));
        }
        items.push(ListItem::new(vec![
            Line::from(format!("{} · {}", a.company, a.role)),
            Line::from(second),
        ]));
        meta.push((Some(i), 2));
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
        let mut hit = app.hit.borrow_mut();
        hit.list = Block::bordered().inner(panes[0]);
        hit.offset = state.offset();
        hit.items = meta;
    }
    app.hit.borrow_mut().details = panes[1];
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
        ];
        if app.stale.contains(&a.id) {
            lines.push(Line::from(Span::styled(
                format!(
                    "No activity for {}+ days: time to follow up?",
                    app.config.stale_days
                ),
                pal.warn(),
            )));
        }
        if let Some(o) = &app.offer {
            lines.push(Line::from(vec![
                Span::styled("Offer: ", pal.stage("Offer")),
                Span::raw(o.summary()),
            ]));
        }
        if !app.tasks.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("TASKS", pal.dim())));
            for t in &app.tasks {
                let due = if t.due.is_empty() {
                    String::new()
                } else {
                    format!("  ({})", t.due)
                };
                lines.push(Line::from(if t.done {
                    Span::styled(format!("☑ {}{due}", t.title), pal.dim())
                } else {
                    Span::raw(format!("☐ {}{due}", t.title))
                }));
            }
        }
        if !app.contacts.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("CONTACTS", pal.dim())));
            for c in &app.contacts {
                let parts: Vec<&str> = [&c.role, &c.email, &c.phone]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .map(String::as_str)
                    .collect();
                lines.push(Line::from(format!("{} · {}", c.name, parts.join(" · "))));
            }
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "[p] Posting · [i] Interview · [d] Document · [o] Offer · [c] Contact · [T] Task · [v] Review · [s] Stage",
            pal.dim(),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "ACTIVITY · newest first",
            pal.dim(),
        )));
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
                    .block(Block::bordered().title(" Add a note · Ctrl+E opens your editor ")),
                r,
            );
        }
        Mode::Help(scroll) => {
            let r = popup(f, 70, 22);
            f.render_widget(
                Paragraph::new(HELP).scroll((*scroll, 0)).block(
                    Block::bordered()
                        .title(" Keyboard help ")
                        .title_bottom(" ↑/↓ scroll · Esc closes "),
                ),
                r,
            );
        }
        Mode::Prepare(kind, fields, field, error, _) => {
            let (title, labels) = spec(*kind);
            let r = popup(f, 76, (labels.len() as u16 * 2 + 4).clamp(8, 22));
            let mut lines = vec![Line::from("Tab changes fields · Enter saves · Esc cancels")];
            for (i, label) in labels.iter().enumerate() {
                lines.push(Line::from(Span::styled(
                    format!(
                        "{label}: {}{}",
                        fields[i].replace('\n', " ⏎ "),
                        if *field == i { " ▏" } else { "" }
                    ),
                    if *field == i {
                        pal.accent().bold()
                    } else {
                        Style::default()
                    },
                )));
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(error, pal.danger())));
            f.render_widget(
                Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .block(Block::bordered().title(format!(" {title} "))),
                r,
            );
        }
        Mode::Inspect(index, sel) => {
            let r = popup(f, 80, 26);
            if let Some(a) = app.rows.get(*index) {
                let items = inspect_items(store, a.id).unwrap_or_default();
                let posting = store.posting(a.id).unwrap_or_default();
                let mut lines = vec![
                    Line::from(Span::styled(
                        format!("{} · {}", a.company, a.role),
                        Style::new().bold(),
                    )),
                    Line::from(""),
                    Line::from(Span::styled("JOB POSTING", pal.dim())),
                    Line::from(posting.0.clone()),
                ];
                let desc: Vec<&str> = posting.1.lines().collect();
                lines.extend(desc.iter().take(4).map(|l| Line::from(l.to_string())));
                if desc.len() > 4 {
                    lines.push(Line::from("…"));
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
                    lines.push(Line::from(Span::styled(heading, pal.dim())));
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
                        if n == *sel {
                            sel_line = lines.len();
                        }
                        let mark = if n == *sel { "▌ " } else { "  " };
                        match item {
                            Item::Interview(i) => {
                                lines.push(Line::from(format!(
                                    "{mark}{} {} {}",
                                    i.starts_at, i.timezone, i.kind
                                )));
                                lines.push(Line::from(Span::styled(
                                    format!("    {} {}", i.url, i.notes.replace('\n', " ⏎ ")),
                                    pal.dim(),
                                )));
                            }
                            Item::Document(d) => {
                                let mut spans =
                                    vec![Span::raw(format!("{mark}{}  {}", d.label, d.path))];
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
                                lines.push(Line::from(if t.done {
                                    Span::styled(format!("{mark}☑ {}{due}", t.title), pal.dim())
                                } else {
                                    Span::raw(format!("{mark}☐ {}{due}", t.title))
                                }));
                            }
                            Item::Contact(c) => {
                                lines.push(Line::from(format!(
                                    "{mark}{} · {} · {} {}",
                                    c.name, c.role, c.email, c.phone
                                )));
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
                        lines.push(Line::from("  none"));
                    }
                }
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled("OFFER", pal.dim())));
                lines.push(Line::from(match &app.offer {
                    Some(o) => format!("  {}", o.summary()),
                    None => "  none · press o in the main view to record one".to_string(),
                }));
                let inner_h = r.height.saturating_sub(2) as usize;
                let scroll = sel_line.saturating_sub(inner_h.saturating_sub(3)) as u16;
                f.render_widget(
                    Paragraph::new(lines)
                        .wrap(Wrap { trim: false })
                        .scroll((scroll, 0))
                        .block(
                            Block::bordered()
                                .title(" Application details ")
                                .title_bottom(" j/k select · Space done · e edit interview · r remove · Esc close "),
                        ),
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
        Mode::Remove(kind, _, label) => {
            let r = popup(f, 60, 7);
            let (verb, note) = match kind {
                RemoveKind::Interview => ("Remove interview", "This cannot be undone."),
                RemoveKind::Document => ("Unlink document", "The file itself is not deleted."),
                RemoveKind::Task => ("Delete task", "This cannot be undone."),
                RemoveKind::Contact => ("Delete contact", "This cannot be undone."),
            };
            f.render_widget(
                Paragraph::new(format!(
                    "{verb} {label}?\n{note}\n\ny Confirm · n / Esc Cancel"
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

const HELP: &str = "VIEWS
1/2/3, Tab  Switch views
j/k, ↑/↓    Select (mouse: click, wheel)
/           Search: words, stage:offer,
            due:overdue|today|week|none

APPLICATIONS
a / e       Add / edit application
n / x       Add a note / complete next action
s / S       Next / previous stage
u           Undo last x, z or s
z           Archive / restore
t           Pipeline statistics

DETAILS
p / i / d   Posting / interview / link document
o / c / T   Offer / contact / task
v           Review everything: j/k select,
            Space done, e edit interview,
            r remove or unlink
Ctrl+E      In a form or note: edit in $EDITOR
PgUp/PgDn   Scroll details and history
q, Ctrl+C   Exit

History times are UTC.";
