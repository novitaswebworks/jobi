# Changelog

All notable changes to Jobi are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [0.2.0] - 2026-11-17

### Added
- Configurable stages: `stages`, `closed_stages`, `stale_stages`, `advanced_stages` and
  `follow_up_stage` in `config.toml`, with derived defaults, `jobi stages`, and graceful handling
  of applications whose stage is no longer configured. Stage names match ignoring case.
- `backup --include-documents` embeds private document copies; restore writes them back
  next to the destination database (only the file name from the backup is used).
- `jobi status -v` and `jobi notify` name the overdue and due-today actions; `status --json`
  includes them as `items`.

## [0.1.0] - 2026-11-17

First release.

### Tracking
- Applications with stage, one next action, due date, archive/restore and a full
  timestamped history; notes; quick stage change (`s`/`S`) and undo (`u`).
- Due dates accept `today`, `tomorrow`, `+3d`, `+2w` or a weekday.
- New `Applied` applications get an automatic follow-up; applications with no
  activity for `stale_days` are flagged.
- Per-application tasks, contacts, one offer each, and an offer comparison.
- Interview preparation: job posting, interviews with timezones, linked documents
  or private copies, with missing files flagged.
- Pipeline statistics (`jobi stats`, `t`).

### Interface
- Today view grouped into Overdue / Today / This week / Later, with counts in the header.
- Colour themes (dark, light, mono; `NO_COLOR` honoured), mouse support, search filters
  (`stage:`, `due:`), scrollable help, `Ctrl+E` to edit long text in `$VISUAL`/`$EDITOR`.
- Terminal is restored on quit, `Ctrl+C`, `SIGTERM` and `SIGHUP`.

### Command line and data
- Full CLI with `--json` output, shell completions and a man page.
- `jobi status` and `jobi notify` for prompts and cron; `jobi ical` calendar export.
- CSV export, previewed and duplicate-aware CSV import, complete JSON backup (format
  version 2, reads version 1) and restore, including `--merge`.
- SQLite with versioned, transactional migrations; databases from a newer Jobi are refused.
- Optional `config.toml` (default timezone and stage, theme, stale and follow-up days).
