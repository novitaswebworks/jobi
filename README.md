# Jobi

A local-first job application tracker for your terminal, built with Rust, Ratatui, and SQLite.

## Run

```sh
cargo run
```

For a command available from any directory:

```sh
cargo install --path .
jobi
```

Cargo installs the executable into `~/.cargo/bin`; that directory must be on your PATH.

## Install

Requires a Rust toolchain (https://rustup.rs). SQLite is bundled; no system libraries are needed.

```sh
git clone <this repository> && cd jobi
cargo install --path . --locked
```

To upgrade, pull and run the same `cargo install` command again. Your database is left in place and is upgraded in place when Jobi starts (new tables only; existing applications and history are untouched). A database created by a newer Jobi is refused rather than downgraded.

Pushing a `vX.Y.Z` tag runs `.github/workflows/ci.yml`, which tests on Linux and macOS and attaches `jobi-<tag>-linux-x86_64.tar.gz` and `jobi-<tag>-macos-arm64.tar.gz` (with SHA-256 files) to a GitHub release. Unpack the archive and put `jobi` on your PATH.

## First application

Press **a** in the app, fill out company and role, choose a stage with Left/Right, and optionally add a next action and due date. Tab moves between fields; Enter saves; Esc cancels.

Or use the CLI:

```sh
jobi add --company Acme --role 'Backend Engineer' --action 'Follow up' --due 2026-11-24
jobi today
jobi list
jobi note 1 'Recruiter requested a portfolio link'
jobi done 1
jobi history 1
jobi archive 1
jobi archive 1 --restore
```

## Views and controls

- **1 Today**: all applications with an open action, sorted by due date, with unscheduled actions last. Overdue and due-today actions are labeled.
- **2 Applications**: active applications, including those without a next action.
- **3 Archived**: archived applications; history is retained and applications can be restored.
- **j/k** or arrow keys: select; **/**: search; **Esc**: clear search.
- **a/e**: add/edit; **n**: note; **x**: complete next action; **z**: archive/restore with confirmation.
- **Page Up/Down**: scroll details/history; **?**: help; **q** or **Ctrl+C**: quit.

The UI requires a terminal of at least 50 columns and 18 rows. Narrow terminals stack the list and details vertically.

## Storage

No account or network service is needed at runtime. On macOS, data is stored at `~/Library/Application Support/jobi/jobi.sqlite3`; on Linux, under `$XDG_DATA_HOME/jobi` or `~/.local/share/jobi`.

```sh
jobi path
jobi --db /tmp/jobi-demo.sqlite3
```

`JOBI_DB` also overrides the path. Jobi uses your computer's local calendar date for action deadlines and records history timestamps in UTC. Application data is not encrypted; it inherits the protection of your OS account and filesystem. Use `jobi backup` (below) rather than copying the database file by hand.

## Interviews, postings, and documents

Select an application and press `p` to save/update its posting URL and description, `i` to schedule an interview, `d` to link an existing document, and `v` to view the saved posting, interviews, and linked documents. In the interview form, use an RFC3339 datetime with an offset (for example `2026-11-20T14:00:00+05:30`) and an IANA timezone.

In the details view (`v`), `j`/`k` select an interview or document, `e` edits the selected interview, and `r` removes it (interviews) or unlinks it (documents; the file itself is never deleted), each after a `y` confirmation. Linked files that no longer exist are marked `FILE MISSING`.

**Long text:** in the posting description and interview notes fields, press `Ctrl+E` to edit in your editor (`$VISUAL`, then `$EDITOR`, then `vi`). Jobi suspends the screen while the editor runs and restores it afterward; quitting the editor with an error leaves the text unchanged. The text passes through a temporary file readable only by you, which is deleted afterward. Press `PgUp`/`PgDn` to scroll the details pane; it shows a hint when content overflows.

CLI equivalents:

```sh
jobi posting 1 --url https://example.com/job --description "Role details"
jobi interview 1 --at 2026-11-20T14:00:00+05:30 --timezone Asia/Kolkata --kind Technical
jobi attach 1 ./resume.pdf --label "Submitted resume"
jobi documents 1                 # ids; marks missing files
jobi interviews 1                # ids
jobi interview-edit 3 --kind Onsite --notes "Bring laptop"
jobi interview-remove 3
jobi unlink 2                    # removes the link only
```

Documents are linked by absolute path; Jobi does not copy or manage the files yet. Keep the source files in place. Posting text and interview information are stored locally in SQLite.

## CSV import/export and backup/restore

```sh
jobi export applications.csv            # all applications, active and archived
jobi import applications.csv            # preview + validation only; writes nothing
jobi import applications.csv --apply    # import, all rows or none
jobi backup jobi-backup.json            # complete backup
jobi --db new.sqlite3 restore jobi-backup.json
```

**CSV format** (header row required, UTF-8, standard quoting so descriptions may contain commas and newlines): `company`, `role` (required), then optional `stage` (default `Applied`), `action`, `due` (`YYYY-MM-DD`; needs an action), `archived` (`true`/`false`), `posting_url`, `posting_description`. `id` is written by export and ignored on import. Unknown or misspelled columns are rejected. Import always creates new applications (it does not de-duplicate or update), and records an "Imported from CSV" history entry. Every row is validated up front; any error is reported as `line N: ...` and nothing is imported. Interviews, documents, and history are not part of CSV; use backup for those.

**Backup** is a versioned JSON file (`"format": "jobi-backup"`, `"version": 1`) holding applications, full activity history, postings, interviews, and document links, written from one consistent snapshot. **Restore** only writes into an *empty* database (use `--db` to point at a new one), preserves ids and history timestamps, and is all-or-nothing: an invalid or inconsistent file changes nothing. `export` and `backup` refuse to overwrite an existing file unless you pass `--force`.

Documents are stored as paths, not file contents. Backup and restore warn about each linked file that does not exist on this computer, but still keep the link. Backup files contain your job-search data in plain text; protect them like the database.

## Current scope

Implemented: persistent applications, arbitrary stage transitions, one open next action, due dates, Today view, searchable active/archive views, notes, atomic stage/action history, archive/restore, and CLI operations.

Not yet implemented: merging a backup into a non-empty database, notifications, or integrations. Today is an in-app action list, not a background notification service. The CLI supports macOS/Linux storage defaults; Windows support has not been validated.

## Development

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

See [ROADMAP.md](ROADMAP.md) for the next milestones.
