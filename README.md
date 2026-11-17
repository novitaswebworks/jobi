# Jobi

[![CI](https://github.com/novitaswebworks/jobi/actions/workflows/ci.yml/badge.svg)](https://github.com/novitaswebworks/jobi/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**Your next career move, one action at a time.** Jobi is a local-first job application tracker for your terminal, built with Rust, [Ratatui](https://ratatui.rs), and SQLite. No account, no network, no cloud: your job search lives in one file on your computer.

![Jobi demo](docs/demo.gif)

<sub>Full recording: [docs/demo.mp4](docs/demo.mp4). Re-create it with `vhs docs/demo.tape`.</sub>

## Why Jobi

A job search is a pile of small follow-ups. Jobi keeps one thing front and centre: **what is the next action for each application, and when is it due?**

- **Today view**, grouped into Overdue / Today / This week / Later, with overdue and due-today counts in the header.
- **One next action per application**, plus a task checklist for everything else. Completing an action records it in history instead of erasing it, and `u` undoes it.
- **Nothing falls through the cracks.** New *Applied* applications get an automatic follow-up, and applications with no activity for two weeks are flagged *stale*.
- **Interview preparation in one place**: job posting, interviews with timezones, contacts, linked documents (or private copies), and an offer with side-by-side comparison.
- **Full history** of stage changes, edits, notes, and completed actions, timestamped in UTC.
- **Portable and safe**: CSV import with a preview and duplicate detection, CSV and calendar export, complete JSON backup with merge-restore. Every write is all-or-nothing.
- **Keyboard-first TUI (mouse optional) plus a scriptable CLI** with `--json` output, shell completions, and a man page.

## Install

Jobi needs a [Rust toolchain](https://rustup.rs). SQLite is bundled, so there are no system libraries to install.

```sh
git clone https://github.com/novitaswebworks/jobi.git
cd jobi
cargo install --path . --locked
```

Cargo installs `jobi` into `~/.cargo/bin`; make sure that directory is on your `PATH`. To upgrade, pull and run the same command again. Your database is upgraded in place on startup by versioned, transactional migrations; a database created by a *newer* Jobi is refused rather than downgraded.

Pushing a `vX.Y.Z` tag runs the [CI workflow](.github/workflows/ci.yml), which tests on Linux, macOS, and Windows and attaches `jobi-<tag>-linux-x86_64.tar.gz` and `jobi-<tag>-macos-arm64.tar.gz` (with SHA-256 files) to a GitHub release. Unpack the archive and put `jobi` on your `PATH`.

**Shell completions and man page**

```sh
jobi completions zsh > ~/.zfunc/_jobi          # also bash, fish, elvish, powershell
jobi man > ~/.local/share/man/man1/jobi.1
```

## Quick start

```sh
jobi            # open the interactive app
jobi sample     # or press L in an empty app: load example data to explore
```

Press **a**, fill in company and role, choose a stage with **←/→**, and optionally add a next action and due date (`2026-12-01`, `tomorrow`, `+3d`, `+2w`, or a weekday such as `fri`). **Tab** moves between fields, **Enter** saves, **Esc** cancels. Press **?** for keyboard help.

The same from the command line:

```sh
jobi add --company Acme --role 'Backend Engineer' --action 'Follow up' --due fri
jobi today                                    # open actions, earliest due first
jobi note 1 'Recruiter requested a portfolio link'
jobi done 1                                   # complete the next action (history is kept)
jobi status                                   # "1 overdue · 2 due today · 5 open · 1 stale"
```

## The app

| View | Shows |
|------|-------|
| **1 Today** | Applications with an open action, grouped and sorted by due date |
| **2 Applications** | All active applications, including those without a next action |
| **3 Archived** | Archived applications; history is retained and they can be restored |

| Key | Action |
|-----|--------|
| `1` `2` `3` / `Tab` | Switch views |
| `j` `k` / `↑` `↓`, mouse click and wheel | Select an application; scroll the pane under the pointer |
| `/` | Search (see below); `Esc` clears |
| `a` / `e` | Add / edit an application |
| `n` | Add a dated note (`Ctrl+E` to write it in your editor) |
| `x` | Complete the next action |
| `s` / `S` | Move to the next / previous stage in your configured list |
| `u` | Undo the last `x`, `s`, `S`, or archive change |
| `z` | Archive / restore (asks for confirmation) |
| `p` `i` `d` | Job posting, interview, linked document |
| `o` `c` `T` | Offer, contact, task |
| `v` | Review everything: `j`/`k` select, `Space` toggles a task, `e` edits an interview, `r` removes it (or unlinks a document) |
| `t` | Pipeline statistics |
| `Ctrl+E` | In a form: edit long text in `$VISUAL` / `$EDITOR` |
| `PgUp` / `PgDn` | Scroll details and history |
| `?` | Help |
| `q` / `Ctrl+C` | Quit |

**Search** takes plain words that must all match (company, role, stage, action) plus filters: `stage:interview` (prefix match) and `due:overdue|today|week|none`. For example: `rust stage:screen due:week`.

The UI needs a terminal of at least 50 columns by 18 rows. Wide terminals show the list and details side by side; narrow ones stack them. Colours follow the `theme` setting; set `NO_COLOR` for a monochrome interface.

## Staying on top of things

- **Automatic follow-up.** A new application added as *Applied* with no action and no due date gets "Follow up" due in `follow_up_days` (default 7). Turn it off with `auto_follow_up = false`.
- **Stale applications.** Active applications (Applied through Offer) with no activity for `stale_days` (default 14) are marked *stale* in the list, the details pane, the header, and `jobi status`.
- **Prompt and cron integration.**

  ```sh
  jobi status              # 1 overdue · 1 due today · 4 open
  jobi status --quiet      # prints nothing when nothing is due: handy in a shell prompt
  jobi status -v           # also lists each overdue or due-today action
  jobi notify              # desktop notification naming the first three due actions
  ```

  Run `jobi notify` from cron or launchd (`0 9 * * *`) for a daily nudge. It uses `osascript` on macOS and `notify-send` on Linux, and prints on other platforms.
- **Calendar.** `jobi ical interviews.ics [--actions]` writes interviews (and optionally each open next action as an all-day event) for import into any calendar app.

## Interview preparation

Select an application, then press `p` to save its posting, `i` to schedule an interview, `d` to link a document, `c` to add a contact, `T` to add a task, `o` to record an offer, or `v` to review everything saved. In the interview form use an RFC 3339 datetime with an offset (for example `2026-11-20T14:00:00+05:30`) and an IANA timezone; the timezone defaults to your configured or system timezone.

**Long text.** In the posting description, interview notes, contact notes, offer notes, and the note box, press `Ctrl+E` to write in your editor (`$VISUAL`, then `$EDITOR`, then `vi`; `notepad` on Windows). Jobi suspends the screen while the editor runs and restores it afterwards. If the editor exits with an error, your text is left unchanged. The text passes through a temporary file readable only by you, which is deleted afterwards.

**Documents** are linked by absolute path by default, and linked files that no longer exist are marked `FILE MISSING`. With `attach --copy` (or answering *yes* in the `d` form) Jobi keeps a private copy in a `documents` folder next to the database, so it survives the original being moved. Unlinking never touches your own file; unlinking a private copy deletes the copy.

**Offers.** One offer per application: yearly base pay in whole currency units, a 3-letter currency code, optional bonus, equity, decision deadline, and notes. `jobi offers` lists them side by side, grouped by currency (no exchange-rate conversion).

```sh
jobi posting 1 --url https://example.com/job --description "Role details"
jobi interview 1 --at 2026-11-20T14:00:00+05:30 --kind Technical    # timezone from config
jobi interviews 1                         # list, with ids
jobi interview-edit 3 --kind Onsite --notes "Bring laptop"
jobi interview-remove 3
jobi attach 1 ./resume.pdf --label "Submitted resume" [--copy]
jobi documents 1                          # list, with ids; flags missing files
jobi unlink 2
jobi task add 1 "Send thank-you" --due tomorrow
jobi task list 1 / done 4 / reopen 4 / rm 4
jobi contact add 1 "Jane Doe" --role Recruiter --email jane@example.com --last-contacted today
jobi contact list 1 / rm 2
jobi offer set 1 --base 120000 --currency USD --bonus 10000 --deadline +10d
jobi offer show 1 / rm 1
jobi offers                               # compare all offers
```

## CLI reference

| Command | Purpose |
|---------|---------|
| `jobi` | Open the interactive app |
| `add`, `list [--archived]`, `today` | Add and show applications |
| `done`, `note`, `history`, `archive [--restore]` | Work on one application |
| `status [--quiet] [-v]`, `notify` | Summary line (optionally with the due actions); desktop notification |
| `stats` | Stage counts, advancement rate, applications added per week |
| `stages` | Configured stages, their roles, and how many applications are in each |
| `posting`, `interview`, `interviews`, `interview-edit`, `interview-remove` | Interview preparation |
| `attach`, `documents`, `unlink` | Documents |
| `task`, `contact`, `offer`, `offers` | Checklist, people, offers |
| `export`, `import`, `backup`, `restore`, `ical` | Portability (below) |
| `sample` | Load example applications into an empty database |
| `completions SHELL`, `man` | Shell completions; man page |
| `path` | Print the database location |

Global options: `--db FILE` (or `JOBI_DB`) selects another database; `--json` prints machine-readable JSON from `list`, `today`, `history`, `interviews`, `documents`, `task list`, `contact list`, `offer show`, `offers`, `status`, and `stats`. Run `jobi --help` or `jobi <command> --help` for details.

## Import, export, backup, restore

```sh
jobi export applications.csv            # all applications, active and archived
jobi import applications.csv            # validate and preview; writes nothing
jobi import applications.csv --apply    # import: all rows or none
jobi backup jobi-backup.json            # complete backup
jobi backup full.json --include-documents   # also embed private document copies
jobi --db new.sqlite3 restore jobi-backup.json
jobi restore jobi-backup.json --merge   # add a backup to the current database
```

**CSV format.** UTF-8 with a header row and standard quoting, so descriptions may contain commas and newlines. Required columns: `company`, `role`. Optional: `stage` (default `Applied`), `action`, `due` (`YYYY-MM-DD`, requires an action), `archived` (`true`/`false`), `posting_url`, `posting_description`. `id` is written by export and ignored on import. Unknown or misspelled columns are rejected. Every row is validated before anything is written; errors are reported as `line N: ...`. Rows whose company and role are already tracked (active or archived), or repeated earlier in the file, are skipped and reported; pass `--allow-duplicates` to import them anyway. Interviews, documents, tasks, and history are not part of CSV; use backup for those.

**Backup.** A versioned JSON file (`"format": "jobi-backup"`, currently version 2; version 1 files still restore) with applications, full history, postings, interviews, document links, tasks, contacts, and offers, written from one consistent snapshot. **Restore** writes into an *empty* database and preserves ids and history timestamps. With `--merge` it adds to existing data instead: applications whose company and role already exist are skipped, and the rest get new ids with all their history and attachments. Either way it is all-or-nothing: an invalid or inconsistent file changes nothing. `export`, `backup`, and `ical` refuse to overwrite an existing file unless you pass `--force`.

Documents are stored as paths. Backup and restore warn about each linked file that is missing on this computer but keep the link. Private copies (`attach --copy`) live in a `documents` folder next to the database. Add `--include-documents` to embed them in the backup, base64-encoded, so a restore on another computer writes them back next to that database; without the flag they are left out and listed as missing on restore. Backup files hold your job-search data and documents in plain text; protect them like the database.

## Configuration

Optional. Create the file below; every key is optional and unknown keys are rejected so typos do not go unnoticed.

| Platform | Location |
|----------|----------|
| macOS | `~/Library/Application Support/jobi/config.toml` |
| Linux | `$XDG_CONFIG_HOME/jobi/config.toml`, or `~/.config/jobi/config.toml` |
| Windows | `%APPDATA%\jobi\config.toml` |

`JOBI_CONFIG` points at a different file.

```toml
default_timezone = "Europe/Berlin"   # interview default; falls back to the system timezone
default_stage    = "Applied"         # preselected when adding
theme            = "dark"            # dark | light | mono
stale_days       = 14
follow_up_days   = 7
auto_follow_up   = true

# Optional: your own stages. Order matters; the first means "not applied yet".
stages        = ["Idea", "Sent", "Call", "Onsite", "Offer", "Hired", "Declined"]
closed_stages = ["Declined"]   # finished outcomes: shown as closed, never stale
```

**Custom stages.** Without `stages` you get Wishlist, Applied, Screening, Interview, Offer, Accepted, Rejected, Withdrawn. With it, `default_stage` must be one of yours, and stage names are matched ignoring case. Everything that depends on a stage's meaning is derived unless you set it: `closed_stages` defaults to any stage named Rejected or Withdrawn; `stale_stages` (where inactivity is flagged) to every stage after the first that is not closed, except the last open one (the final outcome); `advanced_stages` (progress in statistics) to the third stage onwards that is not closed; `follow_up_stage` (where new applications get the automatic follow-up) to the second stage. Set any of them explicitly with stage names from your list. `jobi stages` shows the result and how many applications sit in each.

Changing stages never rewrites your data. Applications whose stage is no longer in the list keep it: they are flagged "not one of your configured stages", still open and editable, and listed as `(not configured)` in `jobi stats`; you can move them to a configured stage with `e`, but not to another unknown one. Restoring a backup accepts any stage name, because the data predates your configuration; CSV import accepts only configured stages and lists them in the error.

## Where your data lives

| Platform | Default database |
|----------|------------------|
| macOS | `~/Library/Application Support/jobi/jobi.sqlite3` |
| Linux | `$XDG_DATA_HOME/jobi/jobi.sqlite3`, or `~/.local/share/jobi/jobi.sqlite3` |
| Windows | `%APPDATA%\jobi\jobi.sqlite3` |

```sh
jobi path                             # print the location
jobi --db /tmp/jobi-demo.sqlite3      # use another database
```

`JOBI_DB` also overrides the path. Jobi uses your computer's local calendar date for deadlines and records history timestamps in UTC. Data is not encrypted; it inherits the protection of your OS account and filesystem. Use `jobi backup` rather than copying the database file by hand.

## Project layout

```
src/main.rs       CLI definition and command dispatch
src/store.rs      SQLite schema migrations, validation, and core reads/writes
src/extras.rs     Tasks, contacts, offers
src/portable.rs   CSV import/export, JSON backup/restore/merge
src/ical.rs       iCalendar export
src/stats.rs      Pipeline statistics and sample data
src/config.rs     config.toml and platform directories
src/dates.rs      Friendly due-date parsing
src/ui/           Ratatui interface: input, draw, theme, tests
tests/            Pseudo-terminal smoke test
docs/             Demo recording (demo.tape, demo-seed.sh, demo.gif, demo.mp4)
```

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                                    # unit, UI, and property tests
cargo build && python3 tests/pty_smoke.py target/debug/jobi   # real-terminal checks (Unix)
```

The smoke test drives the binary in a pseudo-terminal and checks that quitting, `Ctrl+C`, `SIGTERM`, and `SIGHUP` restore the screen, that a tiny terminal shows the resize message, and that the `$EDITOR` hand-off works. To re-record the demo you need [vhs](https://github.com/charmbracelet/vhs), `ttyd`, and `ffmpeg`, with `jobi` on your `PATH`: `vhs docs/demo.tape`. See the [changelog](CHANGELOG.md) and [roadmap](ROADMAP.md).

## Status and limitations

Jobi is early software with a deliberately small scope.

- Developed and manually tested on macOS. CI runs the test suite on Linux, macOS, and Windows, and the terminal smoke test on Linux and macOS; Windows has had no manual testing.
- Private document copies are only in a backup when you pass `--include-documents`, which makes the file larger.
- Offers are compared per currency; there is no exchange-rate conversion.
- There is no background service. Use `jobi notify` from cron or launchd for reminders.
- Email sync, job scraping, and AI features are intentionally out of scope until the core workflow is proven.

## License

[MIT](LICENSE) © 2026 Novitas WebWorks
