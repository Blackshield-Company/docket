use anyhow::{Context, Result};
use chrono::NaiveDate;
use clap::{Parser, Subcommand};
use docket::Store;

#[derive(Parser)]
#[command(
    name = "docket",
    version,
    about = "Local-first case file organizer for public defenders"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Manage cases
    Case {
        #[command(subcommand)]
        command: CaseCommand,
    },
    /// Manage deadlines for a case
    Deadline {
        #[command(subcommand)]
        command: DeadlineCommand,
    },
    /// Show upcoming deadlines across all cases (overdue plus next N days)
    Deadlines {
        /// Days ahead to include (overdue items are always shown)
        #[arg(long, default_value_t = 14)]
        days: i64,
    },
    /// Manage documents linked to a case
    Doc {
        #[command(subcommand)]
        command: DocCommand,
    },
    /// Search case numbers, defendant names, charges, and notes
    Search {
        /// Search term
        term: String,
    },
    /// Close a case
    Close {
        /// Case ID or case number
        id_or_number: String,
    },
}

#[derive(Subcommand)]
enum CaseCommand {
    /// Add a new case
    Add {
        /// Case number
        #[arg(long)]
        number: String,
        /// Defendant's name (omit if using --pseudonym)
        #[arg(long)]
        defendant: Option<String>,
        /// Store this pseudonym INSTEAD of the real name (for screen sharing)
        #[arg(long)]
        pseudonym: Option<String>,
        /// Charges (free text)
        #[arg(long, default_value = "")]
        charges: String,
        /// Court
        #[arg(long)]
        court: Option<String>,
        /// Notes
        #[arg(long)]
        notes: Option<String>,
    },
    /// List cases
    List {
        /// Filter by status
        #[arg(long, value_parser = ["open", "closed"])]
        status: Option<String>,
    },
    /// Show a case with its deadlines and documents
    Show {
        /// Case ID or case number
        id_or_number: String,
    },
}

#[derive(Subcommand)]
enum DeadlineCommand {
    /// Add a deadline to a case
    Add {
        /// Case ID or case number
        #[arg(long)]
        case: String,
        /// Kind (e.g. arraignment, discovery_cutoff, pretrial, trial, filing)
        #[arg(long)]
        kind: String,
        /// Due date (YYYY-MM-DD)
        #[arg(long)]
        due: String,
        /// Notes
        #[arg(long)]
        notes: Option<String>,
    },
    /// Mark a deadline complete
    Done {
        /// Deadline ID
        id: i64,
    },
}

#[derive(Subcommand)]
enum DocCommand {
    /// Register a document for a case
    Add {
        /// Case ID or case number
        #[arg(long)]
        case: String,
        /// Kind (discovery, motion, correspondence, other)
        #[arg(long)]
        kind: String,
        /// Path to the file
        #[arg(long)]
        path: String,
        /// Notes
        #[arg(long)]
        notes: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let store = Store::open_default().context("could not open docket.db")?;

    match cli.command {
        Command::Case { command } => match command {
            CaseCommand::Add {
                number,
                defendant,
                pseudonym,
                charges,
                court,
                notes,
            } => {
                let case = store.add_case(
                    &number,
                    defendant.as_deref(),
                    pseudonym.as_deref(),
                    &charges,
                    court.as_deref(),
                    notes.as_deref(),
                )?;
                if pseudonym.is_some() {
                    println!(
                        "Added case #{} ({}) — pseudonym stored: {}",
                        case.id, case.case_number, case.defendant_name
                    );
                } else {
                    println!(
                        "Added case #{} ({}) — {}",
                        case.id, case.case_number, case.defendant_name
                    );
                }
            }
            CaseCommand::List { status } => {
                let cases = store.list_cases(status.as_deref())?;
                if cases.is_empty() {
                    println!("No cases found.");
                }
                for c in cases {
                    println!(
                        "#{:<4} {:<16} {:<24} [{:<6}] {}",
                        c.id,
                        c.case_number,
                        c.defendant_name,
                        c.status,
                        c.charges.trim()
                    );
                }
            }
            CaseCommand::Show { id_or_number } => {
                let case = store.find_case(&id_or_number)?;
                println!("Case #{} — {}", case.id, case.case_number);
                println!("  Defendant: {}", case.defendant_name);
                println!("  Charges:   {}", case.charges.trim());
                println!("  Court:     {}", case.court.as_deref().unwrap_or("(none)"));
                println!("  Status:    {}", case.status);
                println!("  Opened:    {}", case.opened_date);
                if let Some(n) = &case.notes {
                    println!("  Notes:     {n}");
                }
                let deadlines = store.deadlines_for_case(case.id)?;
                if !deadlines.is_empty() {
                    println!("  Deadlines:");
                    for d in deadlines {
                        let mark = if d.completed { "[x]" } else { "[ ]" };
                        println!(
                            "    {} #{:<4} {:<18} due {}",
                            mark, d.id, d.kind, d.due_date
                        );
                    }
                }
                let docs = store.documents_for_case(case.id)?;
                if !docs.is_empty() {
                    println!("  Documents:");
                    for d in docs {
                        println!(
                            "    #{:<4} {:<15} {} (added {})",
                            d.id, d.kind, d.path, d.added_date
                        );
                    }
                }
            }
        },
        Command::Deadline { command } => match command {
            DeadlineCommand::Add {
                case,
                kind,
                due,
                notes,
            } => {
                let case = store.find_case(&case)?;
                let due = NaiveDate::parse_from_str(&due, "%Y-%m-%d")
                    .context("--due must be YYYY-MM-DD")?;
                let dl = store.add_deadline(case.id, &kind, due, notes.as_deref())?;
                println!(
                    "Added deadline #{} ({}) due {} on case {}",
                    dl.id, dl.kind, dl.due_date, case.case_number
                );
            }
            DeadlineCommand::Done { id } => {
                let dl = store.complete_deadline(id)?;
                println!("Completed deadline #{} ({})", dl.id, dl.kind);
            }
        },
        Command::Deadlines { days } => {
            let upcoming = store.upcoming_deadlines(days)?;
            if upcoming.is_empty() {
                println!("No overdue or upcoming deadlines in the next {days} days.");
            }
            for u in upcoming {
                let flag = if u.overdue { " ** OVERDUE **" } else { "" };
                println!(
                    "{}  {:<18} {:<16} {:<24}{}",
                    u.deadline.due_date, u.deadline.kind, u.case_number, u.defendant_name, flag
                );
            }
        }
        Command::Doc { command } => match command {
            DocCommand::Add {
                case,
                kind,
                path,
                notes,
            } => {
                let case = store.find_case(&case)?;
                let doc = store.add_document(case.id, &kind, &path, notes.as_deref())?;
                println!(
                    "Added document #{} ({}) to case {}: {}",
                    doc.id, doc.kind, case.case_number, doc.path
                );
            }
        },
        Command::Search { term } => {
            let hits = store.search(&term)?;
            if hits.is_empty() {
                println!("No cases matching '{term}'.");
            }
            for c in hits {
                println!(
                    "#{:<4} {:<16} {:<24} [{:<6}] {}",
                    c.id,
                    c.case_number,
                    c.defendant_name,
                    c.status,
                    c.charges.trim()
                );
            }
        }
        Command::Close { id_or_number } => {
            let case = store.close_case(&id_or_number)?;
            println!("Closed case #{} ({})", case.id, case.case_number);
        }
    }
    Ok(())
}
