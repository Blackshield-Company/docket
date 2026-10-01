//! Docket desktop. Same local store as the CLI. Nothing leaves the machine.

use chrono::NaiveDate;
use docket::{render_pdf, Case, Deadline, Document, Store};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::Manager;

struct Desk {
    store: Store,
    db_path: PathBuf,
}

#[derive(Serialize)]
struct CaseView {
    case: Case,
    deadlines: Vec<Deadline>,
    documents: Vec<Document>,
    summary: String,
}

fn lock(state: &Mutex<Desk>) -> Result<std::sync::MutexGuard<'_, Desk>, String> {
    state.lock().map_err(|e| e.to_string())
}

#[tauri::command]
fn db_path(state: tauri::State<'_, Mutex<Desk>>) -> Result<String, String> {
    Ok(lock(&state)?.db_path.display().to_string())
}

#[tauri::command]
fn list_cases(state: tauri::State<'_, Mutex<Desk>>) -> Result<Vec<Case>, String> {
    lock(&state)?
        .store
        .list_cases(None)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn search_cases(state: tauri::State<'_, Mutex<Desk>>, term: String) -> Result<Vec<Case>, String> {
    lock(&state)?.store.search(&term).map_err(|e| e.to_string())
}

#[tauri::command]
fn case_view(state: tauri::State<'_, Mutex<Desk>>, id: i64) -> Result<CaseView, String> {
    let desk = lock(&state)?;
    let case = desk.store.get_case_by_id(id).map_err(|e| e.to_string())?;
    let deadlines = desk
        .store
        .deadlines_for_case(id)
        .map_err(|e| e.to_string())?;
    let documents = desk
        .store
        .documents_for_case(id)
        .map_err(|e| e.to_string())?;
    let summary = desk
        .store
        .export_summary(&case.case_number)
        .map_err(|e| e.to_string())?;
    Ok(CaseView {
        case,
        deadlines,
        documents,
        summary,
    })
}

#[tauri::command]
fn add_case(
    state: tauri::State<'_, Mutex<Desk>>,
    case_number: String,
    pseudonym: String,
    charges: String,
    court: Option<String>,
    notes: Option<String>,
) -> Result<Case, String> {
    let desk = lock(&state)?;
    desk.store
        .add_case(
            case_number.trim(),
            None,
            Some(pseudonym.trim()),
            charges.trim(),
            court.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            notes.as_deref().map(str::trim).filter(|s| !s.is_empty()),
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn close_case(state: tauri::State<'_, Mutex<Desk>>, id: i64) -> Result<Case, String> {
    let desk = lock(&state)?;
    desk.store
        .close_case(&id.to_string())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn add_deadline(
    state: tauri::State<'_, Mutex<Desk>>,
    case_id: i64,
    kind: String,
    due_date: String,
    notes: Option<String>,
) -> Result<Deadline, String> {
    let due = NaiveDate::parse_from_str(due_date.trim(), "%Y-%m-%d")
        .map_err(|_| format!("due date must be YYYY-MM-DD, got {due_date}"))?;
    let desk = lock(&state)?;
    desk.store
        .add_deadline(
            case_id,
            kind.trim(),
            due,
            notes.as_deref().map(str::trim).filter(|s| !s.is_empty()),
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn complete_deadline(state: tauri::State<'_, Mutex<Desk>>, id: i64) -> Result<Deadline, String> {
    lock(&state)?
        .store
        .complete_deadline(id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn reminders(state: tauri::State<'_, Mutex<Desk>>, days: i64) -> Result<Vec<String>, String> {
    lock(&state)?
        .store
        .reminder_lines(days)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn save_pdf(state: tauri::State<'_, Mutex<Desk>>, id: i64) -> Result<String, String> {
    let desk = lock(&state)?;
    let case = desk.store.get_case_by_id(id).map_err(|e| e.to_string())?;
    let summary = desk
        .store
        .export_summary(&case.case_number)
        .map_err(|e| e.to_string())?;
    let dir = desk
        .db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("exports");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let safe: String = case
        .case_number
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let path = dir.join(format!("{safe}.pdf"));
    fs::write(&path, render_pdf(&summary)).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            fs::create_dir_all(&dir)?;
            let db_path = dir.join("docket.db");
            let store = Store::open(&db_path)?;
            app.manage(Mutex::new(Desk { store, db_path }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            db_path,
            list_cases,
            search_cases,
            case_view,
            add_case,
            close_case,
            add_deadline,
            complete_deadline,
            reminders,
            save_pdf
        ])
        .run(tauri::generate_context!())
        .expect("error while running docket");
}
