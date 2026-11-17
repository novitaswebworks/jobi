# Jobi roadmap

## Milestones 1–2: usable local core

- [x] SQLite application storage
- [x] Create/edit/search applications
- [x] Today view with due-date ordering
- [x] Complete actions without losing their history
- [x] Stage transitions and dated notes
- [x] Archive and restore
- [x] Plain-text CLI operations
- [ ] Real-user feedback on daily workflow

## Milestone 3: interview preparation

- [x] Interview dates, IANA timezones, and preparation notes
- [x] Saved posting URL and job description
- [x] Linked application-specific documents
- [x] Multi-line editing of descriptions and interview notes through the user's editor
- [x] Interview edit/remove and document unlink; missing linked files are flagged
- [x] Details/history scrolling limited to content, help lists all shortcuts

## Milestone 4: portability

- [x] CSV import preview and validation
- [x] CSV export
- [x] Complete backup/restore (missing linked documents are reported)
- [ ] Merge restore into a non-empty database
- [x] Newer-schema databases are refused (tested); upgrade from the v1 schema verified
- [ ] A real migration step when the schema first changes

## Milestone 5: distribution

- [x] Real-terminal (PTY) verification on macOS: wide/narrow/too-small sizes, quit and Ctrl+C restore the screen, `$EDITOR` suspend/restore
- [ ] Linux verification (CI configured; not yet run)
- [x] Release profile, tagged-release workflow, installation instructions (workflow not yet run)
- [ ] Accessibility/plain-output review
- [x] `jobi` is unclaimed on crates.io and Homebrew as of this check; re-check right before publishing
- [x] MIT LICENSE file (Novitas WebWorks)

Do not add email sync, job scraping, or AI features before validating the core workflow.
