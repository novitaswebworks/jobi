//! Keyboard and mouse handling for every mode.
use super::*;

pub(super) fn handle(app: &mut App, store: &mut Store, key: KeyEvent) -> Result<bool> {
    match &mut app.mode {
        Mode::Form(form) => match key.code {
            KeyCode::Esc => app.mode = Mode::Browse,
            KeyCode::Tab | KeyCode::Down => form.field = (form.field + 1) % 5,
            KeyCode::BackTab | KeyCode::Up => form.field = (form.field + 4) % 5,
            KeyCode::Left | KeyCode::Right if form.field == 2 => {
                let all = &app.stages.all;
                let i = app.stages.position(&form.values[2]).unwrap_or(0);
                let next = if key.code == KeyCode::Right {
                    (i + 1) % all.len()
                } else {
                    (i + all.len() - 1) % all.len()
                };
                form.values[2] = all[next].clone();
            }
            KeyCode::Enter => {
                let v = &form.values;
                let today = chrono::Local::now().date_naive();
                let saved = resolve_due(&v[4], today).and_then(|due| {
                    let (action, due) = if form.id.is_none() {
                        app.config
                            .with_follow_up(&app.stages, &v[2], &v[3], &due, today)
                    } else {
                        (v[3].clone(), due)
                    };
                    store
                        .save(form.id, &v[0], &v[1], &v[2], &action, &due)
                        .map(|_| (action != v[3]).then_some(due))
                });
                match saved {
                    Ok(auto_due) => {
                        app.scroll = 0;
                        app.mode = Mode::Browse;
                        app.tab = 1;
                        app.query.clear();
                        app.status = match auto_due {
                            Some(due) => format!("Application saved · follow-up set for {due}"),
                            None => "Application saved".into(),
                        };
                    }
                    Err(e) => form.error = e.to_string(),
                }
            }
            KeyCode::Backspace if form.field != 2 => {
                form.values[form.field].pop();
            }
            KeyCode::Char(c)
                if form.field != 2 && !key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                form.values[form.field].push(c)
            }
            _ => {}
        },
        Mode::Note(text) => match key.code {
            KeyCode::Esc => app.mode = Mode::Browse,
            KeyCode::Enter => {
                if let Some(a) = app.rows.get(app.selected) {
                    store.note(a.id, text)?;
                    app.status = "Note saved".into();
                    app.scroll = 0;
                }
                app.mode = Mode::Browse;
            }
            KeyCode::Backspace => {
                text.pop();
            }
            KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.editor = true
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => text.push(c),
            _ => {}
        },
        Mode::Search => match key.code {
            KeyCode::Esc => {
                app.query.clear();
                app.mode = Mode::Browse;
            }
            KeyCode::Enter => app.mode = Mode::Browse,
            KeyCode::Backspace => {
                app.query.pop();
                app.selected = 0;
            }
            KeyCode::Char(c) => {
                app.query.push(c);
                app.selected = 0;
            }
            _ => {}
        },
        Mode::Prepare(kind, fields, field, error, rec) => match key.code {
            KeyCode::Esc => app.mode = Mode::Browse,
            KeyCode::Tab | KeyCode::Down => *field = (*field + 1) % fields.len(),
            KeyCode::BackTab | KeyCode::Up => *field = (*field + fields.len() - 1) % fields.len(),
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::CONTROL) => {
                fields[*field].push('\n')
            }
            KeyCode::Enter => {
                let id = app.rows.get(app.selected).map(|a| a.id);
                if let Some(id) = id {
                    let res = match *kind {
                        0 => store.save_posting(id, &fields[0], &fields[1]),
                        1 => store.add_interview(
                            id, &fields[0], &fields[1], &fields[2], &fields[3], &fields[4],
                        ),
                        3 => store.update_interview(&Interview {
                            id: rec.unwrap_or_default(),
                            starts_at: fields[0].clone(),
                            timezone: fields[1].clone(),
                            kind: fields[2].clone(),
                            url: fields[3].clone(),
                            notes: fields[4].clone(),
                        }),
                        4 => save_offer_form(store, id, fields),
                        5 => (|| {
                            let c = Contact {
                                id: 0,
                                name: fields[0].clone(),
                                role: fields[1].clone(),
                                email: fields[2].clone(),
                                phone: fields[3].clone(),
                                last_contacted: resolve_due(&fields[4], today())?,
                                notes: fields[5].clone(),
                            };
                            store.add_contact(id, &c)
                        })(),
                        6 => resolve_due(&fields[1], today())
                            .and_then(|due| store.add_task(id, &fields[0], &due)),
                        _ if fields[2].trim().to_lowercase().starts_with('y') => {
                            store.add_document_copy(id, &fields[0], &fields[1])
                        }
                        _ => store.add_document(id, &fields[0], &fields[1]),
                    };
                    match res {
                        Ok(()) => {
                            app.mode = Mode::Browse;
                            app.status = "Saved".into();
                        }
                        Err(e) => *error = e.to_string(),
                    }
                }
            }
            KeyCode::Backspace => {
                fields[*field].pop();
            }
            KeyCode::Char('e')
                if key.modifiers.contains(KeyModifiers::CONTROL) && multiline(*kind, *field) =>
            {
                app.editor = true
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                fields[*field].push(c)
            }
            _ => {}
        },
        Mode::Stats(_) => app.mode = Mode::Browse,
        Mode::Inspect(row, sel) => {
            let Some(aid) = app.rows.get(*row).map(|a| a.id) else {
                app.mode = Mode::Browse;
                return Ok(false);
            };
            let items = inspect_items(store, aid)?;
            match key.code {
                KeyCode::Esc | KeyCode::Char('q' | 'v') => app.mode = Mode::Browse,
                KeyCode::Down | KeyCode::Char('j') => {
                    *sel = (*sel + 1).min(items.len().saturating_sub(1))
                }
                KeyCode::Up | KeyCode::Char('k') => *sel = sel.saturating_sub(1),
                KeyCode::Enter | KeyCode::Char(' ') => match items.get(*sel) {
                    Some(Item::Task(t)) => store.set_task_done(t.id, !t.done)?,
                    _ if key.code == KeyCode::Enter => app.mode = Mode::Browse,
                    _ => {}
                },
                KeyCode::Char('e') => {
                    if let Some(Item::Interview(i)) = items.get(*sel) {
                        app.mode = Mode::Prepare(
                            3,
                            vec![
                                i.starts_at.clone(),
                                i.timezone.clone(),
                                i.kind.clone(),
                                i.url.clone(),
                                i.notes.clone(),
                            ],
                            0,
                            String::new(),
                            Some(i.id),
                        );
                    }
                }
                KeyCode::Char('r') => {
                    app.mode = match items.get(*sel) {
                        Some(Item::Interview(i)) => Mode::Remove(
                            RemoveKind::Interview,
                            i.id,
                            format!("{} {}", i.starts_at, i.kind),
                        ),
                        Some(Item::Document(d)) => {
                            Mode::Remove(RemoveKind::Document, d.id, d.label.clone())
                        }
                        Some(Item::Task(t)) => {
                            Mode::Remove(RemoveKind::Task, t.id, t.title.clone())
                        }
                        Some(Item::Contact(c)) => {
                            Mode::Remove(RemoveKind::Contact, c.id, c.name.clone())
                        }
                        None => return Ok(false),
                    };
                }
                _ => {}
            }
        }
        Mode::Remove(kind, id, _) => match key.code {
            KeyCode::Char('y') => {
                let res = match kind {
                    RemoveKind::Interview => store.delete_interview(*id),
                    RemoveKind::Document => store.delete_document(*id),
                    RemoveKind::Task => store.delete_task(*id),
                    RemoveKind::Contact => store.delete_contact(*id),
                };
                app.mode = Mode::Browse;
                res?;
                app.status = "Removed".into();
            }
            KeyCode::Esc | KeyCode::Char('n') => app.mode = Mode::Browse,
            _ => {}
        },
        Mode::Help(scroll) => match key.code {
            KeyCode::Down | KeyCode::Char('j') => *scroll = (*scroll + 1).min(app.help_max.get()),
            KeyCode::Up | KeyCode::Char('k') => *scroll = scroll.saturating_sub(1),
            KeyCode::PageDown => *scroll = (*scroll + 8).min(app.help_max.get()),
            KeyCode::PageUp => *scroll = scroll.saturating_sub(8),
            _ => app.mode = Mode::Browse,
        },
        Mode::Archive(id, state) => match key.code {
            KeyCode::Char('y') => {
                store.archive(*id, *state)?;
                app.undo = Some(Undo::Archive(*id, !*state));
                app.mode = Mode::Browse;
                app.status = "Archive status updated".into();
            }
            KeyCode::Esc | KeyCode::Char('n') => app.mode = Mode::Browse,
            _ => {}
        },
        Mode::Browse => match key.code {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Tab => {
                app.tab = (app.tab + 1) % 3;
                app.selected = 0;
                app.scroll = 0;
            }
            KeyCode::Char('1' | '2' | '3') => {
                if let KeyCode::Char(c) = key.code {
                    app.tab = (c as u8 - b'1') as usize;
                }
                app.selected = 0;
                app.scroll = 0;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.selected = (app.selected + 1).min(app.rows.len().saturating_sub(1));
                app.scroll = 0;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.selected = app.selected.saturating_sub(1);
                app.scroll = 0;
            }
            KeyCode::PageDown => app.scroll = (app.scroll + 5).min(app.max_scroll.get()),
            KeyCode::PageUp => app.scroll = app.scroll.min(app.max_scroll.get()).saturating_sub(5),
            KeyCode::Char('/') => app.mode = Mode::Search,
            KeyCode::Esc => app.query.clear(),
            KeyCode::Char('a') => app.mode = Mode::Form(Form::new(None, &app.config.default_stage)),
            KeyCode::Char('e') => {
                if let Some(a) = app.rows.get(app.selected) {
                    app.mode = Mode::Form(Form::new(Some(a), &app.config.default_stage));
                }
            }
            KeyCode::Char('n') if !app.rows.is_empty() => {
                app.mode = Mode::Note(String::new());
            }
            KeyCode::Char('x') => {
                if let Some(a) = app.rows.get(app.selected) {
                    let prior = Undo::Action(a.id, a.action.clone(), a.due.clone());
                    store.complete(a.id)?;
                    app.undo = Some(prior);
                    app.status = "Action completed · press e to set the next one".into();
                    app.scroll = 0;
                }
            }
            KeyCode::Char('z') => {
                if let Some(a) = app.rows.get(app.selected) {
                    app.mode = Mode::Archive(a.id, !a.archived);
                }
            }
            KeyCode::Char('p') if !app.rows.is_empty() => {
                let (url, desc) = store.posting(app.rows[app.selected].id).unwrap_or_default();
                app.mode = Mode::Prepare(0, vec![url, desc], 0, String::new(), None);
            }
            KeyCode::Char('i') if !app.rows.is_empty() => {
                app.mode = Mode::Prepare(
                    1,
                    vec![
                        String::new(),
                        app.config.timezone(),
                        String::new(),
                        String::new(),
                        String::new(),
                    ],
                    0,
                    String::new(),
                    None,
                )
            }
            KeyCode::Char('d') if !app.rows.is_empty() => {
                app.mode = Mode::Prepare(
                    2,
                    vec![String::new(), String::new(), "no".into()],
                    0,
                    String::new(),
                    None,
                )
            }
            KeyCode::Char('v') if !app.rows.is_empty() => app.mode = Mode::Inspect(app.selected, 0),
            KeyCode::Char('o') if !app.rows.is_empty() => {
                let f = match store.offer(app.rows[app.selected].id)? {
                    Some(o) => vec![
                        o.base.to_string(),
                        o.currency,
                        o.bonus.to_string(),
                        o.equity,
                        o.deadline,
                        o.notes,
                    ],
                    None => vec![String::new(); 6],
                };
                app.mode = Mode::Prepare(4, f, 0, String::new(), None);
            }
            KeyCode::Char('c') if !app.rows.is_empty() => {
                app.mode = Mode::Prepare(5, vec![String::new(); 6], 0, String::new(), None)
            }
            KeyCode::Char('T') if !app.rows.is_empty() => {
                app.mode = Mode::Prepare(6, vec![String::new(); 2], 0, String::new(), None)
            }
            KeyCode::Char(c @ ('s' | 'S')) => {
                if let Some(a) = app.rows.get(app.selected) {
                    let Some(i) = app.stages.position(&a.stage) else {
                        app.status = format!(
                            "'{}' is not one of your configured stages: press e to choose one",
                            a.stage
                        );
                        return Ok(false);
                    };
                    let next = if c == 's' { i + 1 } else { i.wrapping_sub(1) };
                    match app.stages.all.get(next) {
                        Some(stage) => {
                            let stage = stage.clone();
                            let old = store.set_stage(a.id, &stage)?;
                            app.undo = Some(Undo::Stage(a.id, old));
                            app.status = format!("Stage → {stage} · u to undo");
                        }
                        None => app.status = "No further stage in that direction".into(),
                    }
                }
            }
            KeyCode::Char('u') => match app.undo.take() {
                Some(Undo::Stage(id, stage)) => {
                    store.set_stage(id, &stage)?;
                    app.status = format!("Undone · stage back to {stage}");
                }
                Some(Undo::Action(id, action, due)) => {
                    store.reopen_action(id, &action, &due)?;
                    app.status = "Undone · action reopened".into();
                }
                Some(Undo::Archive(id, to)) => {
                    store.archive(id, to)?;
                    app.status = "Undone · archive status restored".into();
                }
                None => app.status = "Nothing to undo".into(),
            },
            KeyCode::Char('t') => {
                app.mode = Mode::Stats(crate::stats::compute(store)?);
            }
            KeyCode::Char('L') => {
                let n = crate::stats::load_sample(store, Local::now().date_naive())?;
                app.tab = 0;
                app.status = format!("Loaded {n} sample applications");
            }
            KeyCode::Char('?') => app.mode = Mode::Help(0),
            _ => {}
        },
    }
    Ok(false)
}
fn today() -> chrono::NaiveDate {
    Local::now().date_naive()
}
/// Offer form: amounts accept separators ("120,000"); base pay is required.
fn save_offer_form(store: &mut Store, id: i64, f: &[String]) -> Result<()> {
    let amount = |s: &str, required: bool| -> Result<i64> {
        let t: String = s
            .chars()
            .filter(|c| !matches!(c, ',' | '_' | ' '))
            .collect();
        if t.is_empty() {
            if required {
                bail!("Base pay is required");
            }
            return Ok(0);
        }
        t.parse()
            .map_err(|_| anyhow::anyhow!("'{s}' is not a whole number"))
    };
    store.save_offer(&Offer {
        application_id: id,
        base: amount(&f[0], true)?,
        currency: f[1].clone(),
        bonus: amount(&f[2], false)?,
        equity: f[3].clone(),
        deadline: resolve_due(&f[4], today())?,
        notes: f[5].clone(),
    })
}

/// Wheel scrolls the pane under the pointer; a left click selects a list row.
pub(super) fn handle_mouse(app: &mut App, m: MouseEvent) {
    if !matches!(app.mode, Mode::Browse) {
        return;
    }
    let (x, y) = (m.column, m.row);
    let inside = |r: Rect| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height;
    let (list, details) = {
        let h = app.hit.borrow();
        (h.list, h.details)
    };
    match m.kind {
        MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
            let down = m.kind == MouseEventKind::ScrollDown;
            if inside(details) {
                app.scroll = if down {
                    (app.scroll + 3).min(app.max_scroll.get())
                } else {
                    app.scroll.min(app.max_scroll.get()).saturating_sub(3)
                };
            } else if inside(list) {
                app.selected = if down {
                    (app.selected + 1).min(app.rows.len().saturating_sub(1))
                } else {
                    app.selected.saturating_sub(1)
                };
                app.scroll = 0;
            }
        }
        MouseEventKind::Down(MouseButton::Left) if inside(list) => {
            let h = app.hit.borrow();
            let mut top = list.y;
            let clicked = h.items.iter().skip(h.offset).find_map(|(row, height)| {
                let hit = y >= top && y < top + height;
                top += height;
                hit.then_some(*row)
            });
            drop(h);
            if let Some(Some(row)) = clicked {
                app.selected = row;
                app.scroll = 0;
            }
        }
        _ => {}
    }
}
