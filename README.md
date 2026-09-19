# docket

A local-first case file organizer for public defenders.

Public defenders routinely carry 100+ active cases with zero software budget.
docket is a small, fast, free tool for tracking cases, deadlines, and
documents — built for that reality.

## Who it's for

- Public defenders and court-appointed counsel
- Legal aid organizations
- Solo practitioners doing indigent defense
- Law students in criminal defense clinics

## Principles

- **Local-first.** Everything lives in a single SQLite file (`docket.db`) in
  the directory where you run docket. It is created on first use.
- **Zero network.** docket makes no network connections. Ever.
- **Zero telemetry.** No analytics, no crash reporting, no phone-home of any
  kind. Your data never leaves your machine.
- **Boring tech.** SQLite via `rusqlite` (bundled — no system dependency), a
  Rust core library, and a `clap` CLI. Your data is in an open, portable
  format you can back up with `cp`.

## Quickstart

```sh
cargo install --path .

# Add a case
docket case add --number CR-2026-0142 --defendant "J. Doe" \
  --charges "possession w/ intent" --court "Superior Court, Dept. 4"

# Add a case with a pseudonym instead of a real name (see below)
docket case add --number CR-2026-0143 --pseudonym "Client J.D." \
  --charges "petty theft"

# List and inspect
docket case list
docket case list --status open
docket case show CR-2026-0142

# Deadlines
docket deadline add --case CR-2026-0142 --kind arraignment --due 2026-10-02
docket deadline add --case CR-2026-0142 --kind discovery_cutoff --due 2026-11-15
docket deadlines            # overdue + next 14 days, overdue highlighted
docket deadlines --days 30
docket deadline done 1

# Documents
docket doc add --case 1 --kind discovery --path ./police-report.pdf

# Search across case numbers, names, charges, notes
docket search "possession"

# Close a case
docket close CR-2026-0142
```

## Pseudonyms and screen sharing

Public defenders share their screens constantly — in court, on calls, in
trainings. `docket case add` supports a `--pseudonym` flag:

```sh
docket case add --number CR-2026-0143 --pseudonym "Client J.D." --charges "..."
```

When `--pseudonym` is given, the pseudonym is stored in the defendant name
field **instead of** the real name. The real name is never written to the
database, so it can't leak onto a shared screen, a screenshot, or a backup.
You track the pseudonym-to-client mapping however your office already handles
confidential keys (paper file, encrypted vault, etc.).

## An honest note

docket is **not affiliated with, endorsed by, or connected to any court
system**. It is a personal organizer, nothing more. You are responsible for
complying with your jurisdiction's confidentiality rules, your office's data
handling policies, and your professional obligations regarding client
information. "Local-first" helps — nothing is transmitted anywhere — but it
does not replace your judgment about what to store and how to protect the
machine it lives on. Consider full-disk encryption.

## Roadmap

- [x] Rust core library + CLI
- [ ] Tauri GUI (same local-first core, no new dependencies on the network)
- [ ] Deadline reminders (optional, local only)
- [ ] Export a case summary to plain text / PDF for court days

## License

Apache-2.0. See [LICENSE](LICENSE).

Made by synth with blackclaw
