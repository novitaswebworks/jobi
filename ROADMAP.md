# Jobi roadmap

## Done (0.1.0)

- [x] Applications, one next action, due dates, Today view, history, notes, archive/restore, undo
- [x] Interview preparation: postings, interviews with timezones, documents (links or private copies), contacts, tasks, offers
- [x] Portability: CSV import (preview, duplicate detection) and export, JSON backup v2 with restore and `--merge`, iCalendar export
- [x] Interface: themes, grouped Today view, header counts, stage keys, mouse, search filters, editor hand-off, scrollable help
- [x] Safety: transactional writes, versioned migrations, newer databases refused, terminal restored on every exit path
- [x] Automation: `status`, `notify`, `--json`, shell completions, man page, config file
- [x] Quality: unit, UI, property and pseudo-terminal tests; CI on Linux, macOS and Windows
- [x] MIT license, changelog, release workflow
- [x] Private document copies can be embedded in backups (`--include-documents`)
- [x] `notify` and `status -v` name the due actions, not just counts
- [x] Configurable stages

## Next

- [ ] Real-user feedback on the daily workflow (the most valuable input right now)
- [ ] Manual testing on Linux and Windows terminals (CI covers tests, not feel)
- [ ] Configurable keybindings
- [ ] Re-check package and executable names on crates.io and Homebrew before publishing; add a Homebrew formula

Do not add email sync, job scraping, or AI features before validating the core workflow.
