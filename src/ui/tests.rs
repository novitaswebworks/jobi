use super::*;
#[test]
fn actions_are_ordered_and_empty_actions_excluded() {
    let make = |id, action: &str, due: &str| Application {
        id,
        company: "A".into(),
        role: "R".into(),
        stage: "Applied".into(),
        action: action.into(),
        due: due.into(),
        archived: false,
    };
    let mut rows = vec![
        make(1, "Call", ""),
        make(2, "Follow up", "2026-11-20"),
        make(3, "", ""),
        make(4, "Prepare", "2026-11-18"),
    ];
    order_actions(&mut rows);
    assert_eq!(rows.iter().map(|a| a.id).collect::<Vec<_>>(), vec![4, 2, 1]);
}
#[test]
fn layouts_and_form_render_at_supported_sizes() -> Result<()> {
    use ratatui::backend::TestBackend;
    for (width, height) in [(110, 32), (50, 18), (30, 10)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height))?;
        let store = Store(rusqlite::Connection::open_in_memory()?, Stages::default());
        let mut app = App::new(&Config::default(), Mode::Browse);
        terminal.draw(|f| draw(f, &app, &store))?;
        let content: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(content.contains(if width < 50 { "Resize" } else { "JOBI" }));
        if width >= 50 {
            app.mode = Mode::Form(Form::new(None, "Applied"));
            terminal.draw(|f| draw(f, &app, &store))?;
            let content: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect();
            assert!(content.contains("Company *"));
            assert!(content.contains("save"));
        }
    }
    Ok(())
}
#[cfg(unix)]
#[test]
fn external_editor_roundtrip_and_failure() {
    // Only this test touches VISUAL, so setting it here cannot race other tests.
    unsafe { std::env::set_var("VISUAL", "printf 'a\\nb\\n' >") };
    assert_eq!(edit_externally("old").unwrap(), "a\nb");
    unsafe { std::env::set_var("VISUAL", "false #") };
    assert!(edit_externally("old").is_err());
    assert!(multiline(0, 1) && multiline(3, 4) && !multiline(0, 0) && !multiline(2, 1));
}
#[test]
fn inspect_remove_help_render_and_details_scroll_is_clamped() -> Result<()> {
    use ratatui::backend::TestBackend;
    let mut store = Store::initialize(rusqlite::Connection::open_in_memory()?)?;
    let id = store.save(None, "Acme", "Eng", "Applied", "", "")?;
    for n in 0..30 {
        store.note(id, &format!("note {n}"))?;
    }
    store.add_interview(
        id,
        "2026-11-20T14:00:00+05:30",
        "Asia/Kolkata",
        "phone",
        "",
        "a\nb",
    )?;
    store
        .add_document(id, "Gone", "/definitely/missing/file.pdf")
        .ok();
    let mut app = App::new(&Config::default(), Mode::Browse);
    app.tab = 1;
    app.scroll = 9999;
    app.rows = store.list(false)?;
    app.history = store.history(id)?;
    for (w, h) in [(110, 32), (50, 18)] {
        let mut t = Terminal::new(TestBackend::new(w, h))?;
        app.mode = Mode::Browse;
        t.draw(|f| draw(f, &app, &store))?;
        let max = app.max_scroll.get();
        assert!(
            max > 0,
            "30 history lines must overflow the pane at {w}x{h}"
        );
        // Handler paging never goes past the measured maximum.
        let key = KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE);
        for _ in 0..50 {
            handle(&mut app, &mut store, key)?;
        }
        assert_eq!(app.scroll, max);
        for mode in [
            Mode::Inspect(0, 0),
            Mode::Remove(RemoveKind::Interview, 1, "x".into()),
            Mode::Help(0),
        ] {
            app.mode = mode;
            t.draw(|f| draw(f, &app, &store))?;
        }
    }
    // Inspect: remove the selected interview through the keyboard flow.
    app.mode = Mode::Inspect(0, 0);
    handle(
        &mut app,
        &mut store,
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
    )?;
    assert!(matches!(
        app.mode,
        Mode::Remove(RemoveKind::Interview, _, _)
    ));
    handle(
        &mut app,
        &mut store,
        KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
    )?;
    assert!(store.interviews(id)?.is_empty());
    Ok(())
}
fn text(t: &Terminal<ratatui::backend::TestBackend>) -> String {
    t.backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect()
}
fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}
#[test]
fn today_groups_header_counts_stage_keys_and_undo() -> Result<()> {
    use ratatui::backend::TestBackend;
    let mut store = Store::initialize(rusqlite::Connection::open_in_memory()?)?;
    let today = Local::now().date_naive();
    let fmt = |d: i64| {
        (today + chrono::Duration::days(d))
            .format("%Y-%m-%d")
            .to_string()
    };
    store.save(None, "Late", "R", "Applied", "Call", &fmt(-2))?;
    store.save(None, "Now", "R", "Screening", "Call", &fmt(0))?;
    store.save(None, "Soon", "R", "Offer", "Call", &fmt(3))?;
    store.save(None, "Far", "R", "Rejected", "Call", &fmt(40))?;
    for theme in ["dark", "light", "mono"] {
        let config = Config::parse(&format!("theme = \"{theme}\""))?;
        let mut app = App::new(&config, Mode::Browse);
        app.refresh(&store)?;
        assert_eq!((app.overdue, app.due_today), (1, 1));
        let mut t = Terminal::new(TestBackend::new(110, 34))?;
        t.draw(|f| draw(f, &app, &store))?;
        let out = text(&t);
        for want in [
            "1 overdue",
            "1 due today",
            "Overdue",
            "This week",
            "Later",
            "◉ Applied",
        ] {
            assert!(out.contains(want), "{theme}: missing {want}");
            // Section rules must fill their own row instead of wrapping onto the next one.
            assert!(out.contains("NEXT ACTION ─"), "{theme}: rule wrapped");
        }
        if theme == "mono" {
            // No colour anywhere: only modifiers.
            assert!(
                t.backend()
                    .buffer()
                    .content
                    .iter()
                    .all(|c| c.fg == Color::Reset && c.bg == Color::Reset
                        || c.modifier.contains(Modifier::REVERSED))
            );
        }
    }
    // s moves the selected application forward, u puts it back.
    let config = Config::default();
    let mut app = App::new(&config, Mode::Browse);
    app.refresh(&store)?;
    let id = app.rows[0].id;
    handle(&mut app, &mut store, key('s'))?;
    assert_eq!(
        store
            .list(false)?
            .iter()
            .find(|a| a.id == id)
            .unwrap()
            .stage,
        "Screening"
    );
    handle(&mut app, &mut store, key('u'))?;
    assert_eq!(
        store
            .list(false)?
            .iter()
            .find(|a| a.id == id)
            .unwrap()
            .stage,
        "Applied"
    );
    handle(&mut app, &mut store, key('x'))?;
    assert!(
        store
            .list(false)?
            .iter()
            .find(|a| a.id == id)
            .unwrap()
            .action
            .is_empty()
    );
    handle(&mut app, &mut store, key('u'))?;
    assert_eq!(
        store
            .list(false)?
            .iter()
            .find(|a| a.id == id)
            .unwrap()
            .action,
        "Call"
    );
    handle(&mut app, &mut store, key('u'))?;
    assert_eq!(app.status, "Nothing to undo");
    Ok(())
}
#[test]
fn stats_popup_and_sample_key_render() -> Result<()> {
    use ratatui::backend::TestBackend;
    let mut store = Store::initialize(rusqlite::Connection::open_in_memory()?)?;
    let config = Config::default();
    let mut app = App::new(&config, Mode::Browse);
    app.refresh(&store)?;
    let mut t = Terminal::new(TestBackend::new(100, 30))?;
    t.draw(|f| draw(f, &app, &store))?;
    assert!(text(&t).contains("Load sample data"));
    handle(&mut app, &mut store, key('L'))?;
    app.refresh(&store)?;
    assert_eq!(app.rows.len(), 3);
    handle(&mut app, &mut store, key('t'))?;
    t.draw(|f| draw(f, &app, &store))?;
    let out = text(&t);
    assert!(out.contains("Pipeline statistics") && out.contains("Applications: 4"));
    // The sample loader refuses a database that already has data.
    app.mode = Mode::Browse;
    assert!(handle(&mut app, &mut store, key('L')).is_err());
    Ok(())
}

fn type_text(app: &mut App, store: &mut Store, text: &str) {
    for c in text.chars() {
        handle(app, store, key(c)).unwrap();
    }
}
fn press(app: &mut App, store: &mut Store, code: KeyCode) {
    handle(app, store, KeyEvent::new(code, KeyModifiers::NONE)).unwrap();
}

#[test]
fn search_filters() {
    let a = |stage: &str, due: &str| Application {
        id: 1,
        company: "Acme".into(),
        role: "Rust Dev".into(),
        stage: stage.into(),
        action: "Call".into(),
        due: due.into(),
        archived: false,
    };
    let m = |app: &Application, q: &str| matches_query(app, q, "2026-11-17", "2026-11-24");
    assert!(m(&a("Offer", ""), ""));
    assert!(m(&a("Offer", ""), "acme rust"));
    assert!(!m(&a("Offer", ""), "acme python"));
    assert!(m(&a("Interview", ""), "stage:int"));
    assert!(!m(&a("Interview", ""), "stage:off"));
    assert!(m(&a("Applied", "2026-11-16"), "due:overdue"));
    assert!(m(&a("Applied", "2026-11-17"), "due:today"));
    assert!(m(&a("Applied", "2026-11-20"), "due:week acme"));
    assert!(!m(&a("Applied", "2026-12-20"), "due:week"));
    assert!(m(&a("Applied", ""), "due:none"));
    assert!(!m(&a("Applied", ""), "due:bogus"));
}

#[test]
fn forms_for_tasks_contacts_offers_and_follow_up() -> Result<()> {
    let mut store = Store::initialize(rusqlite::Connection::open_in_memory()?)?;
    let mut app = App::new(&Config::default(), Mode::Browse);
    // New "Applied" application without an action gets the automatic follow-up.
    press(&mut app, &mut store, KeyCode::Char('a'));
    type_text(&mut app, &mut store, "Acme");
    press(&mut app, &mut store, KeyCode::Tab);
    type_text(&mut app, &mut store, "Eng");
    press(&mut app, &mut store, KeyCode::Enter);
    assert!(app.status.contains("follow-up set for"), "{}", app.status);
    app.refresh(&store)?;
    let a = &store.list(false)?[0];
    assert_eq!(a.action, "Follow up");
    let id = a.id;
    app.tab = 1;
    app.refresh(&store)?;

    // Task with a friendly date.
    press(&mut app, &mut store, KeyCode::Char('T'));
    type_text(&mut app, &mut store, "Send thank-you");
    press(&mut app, &mut store, KeyCode::Tab);
    type_text(&mut app, &mut store, "+2d");
    press(&mut app, &mut store, KeyCode::Enter);
    assert!(matches!(app.mode, Mode::Browse), "{:?}", app.status);
    assert_eq!(store.tasks(id)?[0].title, "Send thank-you");
    assert!(!store.tasks(id)?[0].due.is_empty());

    // Contact, with validation error staying in the form.
    press(&mut app, &mut store, KeyCode::Char('c'));
    type_text(&mut app, &mut store, "Jane");
    press(&mut app, &mut store, KeyCode::Tab);
    press(&mut app, &mut store, KeyCode::Tab);
    type_text(&mut app, &mut store, "not-an-email");
    press(&mut app, &mut store, KeyCode::Enter);
    assert!(matches!(&app.mode, Mode::Prepare(_, _, _, e, _) if e.contains("email")));
    if let Mode::Prepare(_, f, _, _, _) = &mut app.mode {
        f[2] = "jane@x.com".into();
    }
    press(&mut app, &mut store, KeyCode::Enter);
    assert_eq!(store.contacts(id)?[0].email, "jane@x.com");

    // Offer with separators, then reopened pre-filled.
    press(&mut app, &mut store, KeyCode::Char('o'));
    type_text(&mut app, &mut store, "120,000");
    press(&mut app, &mut store, KeyCode::Tab);
    type_text(&mut app, &mut store, "usd");
    press(&mut app, &mut store, KeyCode::Enter);
    assert_eq!(store.offer(id)?.unwrap().base, 120_000);
    press(&mut app, &mut store, KeyCode::Char('o'));
    assert!(matches!(&app.mode, Mode::Prepare(4, f, ..) if f[0] == "120000" && f[1] == "USD"));
    press(&mut app, &mut store, KeyCode::Esc);

    // Details view: Space toggles the task; items are removable.
    app.refresh(&store)?;
    press(&mut app, &mut store, KeyCode::Char('v'));
    // No interviews or documents, so the first selectable item is the task.
    type_text(&mut app, &mut store, " ");
    assert!(store.tasks(id)?[0].done);
    type_text(&mut app, &mut store, "r");
    assert!(matches!(app.mode, Mode::Remove(RemoveKind::Task, _, _)));
    type_text(&mut app, &mut store, "y");
    assert!(store.tasks(id)?.is_empty());
    Ok(())
}

#[test]
fn mouse_click_selects_and_wheel_scrolls_and_note_editor_flag() -> Result<()> {
    use ratatui::backend::TestBackend;
    let mut store = Store::initialize(rusqlite::Connection::open_in_memory()?)?;
    for c in ["A", "B", "C"] {
        store.save(None, c, "R", "Applied", "Call", "2099-01-01")?;
    }
    let mut app = App::new(&Config::default(), Mode::Browse);
    app.tab = 1;
    app.refresh(&store)?;
    let mut t = Terminal::new(TestBackend::new(110, 34))?;
    t.draw(|f| draw(f, &app, &store))?;
    let list = app.hit.borrow().list;
    let click = |row: u16| MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: list.x + 3,
        row,
        modifiers: KeyModifiers::NONE,
    };
    // Each application takes two lines: the third row is at list.y + 4.
    handle_mouse(&mut app, click(list.y + 4));
    assert_eq!(app.selected, 2);
    handle_mouse(&mut app, click(list.y));
    assert_eq!(app.selected, 0);
    let wheel = |kind| MouseEvent {
        kind,
        column: list.x + 1,
        row: list.y + 1,
        modifiers: KeyModifiers::NONE,
    };
    handle_mouse(&mut app, wheel(MouseEventKind::ScrollDown));
    assert_eq!(app.selected, 1);
    handle_mouse(&mut app, wheel(MouseEventKind::ScrollUp));
    assert_eq!(app.selected, 0);
    // Ctrl+E in the note box asks the run loop to open the editor.
    press(&mut app, &mut store, KeyCode::Char('n'));
    handle(
        &mut app,
        &mut store,
        KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL),
    )?;
    assert!(app.editor);
    Ok(())
}

#[test]
fn custom_stages_drive_the_pipeline_keys_and_legacy_handling() -> Result<()> {
    use ratatui::backend::TestBackend;
    let config = Config::parse(
        "stages = [\"Idea\", \"Sent\", \"Call\", \"Hired\", \"Declined\"]\nclosed_stages = [\"Declined\"]\ndefault_stage = \"Sent\"",
    )?;
    let mut store = Store::initialize(rusqlite::Connection::open_in_memory()?)?;
    // Created under the default stages, before the custom ones were configured.
    let legacy = store.save(None, "Old", "Dev", "Applied", "Call", "2099-01-02")?;
    store.set_stages(config.stages()?);
    let id = store.save(None, "Acme", "Eng", "Sent", "Call", "2099-01-01")?;
    let mut app = App::new(&config, Mode::Browse);
    app.refresh(&store)?;
    let mut t = Terminal::new(TestBackend::new(120, 34))?;
    t.draw(|f| draw(f, &app, &store))?;
    let out = text(&t);
    assert!(
        out.contains("Idea") && out.contains("◉ Sent") && out.contains("Hired"),
        "{out}"
    );
    assert!(!out.contains("Wishlist") && !out.contains("Screening"));
    // s walks the configured list, then stops, and Declined is reachable but not in the bar.
    press(&mut app, &mut store, KeyCode::Char('s'));
    assert_eq!(stage_of(&store, id), "Call");
    // (The run loop refreshes after every key; the test does it by hand.)
    for _ in 0..2 {
        app.refresh(&store)?;
        press(&mut app, &mut store, KeyCode::Char('s'));
    }
    assert_eq!(stage_of(&store, id), "Declined");
    app.refresh(&store)?;
    t.draw(|f| draw(f, &app, &store))?;
    assert!(text(&t).contains("closed"));
    press(&mut app, &mut store, KeyCode::Char('s'));
    assert!(app.status.contains("No further stage"), "{}", app.status);
    // A leftover stage is flagged and `s` refuses to guess.
    app.selected = app.rows.iter().position(|a| a.id == legacy).unwrap();
    app.refresh(&store)?;
    t.draw(|f| draw(f, &app, &store))?;
    assert!(text(&t).contains("not one of your configured stages"));
    press(&mut app, &mut store, KeyCode::Char('s'));
    assert!(app.status.contains("not one of your configured stages"));
    assert_eq!(stage_of(&store, legacy), "Applied");
    // The add form starts on the configured default and cycles through the configured list.
    press(&mut app, &mut store, KeyCode::Char('a'));
    assert!(matches!(&app.mode, Mode::Form(f) if f.values[2] == "Sent"));
    press(&mut app, &mut store, KeyCode::Tab);
    press(&mut app, &mut store, KeyCode::Tab);
    press(&mut app, &mut store, KeyCode::Right);
    assert!(matches!(&app.mode, Mode::Form(f) if f.values[2] == "Call"));
    Ok(())
}
fn stage_of(store: &Store, id: i64) -> String {
    store
        .list(false)
        .unwrap()
        .into_iter()
        .find(|a| a.id == id)
        .unwrap()
        .stage
}
