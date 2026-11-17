# Jobi

[![CI](https://github.com/novitaswebworks/jobi/actions/workflows/ci.yml/badge.svg)](https://github.com/novitaswebworks/jobi/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**Your next career move, one action at a time.** Jobi is a local-first job application tracker for your terminal, built with Rust, [Ratatui](https://ratatui.rs), and SQLite. No account, no network, no cloud: your job search lives in one file on your computer.

![Jobi demo](docs/demo.gif)

<sub>Full recording: [docs/demo.mp4](docs/demo.mp4). Re-create it with `vhs docs/demo.tape`.</sub>

## Why Jobi

A job search is a pile of small follow-ups. Jobi keeps exactly one thing front and centre: **what is the next action for each application, and when is it due?**

- **Today view** lists every application with an open action, earliest due first, with overdue and due-today items called out.
- **One next action per application**, with a due date. Completing it records the fact in history instead of erasing it.
- **Full history** of stage changes, edits, notes, and completed actions, timestamped in UTC.
- **Interview prep** in one place: posting URL and description, interviews (with timezone and notes), and linked documents.
- **Portable and safe**: CSV import with a preview, CSV export, and complete JSON backup/restore. Every write is all-or-nothing.
- **Keyboard-first TUI plus a scriptable CLI**, with plain-text output.

## Install

Jobi needs a [Rust toolchain](https://rustup.rs). SQLite is bundled, so there are no system libraries to install.

```sh
git clone https://github.com/novitaswebworks/jobi.git
cd jobi
cargo install --path . --locked
```

Cargo installs `jobi` into `~/.cargo/bin`; make sure that directory is on your `PATH`. To upgrade, pull and run the same `cargo install` command again. Your database is upgraded in place on startup (only new tables are added; existing applications and history are untouched). A database created by a newer Jobi is refused rather than downgraded.

Pushing a `vX.Y.Z` tag runs the [CI workflow](.github/workflows/ci.yml), which tests on Linux and macOS and attaches `jobi-<tag>-linux-x86_64.tar.gz` and `jobi-<tag>-macos-arm64.tar.gz` (with SHA-256 files) to a GitHub release. Unpack the archive and put `jobi` on your `PATH`.

## Quick start

```sh
jobi          # open the interactive app
```

Press **a**, fill in company and role, choose a stage with **←/→**, and optionally add a next action and due date. **Tab** moves between fields, **Enter** saves, **Esc** cancels. Press **?** at any time for keyboard help.

Or use the CLI:

```sh
jobi add --company Acme --role 'Backend Engineer' --action 'Follow up' --due 2026-11-24
jobi today                                    # open actions, earliest due first
jobi list                                     # all active applications
jobi note 1 'Recruiter requested a portfolio link'
jobi done 1                                   # complete the next action (history is kept)
jobi history 1
jobi archive 1                                # hide it; use --restore to bring it back
```

`jobi add` without `--company`/`--role` opens the add form in the TUI.

## The app

| View | Shows |
|------|-------|
| **1 Today** | Applications with an open action, sorted by due date; unscheduled actions last |
| **2 Applications** | All active applications, including those without a next action |
| **3 Archived** | Archived applications; history is retained and they can be restored |

| Key | Action |
|-----|--------|
| `1` `2` `3` / `Tab` | Switch views |
| `j` `k` / `↑` `↓` | Select an application |
| `/` | Search company, role, stage, and action (`Esc` clears) |
| `a` / `e` | Add / edit an application |
| `n` | Add a dated note |
| `x` | Complete the next action |
| `z` | Archive / restore (asks for confirmation) |
| `p` | Save the job posting URL and description |
| `i` | Schedule an interview |
| `d` | Link a document |
| `v` | View details; then `j`/`k` select, `e` edits an interview, `r` removes it or unlinks a document |
| `Ctrl+E` | In a long-text field, edit in `$VISUAL` / `$EDITOR` |
| `PgUp` / `PgDn` | Scroll details and history |
| `?` | Help |
| `q` / `Ctrl+C` | Quit |

The UI needs a terminal of at least 50 columns by 18 rows. Wide terminals show the list and details side by side; narrow ones stack them.

## Interview preparation

Select an application, then press `p` to save its posting, `i` to schedule an interview, `d` to link a document, or `v` to review everything saved. In the interview form use an RFC 3339 datetime with an offset (for example `2026-11-20T14:00:00+05:30`) and an IANA timezone (for example `Asia/Kolkata`).

**Long text.** In the posting description and interview notes fields, press `Ctrl+E` to write in your editor (`$VISUAL`, then `$EDITOR`, then `vi`). Jobi suspends the screen while the editor runs and restores it afterwards. If the editor exits with an error, your text is left unchanged. The text passes through a temporary file readable only by you, which is deleted afterwards.

**Documents** are linked by absolute path; Jobi does not copy or manage the files. Linked files that no longer exist are marked `FILE MISSING`. Unlinking removes only the link, never the file.

CLI equivalents:

```sh
jobi posting 1 --url https://example.com/job --description "Role details"
jobi interview 1 --at 2026-11-20T14:00:00+05:30 --timezone Asia/Kolkata --kind Technical
jobi interviews 1                             # list, with ids
jobi interview-edit 3 --kind Onsite --notes "Bring laptop"
jobi interview-remove 3
jobi attach 1 ./resume.pdf --label "Submitted resume"
jobi documents 1                              # list, with ids; flags missing files
jobi unlink 2                                 # removes the link only
```

## CLI reference

| Command | Purpose |
|---------|---------|
| `jobi` | Open the interactive app |
| `add` | Add an application (opens the form if company/role are omitted) |
| `today`, `list [--archived]` | Show applications |
| `history ID`, `note ID TEXT` | Read history; append a dated note |
| `done ID` | Complete the next action |
| `archive ID [--restore]` | Archive or restore |
| `posting`, `interview`, `interviews`, `interview-edit`, `interview-remove` | Interview preparation |
| `attach`, `documents`, `unlink` | Linked documents |
| `export`, `import`, `backup`, `restore` | Portability (below) |
| `path` | Print the database location |

Global option: `--db FILE` (or `JOBI_DB`) selects a different database. Run `jobi --help` or `jobi <command> --help` for details.

## Import, export, backup, restore

```sh
jobi export applications.csv            # all applications, active and archived
jobi import applications.csv            # validate and preview; writes nothing
jobi import applications.csv --apply    # import: all rows or none
jobi backup jobi-backup.json            # complete backup
jobi --db new.sqlite3 restore jobi-backup.json
```

**CSV format.** UTF-8 with a header row and standard quoting, so descriptions may contain commas and newlines. Required columns: `company`, `role`. Optional: `stage` (default `Applied`), `action`, `due` (`YYYY-MM-DD`, requires an action), `archived` (`true`/`false`), `posting_url`, `posting_description`. `id` is written by export and ignored on import. Unknown or misspelled columns are rejected. Every row is validated before anything is written; errors are reported as `line N: ...`. Import always creates new applications (it does not de-duplicate or update existing ones) and records an "Imported from CSV" history entry. Interviews, documents, and history are not part of CSV; use backup for those.

**Backup.** A versioned JSON file (`"format": "jobi-backup"`, `"version": 1`) with applications, full history, postings, interviews, and document links, written from one consistent snapshot. **Restore** writes only into an *empty* database, preserves ids and history timestamps, and is all-or-nothing: an invalid or inconsistent file changes nothing. `export` and `backup` refuse to overwrite an existing file unless you pass `--force`.

Documents are stored as paths, not contents. Backup and restore warn about each linked file that is missing on this computer, but keep the link. Backup files hold your job-search data in plain text; protect them like the database.

## Where your data lives

| Platform | Default location |
|----------|------------------|
| macOS | `~/Library/Application Support/jobi/jobi.sqlite3` |
| Linux | `$XDG_DATA_HOME/jobi/jobi.sqlite3`, or `~/.local/share/jobi/jobi.sqlite3` |

```sh
jobi path                             # print the location
jobi --db /tmp/jobi-demo.sqlite3      # use another database
```

`JOBI_DB` also overrides the path. Jobi uses your computer's local calendar date for deadlines and records history timestamps in UTC. Data is not encrypted; it inherits the protection of your OS account and filesystem. Use `jobi backup` rather than copying the database file by hand.

## Project layout

```
src/main.rs       CLI definition and command dispatch
src/store.rs      SQLite schema, validation, and all reads/writes (transactional)
src/portable.rs   CSV import/export and JSON backup/restore
src/ui.rs         Ratatui interface, key handling, and external-editor support
docs/             Demo recording (demo.tape, demo-seed.sh, demo.gif, demo.mp4)
```

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

To re-record the demo you need [vhs](https://github.com/charmbracelet/vhs), `ttyd`, and `ffmpeg`, with `jobi` on your `PATH`: `vhs docs/demo.tape`.

## Status and limitations

Jobi is early software with a deliberately small scope.

- Verified on macOS, including real-terminal checks of resizing, quitting, `Ctrl+C`, and editor hand-off. Linux is covered by CI but has had no manual testing. Windows is not supported or validated.
- Restore cannot merge into a database that already has data.
- Terminals that are killed with `SIGTERM` or `SIGKILL` may need `reset`; normal quit and `Ctrl+C` restore the screen.
- There are no notifications or background services; Today is an in-app list.
- Email sync, job scraping, and AI features are intentionally out of scope until the core workflow is proven. See the [roadmap](ROADMAP.md).

## License

[MIT](LICENSE) © 2026 Novitas WebWorks
