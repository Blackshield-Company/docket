//! docket — local-first case file organizer for public defenders.
//!
//! All data lives in a single SQLite file (`docket.db`) on the local
//! machine. No network access, no telemetry, no cloud sync.

use anyhow::{anyhow, Context, Result};
use chrono::{Duration, Local, NaiveDate};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Default database filename, created in the current working directory.
pub const DB_FILENAME: &str = "docket.db";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Case {
    pub id: i64,
    pub case_number: String,
    pub defendant_name: String,
    pub charges: String,
    pub court: Option<String>,
    pub status: String,
    pub opened_date: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Deadline {
    pub id: i64,
    pub case_id: i64,
    pub kind: String,
    pub due_date: String,
    pub completed: bool,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Document {
    pub id: i64,
    pub case_id: i64,
    pub kind: String,
    pub path: String,
    pub added_date: String,
    pub notes: Option<String>,
}

/// A deadline joined with its case, plus a computed overdue flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpcomingDeadline {
    pub deadline: Deadline,
    pub case_number: String,
    pub defendant_name: String,
    pub overdue: bool,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open (creating on first use) the database at the given path.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open database at {}", path.display()))?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    /// Open the default `docket.db` in the current working directory.
    pub fn open_default() -> Result<Self> {
        Self::open(Path::new(DB_FILENAME))
    }

    /// In-memory database, for tests.
    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS cases (
                id             INTEGER PRIMARY KEY AUTOINCREMENT,
                case_number    TEXT NOT NULL UNIQUE,
                defendant_name TEXT NOT NULL,
                charges        TEXT NOT NULL DEFAULT '',
                court          TEXT,
                status         TEXT NOT NULL DEFAULT 'open',
                opened_date    TEXT NOT NULL,
                notes          TEXT
            );
            CREATE TABLE IF NOT EXISTS deadlines (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                case_id    INTEGER NOT NULL REFERENCES cases(id),
                kind       TEXT NOT NULL,
                due_date   TEXT NOT NULL,
                completed  INTEGER NOT NULL DEFAULT 0,
                notes      TEXT
            );
            CREATE TABLE IF NOT EXISTS documents (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                case_id    INTEGER NOT NULL REFERENCES cases(id),
                kind       TEXT NOT NULL,
                path       TEXT NOT NULL,
                added_date TEXT NOT NULL,
                notes      TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_deadlines_case ON deadlines(case_id);
            CREATE INDEX IF NOT EXISTS idx_deadlines_due ON deadlines(due_date);
            CREATE INDEX IF NOT EXISTS idx_documents_case ON documents(case_id);
            ",
        )?;
        Ok(())
    }

    /// Add a case. If `pseudonym` is provided it is stored in
    /// `defendant_name` *instead of* the real name — the real name is
    /// never written to disk.
    pub fn add_case(
        &self,
        case_number: &str,
        defendant_name: Option<&str>,
        pseudonym: Option<&str>,
        charges: &str,
        court: Option<&str>,
        notes: Option<&str>,
    ) -> Result<Case> {
        let stored_name = match (pseudonym, defendant_name) {
            (Some(p), _) => p.to_string(),
            (None, Some(n)) => n.to_string(),
            (None, None) => {
                return Err(anyhow!(
                    "either --defendant or --pseudonym must be provided"
                ))
            }
        };
        let opened_date = Local::now().date_naive().to_string();
        self.conn.execute(
            "INSERT INTO cases (case_number, defendant_name, charges, court, status, opened_date, notes)
             VALUES (?1, ?2, ?3, ?4, 'open', ?5, ?6)",
            params![case_number, stored_name, charges, court, opened_date, notes],
        )?;
        let id = self.conn.last_insert_rowid();
        self.get_case_by_id(id)
    }

    pub fn list_cases(&self, status: Option<&str>) -> Result<Vec<Case>> {
        let mut cases = Vec::new();
        match status {
            Some(s) => {
                let mut stmt = self.conn.prepare(
                    "SELECT id, case_number, defendant_name, charges, court, status, opened_date, notes
                     FROM cases WHERE status = ?1 ORDER BY case_number",
                )?;
                let rows = stmt.query_map(params![s], row_to_case)?;
                for r in rows {
                    cases.push(r?);
                }
            }
            None => {
                let mut stmt = self.conn.prepare(
                    "SELECT id, case_number, defendant_name, charges, court, status, opened_date, notes
                     FROM cases ORDER BY case_number",
                )?;
                let rows = stmt.query_map([], row_to_case)?;
                for r in rows {
                    cases.push(r?);
                }
            }
        }
        Ok(cases)
    }

    pub fn get_case_by_id(&self, id: i64) -> Result<Case> {
        self.conn
            .query_row(
                "SELECT id, case_number, defendant_name, charges, court, status, opened_date, notes
                 FROM cases WHERE id = ?1",
                params![id],
                row_to_case,
            )
            .with_context(|| format!("no case with id {id}"))
    }

    /// Resolve an ID or case number to a case.
    pub fn find_case(&self, id_or_number: &str) -> Result<Case> {
        if let Ok(id) = id_or_number.parse::<i64>() {
            if let Ok(case) = self.get_case_by_id(id) {
                return Ok(case);
            }
        }
        self.conn
            .query_row(
                "SELECT id, case_number, defendant_name, charges, court, status, opened_date, notes
                 FROM cases WHERE case_number = ?1",
                params![id_or_number],
                row_to_case,
            )
            .with_context(|| format!("no case matching '{id_or_number}'"))
    }

    /// Mark a case closed.
    pub fn close_case(&self, id_or_number: &str) -> Result<Case> {
        let case = self.find_case(id_or_number)?;
        self.conn.execute(
            "UPDATE cases SET status = 'closed' WHERE id = ?1",
            params![case.id],
        )?;
        self.get_case_by_id(case.id)
    }

    pub fn add_deadline(
        &self,
        case_id: i64,
        kind: &str,
        due_date: NaiveDate,
        notes: Option<&str>,
    ) -> Result<Deadline> {
        // Ensure the case exists.
        self.get_case_by_id(case_id)?;
        self.conn.execute(
            "INSERT INTO deadlines (case_id, kind, due_date, completed, notes)
             VALUES (?1, ?2, ?3, 0, ?4)",
            params![case_id, kind, due_date.to_string(), notes],
        )?;
        let id = self.conn.last_insert_rowid();
        self.get_deadline(id)
    }

    pub fn get_deadline(&self, id: i64) -> Result<Deadline> {
        self.conn
            .query_row(
                "SELECT id, case_id, kind, due_date, completed, notes
                 FROM deadlines WHERE id = ?1",
                params![id],
                row_to_deadline,
            )
            .with_context(|| format!("no deadline with id {id}"))
    }

    pub fn complete_deadline(&self, id: i64) -> Result<Deadline> {
        let changed = self.conn.execute(
            "UPDATE deadlines SET completed = 1 WHERE id = ?1",
            params![id],
        )?;
        if changed == 0 {
            return Err(anyhow!("no deadline with id {id}"));
        }
        self.get_deadline(id)
    }

    pub fn deadlines_for_case(&self, case_id: i64) -> Result<Vec<Deadline>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, case_id, kind, due_date, completed, notes
             FROM deadlines WHERE case_id = ?1 ORDER BY due_date",
        )?;
        let rows = stmt.query_map(params![case_id], row_to_deadline)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Upcoming deadlines across all cases: every uncompleted overdue
    /// deadline (due before today) plus uncompleted deadlines due within
    /// the next `days` days, sorted by due date.
    pub fn upcoming_deadlines(&self, days: i64) -> Result<Vec<UpcomingDeadline>> {
        let today = Local::now().date_naive();
        let horizon = today + Duration::days(days);
        let mut stmt = self.conn.prepare(
            "SELECT d.id, d.case_id, d.kind, d.due_date, d.completed, d.notes,
                    c.case_number, c.defendant_name
             FROM deadlines d
             JOIN cases c ON c.id = d.case_id
             WHERE d.completed = 0 AND d.due_date <= ?1
             ORDER BY d.due_date ASC, d.id ASC",
        )?;
        let rows = stmt.query_map(params![horizon.to_string()], |row| {
            let deadline = row_to_deadline(row)?;
            let case_number: String = row.get(6)?;
            let defendant_name: String = row.get(7)?;
            Ok((deadline, case_number, defendant_name))
        })?;
        let mut out = Vec::new();
        for r in rows {
            let (deadline, case_number, defendant_name) = r?;
            let due = NaiveDate::parse_from_str(&deadline.due_date, "%Y-%m-%d")
                .with_context(|| format!("bad due_date '{}'", deadline.due_date))?;
            out.push(UpcomingDeadline {
                overdue: due < today,
                deadline,
                case_number,
                defendant_name,
            });
        }
        Ok(out)
    }

    pub fn add_document(
        &self,
        case_id: i64,
        kind: &str,
        path: &str,
        notes: Option<&str>,
    ) -> Result<Document> {
        self.get_case_by_id(case_id)?;
        let added_date = Local::now().date_naive().to_string();
        self.conn.execute(
            "INSERT INTO documents (case_id, kind, path, added_date, notes)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![case_id, kind, path, added_date, notes],
        )?;
        let id = self.conn.last_insert_rowid();
        self.conn
            .query_row(
                "SELECT id, case_id, kind, path, added_date, notes
                 FROM documents WHERE id = ?1",
                params![id],
                row_to_document,
            )
            .map_err(Into::into)
    }

    pub fn documents_for_case(&self, case_id: i64) -> Result<Vec<Document>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, case_id, kind, path, added_date, notes
             FROM documents WHERE case_id = ?1 ORDER BY added_date, id",
        )?;
        let rows = stmt.query_map(params![case_id], row_to_document)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Search case numbers, defendant names, charges, and notes.
    pub fn search(&self, term: &str) -> Result<Vec<Case>> {
        let pattern = format!("%{}%", term.replace('%', "\\%").replace('_', "\\_"));
        let mut stmt = self.conn.prepare(
            "SELECT id, case_number, defendant_name, charges, court, status, opened_date, notes
             FROM cases
             WHERE case_number LIKE ?1 ESCAPE '\\'
                OR defendant_name LIKE ?1 ESCAPE '\\'
                OR charges LIKE ?1 ESCAPE '\\'
                OR notes LIKE ?1 ESCAPE '\\'
             ORDER BY case_number",
        )?;
        let rows = stmt.query_map(params![pattern], row_to_case)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Plain-text case summary for a court day. Overdue is the local date.
    /// Completed deadlines are never marked overdue.
    pub fn export_summary(&self, id_or_number: &str) -> Result<String> {
        let case = self.find_case(id_or_number)?;
        let today = Local::now().date_naive();
        let deadlines = self.deadlines_for_case(case.id)?;
        let docs = self.documents_for_case(case.id)?;

        let mut out = String::new();
        out.push_str(&format!("DOCKET — {}\n", case.case_number));
        out.push_str(&format!("Defendant: {}\n", case.defendant_name));
        out.push_str(&format!("Charges: {}\n", case.charges.trim()));
        out.push_str(&format!(
            "Court: {}\n",
            case.court.as_deref().unwrap_or("(none)")
        ));
        out.push_str(&format!("Status: {}\n", case.status));
        out.push_str(&format!("Opened: {}\n", case.opened_date));

        out.push_str("\nDeadlines\n");
        if deadlines.is_empty() {
            out.push_str("  (none)\n");
        }
        for d in &deadlines {
            let due = NaiveDate::parse_from_str(&d.due_date, "%Y-%m-%d")
                .with_context(|| format!("bad due_date '{}'", d.due_date))?;
            let flag = if d.completed {
                "DONE"
            } else if due < today {
                "OVERDUE"
            } else {
                ""
            };
            let mark = if d.completed { "[x]" } else { "[ ]" };
            out.push_str(&format!(
                "  {mark} #{:<4} {:<18} due {} {flag}\n",
                d.id, d.kind, d.due_date
            ));
            if let Some(notes) = &d.notes {
                out.push_str(&format!("       {notes}\n"));
            }
        }

        out.push_str("\nDocuments\n");
        if docs.is_empty() {
            out.push_str("  (none)\n");
        }
        for d in &docs {
            out.push_str(&format!(
                "  #{:<4} {:<15} {} (added {})\n",
                d.id, d.kind, d.path, d.added_date
            ));
        }

        if let Some(notes) = &case.notes {
            out.push_str(&format!("\nNotes\n  {notes}\n"));
        }

        Ok(out)
    }

    /// Overdue deadlines plus anything due within `days` of today.
    /// `days = 0` is overdue and due today. Completed deadlines are omitted.
    /// Empty means stay quiet — this is the cron/reminder path.
    pub fn reminder_lines(&self, days: i64) -> Result<Vec<String>> {
        let upcoming = self.upcoming_deadlines(days)?;
        Ok(upcoming
            .iter()
            .map(|item| {
                let flag = if item.overdue { "OVERDUE" } else { "DUE" };
                format!(
                    "{flag}  {}  {}  {}  {}",
                    item.deadline.due_date,
                    item.deadline.kind,
                    item.case_number,
                    item.defendant_name
                )
            })
            .collect())
    }
}

fn row_to_case(row: &rusqlite::Row) -> rusqlite::Result<Case> {
    Ok(Case {
        id: row.get(0)?,
        case_number: row.get(1)?,
        defendant_name: row.get(2)?,
        charges: row.get(3)?,
        court: row.get(4)?,
        status: row.get(5)?,
        opened_date: row.get(6)?,
        notes: row.get(7)?,
    })
}

fn row_to_deadline(row: &rusqlite::Row) -> rusqlite::Result<Deadline> {
    Ok(Deadline {
        id: row.get(0)?,
        case_id: row.get(1)?,
        kind: row.get(2)?,
        due_date: row.get(3)?,
        completed: row.get::<_, i64>(4)? != 0,
        notes: row.get(5)?,
    })
}

fn row_to_document(row: &rusqlite::Row) -> rusqlite::Result<Document> {
    Ok(Document {
        id: row.get(0)?,
        case_id: row.get(1)?,
        kind: row.get(2)?,
        path: row.get(3)?,
        added_date: row.get(4)?,
        notes: row.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    fn today() -> NaiveDate {
        Local::now().date_naive()
    }

    fn add_test_case(store: &Store, number: &str) -> Case {
        store
            .add_case(
                number,
                Some("J. Doe"),
                None,
                " misdemeanor theft",
                Some("County Court"),
                Some("assigned via conflict panel"),
            )
            .unwrap()
    }

    #[test]
    fn case_crud() {
        let store = Store::open_memory().unwrap();
        let case = add_test_case(&store, "CR-2026-001");
        assert_eq!(case.case_number, "CR-2026-001");
        assert_eq!(case.defendant_name, "J. Doe");
        assert_eq!(case.status, "open");

        // Read by id and by case number.
        let by_id = store.find_case(&case.id.to_string()).unwrap();
        let by_num = store.find_case("CR-2026-001").unwrap();
        assert_eq!(by_id, by_num);

        // List all, then filter by status.
        assert_eq!(store.list_cases(None).unwrap().len(), 1);
        assert_eq!(store.list_cases(Some("open")).unwrap().len(), 1);
        assert_eq!(store.list_cases(Some("closed")).unwrap().len(), 0);

        // Duplicate case number rejected.
        assert!(add_test_case_checked(&store, "CR-2026-001").is_err());
    }

    fn add_test_case_checked(store: &Store, number: &str) -> Result<Case> {
        store.add_case(number, Some("J. Doe"), None, "", None, None)
    }

    #[test]
    fn deadline_ordering_and_overdue() {
        let store = Store::open_memory().unwrap();
        let case = add_test_case(&store, "CR-2026-002");
        let past = today() - Duration::days(3);
        let soon = today() + Duration::days(5);
        let later = today() + Duration::days(30);

        store.add_deadline(case.id, "trial", later, None).unwrap();
        let overdue_dl = store
            .add_deadline(case.id, "discovery_cutoff", past, None)
            .unwrap();
        store
            .add_deadline(case.id, "arraignment", soon, None)
            .unwrap();

        let upcoming = store.upcoming_deadlines(14).unwrap();
        // Sorted ascending; the 30-day-out trial is beyond the window.
        assert_eq!(upcoming.len(), 2);
        assert_eq!(upcoming[0].deadline.id, overdue_dl.id);
        assert!(upcoming[0].overdue);
        assert!(!upcoming[1].overdue);
        assert_eq!(upcoming[1].deadline.kind, "arraignment");

        // Wider window picks up the trial too, still sorted.
        let wide = store.upcoming_deadlines(60).unwrap();
        assert_eq!(wide.len(), 3);
        assert_eq!(wide[2].deadline.kind, "trial");

        // Completing the overdue one removes it from the list.
        store.complete_deadline(overdue_dl.id).unwrap();
        assert!(store.get_deadline(overdue_dl.id).unwrap().completed);
        let after = store.upcoming_deadlines(60).unwrap();
        assert_eq!(after.len(), 2);
        assert!(after.iter().all(|u| !u.overdue));
    }

    #[test]
    fn search_hits() {
        let store = Store::open_memory().unwrap();
        add_test_case(&store, "CR-2026-100");
        store
            .add_case(
                "CR-2026-200",
                Some("A. Smith"),
                None,
                "felony burglary",
                None,
                None,
            )
            .unwrap();

        // By case number fragment.
        assert_eq!(store.search("2026-100").unwrap().len(), 1);
        // By defendant name.
        let hits = store.search("Smith").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].case_number, "CR-2026-200");
        // By charges.
        assert_eq!(store.search("burglary").unwrap().len(), 1);
        // By notes.
        assert_eq!(store.search("conflict panel").unwrap().len(), 1);
        // No match.
        assert_eq!(store.search("nonexistent-zzz").unwrap().len(), 0);
    }

    #[test]
    fn pseudonym_stored_instead_of_name() {
        let store = Store::open_memory().unwrap();
        let case = store
            .add_case(
                "CR-2026-300",
                Some("Real Name"),
                Some("Client R.N."),
                "assault",
                None,
                None,
            )
            .unwrap();
        assert_eq!(case.defendant_name, "Client R.N.");
        // The real name must not be searchable anywhere.
        assert!(store.search("Real Name").unwrap().is_empty());

        // Pseudonym alone is fine.
        let case2 = store
            .add_case("CR-2026-301", None, Some("Client X"), "dui", None, None)
            .unwrap();
        assert_eq!(case2.defendant_name, "Client X");

        // Neither name nor pseudonym is an error.
        assert!(store
            .add_case("CR-2026-302", None, None, "dui", None, None)
            .is_err());
    }

    #[test]
    fn close_sets_status() {
        let store = Store::open_memory().unwrap();
        let case = add_test_case(&store, "CR-2026-400");
        assert_eq!(case.status, "open");

        let closed = store.close_case("CR-2026-400").unwrap();
        assert_eq!(closed.status, "closed");
        assert_eq!(store.list_cases(Some("open")).unwrap().len(), 0);
        assert_eq!(store.list_cases(Some("closed")).unwrap().len(), 1);

        // Closing by numeric id works too.
        let case2 = add_test_case(&store, "CR-2026-401");
        let closed2 = store.close_case(&case2.id.to_string()).unwrap();
        assert_eq!(closed2.status, "closed");
    }

    #[test]
    fn documents_round_trip() {
        let store = Store::open_memory().unwrap();
        let case = add_test_case(&store, "CR-2026-500");
        let doc = store
            .add_document(case.id, "discovery", "/files/police-report.pdf", None)
            .unwrap();
        assert_eq!(doc.kind, "discovery");
        let docs = store.documents_for_case(case.id).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].path, "/files/police-report.pdf");
    }

    #[test]
    fn export_summary_marks_overdue_and_done() {
        let store = Store::open_memory().unwrap();
        let case = store
            .add_case(
                "CR-2026-900",
                None,
                Some("Client J.D."),
                "possession w/ intent",
                Some("Superior Court, Dept. 4"),
                Some("bring the file"),
            )
            .unwrap();
        let yesterday = Local::now().date_naive() - Duration::days(1);
        let tomorrow = Local::now().date_naive() + Duration::days(1);
        let overdue = store
            .add_deadline(case.id, "arraignment", yesterday, None)
            .unwrap();
        let upcoming = store
            .add_deadline(case.id, "trial", tomorrow, None)
            .unwrap();
        store.complete_deadline(upcoming.id).unwrap();
        store
            .add_document(case.id, "discovery", "./police-report.pdf", None)
            .unwrap();

        let text = store.export_summary("CR-2026-900").unwrap();
        assert!(text.contains("DOCKET — CR-2026-900"));
        assert!(text.contains("Defendant: Client J.D."));
        assert!(text.contains("Real Name") == false);
        assert!(text.contains(&format!("#{} ", overdue.id)));
        assert!(text.contains("OVERDUE"));
        assert!(text.contains("DONE"));
        assert!(text.contains("./police-report.pdf"));
        assert!(text.contains("bring the file"));
        // A completed future deadline must not also be flagged overdue.
        let done_line = text.lines().find(|line| line.contains("trial")).unwrap();
        assert!(done_line.contains("DONE"));
        assert!(!done_line.contains("OVERDUE"));
    }

    #[test]
    fn reminder_is_quiet_unless_due() {
        let store = Store::open_memory().unwrap();
        let case = store
            .add_case(
                "CR-2026-910",
                None,
                Some("Client J.D."),
                "petty theft",
                None,
                None,
            )
            .unwrap();
        store
            .add_deadline(case.id, "trial", today() + Duration::days(20), None)
            .unwrap();
        assert!(store.reminder_lines(0).unwrap().is_empty());

        store
            .add_deadline(case.id, "arraignment", today() - Duration::days(2), None)
            .unwrap();
        store
            .add_deadline(case.id, "hearing", today(), None)
            .unwrap();
        let done = store
            .add_deadline(case.id, "filing", today() - Duration::days(1), None)
            .unwrap();
        store.complete_deadline(done.id).unwrap();

        let lines = store.reminder_lines(0).unwrap();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("OVERDUE"));
        assert!(lines[0].contains("arraignment"));
        assert!(lines[0].contains("Client J.D."));
        assert!(lines[1].starts_with("DUE"));
        assert!(lines[1].contains("hearing"));
        assert!(!lines.iter().any(|line| line.contains("filing")));
        assert!(!lines.iter().any(|line| line.contains("trial")));
    }
}
