//! Rendering of the main view: header, application list, details pane and footer.
use super::popups::draw_popup;
use super::widgets::{frame, hints, keycap, rule};
use super::*;
use chrono::NaiveDate;

pub(super) fn draw(f: &mut Frame, app: &App, store: &Store) {
    let area = f.area();
    let pal = app.pal;
    if area.width < 50 || area.height < 18 {
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled("◆ JOBI", pal.accent().bold())),
                Line::from("Needs at least 50 columns × 18 rows."),
                Line::from(Span::styled(
                    "Resize the terminal. Ctrl+C exits.",
                    pal.dim(),
                )),
            ])
            .alignment(Alignment::Center),
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
    header(f, app, parts[0]);
    let panes = if area.width >= 100 {
        Layout::horizontal([Constraint::Percentage(46), Constraint::Percentage(54)]).split(parts[1])
    } else {
        Layout::vertical([Constraint::Percentage(45), Constraint::Percentage(55)]).split(parts[1])
    };
    list_pane(f, app, panes[0]);
    details_pane(f, app, panes[1]);
    footer(f, app, parts[2]);
    draw_popup(f, app, store);
}

fn header(f: &mut Frame, app: &App, area: Rect) {
    let pal = app.pal;
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(pal.border());
    let inner = block.inner(area);
    f.render_widget(block, area);
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(inner);
    let date = Local::now().format("%a %d %b %Y").to_string();
    let cols = Layout::horizontal([
        Constraint::Min(1),
        Constraint::Length(date.chars().count() as u16 + 1),
    ])
    .split(rows[0]);
    let mut tabs = vec![Span::styled(" ◆ JOBI  ", pal.accent().bold())];
    for (i, n) in ["1 Today", "2 Applications", "3 Archived"]
        .iter()
        .enumerate()
    {
        tabs.push(Span::styled(
            format!(" {n} "),
            if app.tab == i {
                pal.tab_active()
            } else {
                pal.tab_idle()
            },
        ));
        tabs.push(Span::raw(" "));
    }
    f.render_widget(Paragraph::new(Line::from(tabs)), cols[0]);
    f.render_widget(
        Paragraph::new(Span::styled(date, pal.dim())).alignment(Alignment::Right),
        cols[1],
    );
    let mut badges: Vec<Span> = vec![Span::raw(" ")];
    let mut push = |n: usize, text: &str, style: Style| {
        if n > 0 {
            if badges.len() > 1 {
                badges.push(Span::raw("   "));
            }
            badges.push(Span::styled(format!("● {n} {text}"), style));
        }
    };
    push(app.overdue, "overdue", pal.danger());
    push(app.due_today, "due today", pal.warn());
    push(app.stale.len(), "stale", pal.dim());
    if badges.len() == 1 {
        badges.push(Span::styled("✓ All clear: nothing due today", pal.ok()));
    }
    f.render_widget(Paragraph::new(Line::from(badges)), rows[1]);
}

/// "2d overdue", "today", "tomorrow", "in 5d" relative to `now`.
fn relative(due: &str, now: NaiveDate) -> Option<(String, i64)> {
    let d = NaiveDate::parse_from_str(due, "%Y-%m-%d").ok()?;
    let days = (d - now).num_days();
    let text = match days {
        i64::MIN..=-1 => format!("{}d overdue", -days),
        0 => "today".into(),
        1 => "tomorrow".into(),
        n => format!("in {n}d"),
    };
    Some((text, days))
}

fn list_pane(f: &mut Frame, app: &App, area: Rect) {
    let pal = app.pal;
    let now = Local::now().date_naive();
    let today = now.format("%Y-%m-%d").to_string();
    let week = (now + chrono::Duration::days(7))
        .format("%Y-%m-%d")
        .to_string();
    let group_of = |due: &str| {
        if due.is_empty() {
            "No date"
        } else if due < today.as_str() {
            "Overdue"
        } else if due == today {
            "Today"
        } else if due <= week.as_str() {
            "This week"
        } else {
            "Later"
        }
    };
    let mut items: Vec<ListItem> = vec![];
    let mut meta: Vec<(Option<usize>, u16)> = vec![];
    let mut shown_selected = 0;
    let mut group = "";
    for (i, a) in app.rows.iter().enumerate() {
        // The Today view is ordered by due date, so groups appear as contiguous runs.
        if app.tab == 0 {
            let g = group_of(&a.due);
            if g != group {
                group = g;
                let n = app.rows.iter().filter(|r| group_of(&r.due) == g).count();
                let style = match g {
                    "Overdue" => pal.danger(),
                    "Today" => pal.warn(),
                    _ => pal.dim(),
                };
                items.push(ListItem::new(Line::from(vec![
                    Span::styled(format!(" {g}"), style),
                    Span::styled(format!(" · {n} "), pal.dim()),
                    Span::styled("─".repeat(20), pal.border()),
                ])));
                meta.push((None, 1));
            }
        }
        if i == app.selected {
            shown_selected = items.len();
        }
        let rel = relative(&a.due, now);
        let (marker, due_text, due_style) = match (&rel, a.due.is_empty()) {
            (_, true) => (Span::raw(" "), "no due date".to_string(), pal.dim()),
            (Some((t, d)), _) if *d < 0 && !a.action.is_empty() => {
                (Span::styled("▍", pal.danger()), t.clone(), pal.danger())
            }
            (Some((t, 0)), _) if !a.action.is_empty() => {
                (Span::styled("▍", pal.warn()), t.clone(), pal.warn())
            }
            (Some((t, _)), _) => (Span::raw(" "), t.clone(), pal.dim()),
            (None, _) => (Span::raw(" "), a.due.clone(), pal.dim()),
        };
        let mut second = vec![
            Span::raw(" "),
            pal.chip(&app.stages, &a.stage),
            Span::raw("  "),
            Span::styled(due_text, due_style),
        ];
        if !a.due.is_empty() {
            second.push(Span::styled(format!(" · {}", a.due), pal.dim()));
        }
        if app.stale.contains(&a.id) {
            second.push(Span::styled("  ⏳ stale", pal.warn()));
        }
        items.push(ListItem::new(vec![
            Line::from(vec![
                marker,
                Span::styled(a.company.clone(), Style::new().bold()),
                Span::styled(format!(" · {}", a.role), pal.dim()),
            ]),
            Line::from(second),
        ]));
        meta.push((Some(i), 2));
    }
    let names = ["Today", "Applications", "Archived"];
    let searching = matches!(app.mode, Mode::Search);
    let bottom = if searching || !app.query.is_empty() {
        Line::from(vec![
            keycap(&pal, "/"),
            Span::styled(
                format!(" {}{}", app.query, if searching { "▏" } else { "" }),
                pal.accent(),
            ),
        ])
    } else {
        hints(&pal, &[("/", "search")])
    };
    let block = frame(
        &pal,
        &format!("{} · {}", names[app.tab], app.rows.len()),
        Some(bottom),
    );
    if items.is_empty() {
        let inner = block.inner(area);
        f.render_widget(block, area);
        // A fixed-width card centred in the pane keeps the key rows aligned with each other.
        let w = inner.width.min(44);
        let card = Rect::new(inner.x + (inner.width - w) / 2, inner.y, w, inner.height);
        f.render_widget(
            Paragraph::new(empty_state(app)).wrap(Wrap { trim: false }),
            card,
        );
        return;
    }
    let mut state = ListState::default().with_selected(Some(shown_selected));
    f.render_stateful_widget(
        List::new(items)
            .block(block)
            .highlight_style(pal.selected())
            .highlight_symbol(""),
        area,
        &mut state,
    );
    let mut hit = app.hit.borrow_mut();
    hit.list = Block::bordered().inner(area);
    hit.offset = state.offset();
    hit.items = meta;
}

/// What the list shows when there is nothing to list.
fn empty_state(app: &App) -> Vec<Line<'static>> {
    let pal = app.pal;
    if !app.query.is_empty() {
        return vec![
            Line::from(""),
            Line::from(format!("No matches for “{}”", app.query)).alignment(Alignment::Center),
            Line::from(Span::styled("Esc clears the search", pal.dim()))
                .alignment(Alignment::Center),
        ];
    }
    if app.tab == 2 {
        return vec![
            Line::from(""),
            Line::from("Nothing archived").alignment(Alignment::Center),
            Line::from(Span::styled(
                "Press z on an application to archive it",
                pal.dim(),
            ))
            .alignment(Alignment::Center),
        ];
    }
    let row = |k: &str, text: &str| {
        Line::from(vec![
            Span::styled(format!("  {k}"), pal.accent().bold()),
            Span::raw("   "),
            Span::raw(text.to_string()),
        ])
    };
    vec![
        Line::from(""),
        Line::from(Span::styled("◆ Welcome to Jobi", pal.accent().bold()))
            .alignment(Alignment::Center),
        Line::from(Span::styled(
            "Your next career move, one action at a time.",
            pal.dim(),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
        row("a", "Add your first application"),
        row("L", "Load sample data to explore"),
        row("?", "Learn the keys"),
        Line::from(""),
        Line::from(Span::styled(
            "Everything stays on this computer.",
            pal.dim(),
        ))
        .alignment(Alignment::Center),
    ]
}

fn details_pane(f: &mut Frame, app: &App, area: Rect) {
    let pal = app.pal;
    app.hit.borrow_mut().details = area;
    let inner = Block::bordered().inner(area);
    let now = Local::now().date_naive();
    // The frame pads the text by one column on each side.
    let width = inner.width.saturating_sub(2);
    let lines: Vec<Line> = match app.rows.get(app.selected) {
        Some(a) => {
            let mut l = vec![
                Line::from(vec![
                    Span::styled(a.company.clone(), Style::new().bold()),
                    if a.archived {
                        Span::styled("  archived", pal.dim())
                    } else {
                        Span::raw("")
                    },
                ]),
                Line::from(Span::styled(a.role.clone(), pal.dim())),
                Line::from(""),
                pipeline(&pal, &app.stages, &a.stage),
                Line::from(""),
                rule(&pal, "NEXT ACTION", width),
            ];
            if a.action.is_empty() {
                l.push(Line::from(Span::styled(
                    "None yet. Press e to set one.",
                    pal.dim(),
                )));
            } else {
                l.push(Line::from(Span::styled(
                    a.action.clone(),
                    Style::new().bold(),
                )));
                l.push(Line::from(match relative(&a.due, now) {
                    Some((text, days)) => {
                        let style = if days < 0 {
                            pal.danger()
                        } else if days == 0 {
                            pal.warn()
                        } else {
                            Style::new()
                        };
                        vec![
                            Span::styled("due ", pal.dim()),
                            Span::styled(text, style),
                            Span::styled(format!(" · {}", a.due), pal.dim()),
                        ]
                    }
                    None => vec![Span::styled("no due date", pal.dim())],
                }));
            }
            if app.stale.contains(&a.id) {
                l.push(Line::from(Span::styled(
                    format!(
                        "⏳ No activity for {}+ days. Time to follow up?",
                        app.config.stale_days
                    ),
                    pal.warn(),
                )));
            }
            if let Some(o) = &app.offer {
                l.push(Line::from(""));
                l.push(rule(&pal, "OFFER", width));
                l.push(Line::from(Span::styled(o.summary(), pal.ok())));
            }
            if !app.tasks.is_empty() {
                let open = app.tasks.iter().filter(|t| !t.done).count();
                l.push(Line::from(""));
                l.push(rule(
                    &pal,
                    &format!("TASKS · {open} open of {}", app.tasks.len()),
                    width,
                ));
                for t in &app.tasks {
                    let due = if t.due.is_empty() {
                        String::new()
                    } else {
                        format!("  ({})", t.due)
                    };
                    l.push(Line::from(if t.done {
                        Span::styled(format!("☑ {}{due}", t.title), pal.dim())
                    } else {
                        Span::raw(format!("☐ {}{due}", t.title))
                    }));
                }
            }
            if !app.contacts.is_empty() {
                l.push(Line::from(""));
                l.push(rule(&pal, "CONTACTS", width));
                for c in &app.contacts {
                    let extra: Vec<&str> = [&c.role, &c.email, &c.phone]
                        .into_iter()
                        .filter(|s| !s.is_empty())
                        .map(String::as_str)
                        .collect();
                    l.push(Line::from(vec![
                        Span::styled(c.name.clone(), Style::new().bold()),
                        Span::styled(format!("  {}", extra.join(" · ")), pal.dim()),
                    ]));
                }
            }
            l.push(Line::from(""));
            l.push(rule(&pal, "ACTIVITY", width));
            for h in &app.history {
                // "2026-11-17 06:32:56 UTC  message"
                let (when, what) = h.split_once("  ").unwrap_or(("", h));
                l.push(Line::from(vec![
                    Span::styled(
                        format!("{} ", when.chars().take(16).collect::<String>()),
                        pal.dim(),
                    ),
                    Span::raw(what.to_string()),
                ]));
            }
            l
        }
        None => vec![
            Line::from(""),
            Line::from(Span::styled("Select an application", pal.dim()))
                .alignment(Alignment::Center),
            Line::from(""),
            Line::from(Span::styled("Tips", pal.accent().bold())).alignment(Alignment::Center),
            Line::from("Search with stage:interview or due:overdue").alignment(Alignment::Center),
            Line::from("Dates accept today, tomorrow, +3d or fri").alignment(Alignment::Center),
            Line::from("Press ? for every key").alignment(Alignment::Center),
        ],
    };
    let details = Paragraph::new(lines).wrap(Wrap { trim: false });
    let total = u16::try_from(details.line_count(width)).unwrap_or(u16::MAX);
    let max_scroll = total.saturating_sub(inner.height);
    app.max_scroll.set(max_scroll);
    let mut block = frame(
        &pal,
        "Details",
        Some(hints(
            &pal,
            &[
                ("p", "post"),
                ("i", "interview"),
                ("d", "doc"),
                ("o", "offer"),
                ("c", "contact"),
                ("T", "task"),
                ("v", "review"),
            ],
        )),
    );
    if max_scroll > 0 {
        block =
            block.title(Line::from(Span::styled(" PgUp/PgDn scroll ", pal.dim())).right_aligned());
    }
    f.render_widget(
        details.scroll((app.scroll.min(max_scroll), 0)).block(block),
        area,
    );
}

fn footer(f: &mut Frame, app: &App, area: Rect) {
    let pal = app.pal;
    let rows = Layout::vertical([Constraint::Length(2), Constraint::Length(1)]).split(area);
    let keys: Vec<(&str, &str)> = match app.mode {
        Mode::Search => vec![("Enter", "apply"), ("Esc", "clear")],
        _ => vec![
            ("a", "add"),
            ("e", "edit"),
            ("n", "note"),
            ("x", "done"),
            ("s", "stage"),
            ("u", "undo"),
            ("z", if app.tab == 2 { "restore" } else { "archive" }),
            ("?", "help"),
            ("q", "quit"),
        ],
    };
    f.render_widget(
        Paragraph::new(hints(&pal, &keys)).wrap(Wrap { trim: true }),
        rows[0],
    );
    let style = if app.status.starts_with("Error") {
        pal.danger()
    } else if app.status.starts_with("Nothing") || app.status.starts_with("Local storage") {
        pal.dim()
    } else {
        pal.ok()
    };
    f.render_widget(
        Paragraph::new(Span::styled(format!(" {}", app.status), style)),
        rows[1],
    );
}

/// "● Wishlist  ● Applied  ◉ Screening  ○ Interview ..." with the current stage highlighted.
fn pipeline(pal: &super::theme::Palette, stages: &Stages, stage: &str) -> Line<'static> {
    let main = stages.pipeline();
    let Some(at) = main.iter().position(|s| *s == stage) else {
        let note = if stages.is_closed(stage) {
            "  closed"
        } else {
            "  not one of your configured stages"
        };
        return Line::from(vec![pal.chip(stages, stage), Span::styled(note, pal.dim())]);
    };
    let mut spans = vec![];
    for (i, name) in main.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(match i.cmp(&at) {
            std::cmp::Ordering::Less => Span::styled(format!("● {name}"), pal.dim()),
            std::cmp::Ordering::Equal => {
                Span::styled(format!("◉ {name}"), pal.stage_in(stages, stage).bold())
            }
            std::cmp::Ordering::Greater => Span::styled(format!("○ {name}"), pal.dim()),
        });
    }
    Line::from(spans)
}
