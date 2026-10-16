//! SQLite persistence for the queue (docs/architecture.md, "Job queue and resume").
//!
//! One row per queued file, plus the error of each failed page. Everything that changes a row
//! goes through [`Db::update`], which reads, changes and writes under one lock, so the runner and
//! the commands cannot overwrite each other's changes.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::contract::{
    AppError, AttentionReason, EngineInfo, ExportFormat, FileKind, OutputFile, QueueStatus,
    SaveLocation, TextMode,
};
use crate::error::Result;

/// Each entry upgrades the schema by one version (`PRAGMA user_version`).
const MIGRATIONS: &[&str] = &["CREATE TABLE files (
        file_id        TEXT PRIMARY KEY,
        seq            INTEGER NOT NULL,
        path           TEXT NOT NULL,
        name           TEXT NOT NULL,
        kind           TEXT NOT NULL,
        size_bytes     INTEGER NOT NULL,
        page_count     INTEGER,
        has_text_layer INTEGER,
        preview_path   TEXT,
        status         TEXT NOT NULL,
        attention      TEXT,
        text_mode      TEXT,
        model_id       TEXT,
        formats        TEXT NOT NULL,
        save_to        TEXT NOT NULL,
        pages_done     INTEGER NOT NULL DEFAULT 0,
        duration_ms    INTEGER,
        outputs        TEXT NOT NULL DEFAULT '[]',
        error          TEXT,
        engine         TEXT,
        added_at       INTEGER NOT NULL
    );
    CREATE INDEX files_seq ON files(seq);
    CREATE TABLE page_errors (
        file_id TEXT NOT NULL,
        page    INTEGER NOT NULL,
        error   TEXT NOT NULL,
        PRIMARY KEY (file_id, page)
    );"];

#[derive(Debug, Clone, PartialEq)]
pub struct FileRow {
    pub file_id: String,
    pub seq: i64,
    pub path: String,
    pub name: String,
    pub kind: FileKind,
    pub size_bytes: u64,
    pub page_count: Option<u32>,
    pub has_text_layer: Option<bool>,
    pub preview_path: Option<String>,
    pub status: QueueStatus,
    pub attention: Option<AttentionReason>,
    pub text_mode: Option<TextMode>,
    pub model_id: Option<String>,
    pub formats: Vec<ExportFormat>,
    pub save_to: SaveLocation,
    pub pages_done: u32,
    pub duration_ms: Option<u64>,
    pub outputs: Vec<OutputFile>,
    pub error: Option<AppError>,
    pub engine: Option<EngineInfo>,
    pub added_at: i64,
}

fn to_text<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("contract types serialize")
}

fn from_text<T: DeserializeOwned>(text: &str) -> Option<T> {
    serde_json::from_str(text).ok()
}

/// Enums are stored as their bare camelCase names (no JSON quotes).
fn enum_text<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(text)) => text,
        _ => String::new(),
    }
}

fn enum_from<T: DeserializeOwned>(text: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(text.to_string())).ok()
}

impl FileRow {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        let status: String = row.get("status")?;
        let kind: String = row.get("kind")?;
        let attention: Option<String> = row.get("attention")?;
        let text_mode: Option<String> = row.get("text_mode")?;
        let formats: String = row.get("formats")?;
        let save_to: String = row.get("save_to")?;
        let outputs: String = row.get("outputs")?;
        let error: Option<String> = row.get("error")?;
        let engine: Option<String> = row.get("engine")?;
        let has_text: Option<i64> = row.get("has_text_layer")?;
        Ok(Self {
            file_id: row.get("file_id")?,
            seq: row.get("seq")?,
            path: row.get("path")?,
            name: row.get("name")?,
            kind: enum_from(&kind).unwrap_or(FileKind::Image),
            size_bytes: row.get::<_, i64>("size_bytes")? as u64,
            page_count: row.get::<_, Option<i64>>("page_count")?.map(|n| n as u32),
            has_text_layer: has_text.map(|v| v != 0),
            preview_path: row.get("preview_path")?,
            status: enum_from(&status).unwrap_or(QueueStatus::Failed),
            attention: attention.as_deref().and_then(enum_from),
            text_mode: text_mode.as_deref().and_then(enum_from),
            model_id: row.get("model_id")?,
            formats: from_text(&formats).unwrap_or_default(),
            save_to: from_text(&save_to).unwrap_or(SaveLocation::Original),
            pages_done: row.get::<_, i64>("pages_done")? as u32,
            duration_ms: row.get::<_, Option<i64>>("duration_ms")?.map(|n| n as u64),
            outputs: from_text(&outputs).unwrap_or_default(),
            error: error.as_deref().and_then(from_text),
            engine: engine.as_deref().and_then(from_text),
            added_at: row.get("added_at")?,
        })
    }
}

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Self::init(Connection::open(path)?)
    }

    pub fn open_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")
            .ok();
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn schema_version(&self) -> Result<i64> {
        let conn = self.conn.lock().expect("db lock");
        Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }

    pub fn insert(&self, row: &mut FileRow) -> Result<()> {
        let conn = self.conn.lock().expect("db lock");
        row.seq = conn.query_row("SELECT COALESCE(MAX(seq), 0) + 1 FROM files", [], |r| {
            r.get(0)
        })?;
        write_row(&conn, row, true)?;
        Ok(())
    }

    pub fn get(&self, file_id: &str) -> Result<Option<FileRow>> {
        let conn = self.conn.lock().expect("db lock");
        Ok(conn
            .query_row(
                "SELECT * FROM files WHERE file_id = ?1",
                params![file_id],
                FileRow::from_row,
            )
            .optional()?)
    }

    pub fn all(&self) -> Result<Vec<FileRow>> {
        let conn = self.conn.lock().expect("db lock");
        let mut statement = conn.prepare("SELECT * FROM files ORDER BY seq")?;
        let rows = statement.query_map([], FileRow::from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Reads the row, applies `change`, writes it back, all under the lock. Returns the new row,
    /// or `None` when the file is gone.
    pub fn update(
        &self,
        file_id: &str,
        change: impl FnOnce(&mut FileRow),
    ) -> Result<Option<FileRow>> {
        let conn = self.conn.lock().expect("db lock");
        let Some(mut row) = conn
            .query_row(
                "SELECT * FROM files WHERE file_id = ?1",
                params![file_id],
                FileRow::from_row,
            )
            .optional()?
        else {
            return Ok(None);
        };
        change(&mut row);
        write_row(&conn, &row, false)?;
        Ok(Some(row))
    }

    pub fn delete(&self, file_id: &str) -> Result<()> {
        let conn = self.conn.lock().expect("db lock");
        conn.execute(
            "DELETE FROM page_errors WHERE file_id = ?1",
            params![file_id],
        )?;
        conn.execute("DELETE FROM files WHERE file_id = ?1", params![file_id])?;
        Ok(())
    }

    pub fn set_page_error(&self, file_id: &str, page: u32, error: &AppError) -> Result<()> {
        let conn = self.conn.lock().expect("db lock");
        conn.execute(
            "INSERT OR REPLACE INTO page_errors (file_id, page, error) VALUES (?1, ?2, ?3)",
            params![file_id, page, to_text(error)],
        )?;
        Ok(())
    }

    pub fn clear_page_error(&self, file_id: &str, page: u32) -> Result<()> {
        let conn = self.conn.lock().expect("db lock");
        conn.execute(
            "DELETE FROM page_errors WHERE file_id = ?1 AND page = ?2",
            params![file_id, page],
        )?;
        Ok(())
    }

    pub fn clear_page_errors(&self, file_id: &str) -> Result<()> {
        let conn = self.conn.lock().expect("db lock");
        conn.execute(
            "DELETE FROM page_errors WHERE file_id = ?1",
            params![file_id],
        )?;
        Ok(())
    }

    pub fn page_errors(&self, file_id: &str) -> Result<HashMap<u32, AppError>> {
        let conn = self.conn.lock().expect("db lock");
        let mut statement =
            conn.prepare("SELECT page, error FROM page_errors WHERE file_id = ?1")?;
        let rows = statement.query_map(params![file_id], |r| {
            Ok((r.get::<_, u32>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut errors = HashMap::new();
        for entry in rows {
            let (page, text) = entry?;
            if let Some(error) = from_text(&text) {
                errors.insert(page, error);
            }
        }
        Ok(errors)
    }
}

fn migrate(conn: &Connection) -> Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        let transaction = conn.unchecked_transaction()?;
        transaction.execute_batch(sql)?;
        transaction.execute_batch(&format!("PRAGMA user_version = {}", index + 1))?;
        transaction.commit()?;
    }
    Ok(())
}

fn write_row(conn: &Connection, row: &FileRow, insert: bool) -> Result<()> {
    let verb = if insert {
        "INSERT"
    } else {
        "INSERT OR REPLACE"
    };
    conn.execute(
        &format!(
            "{verb} INTO files (file_id, seq, path, name, kind, size_bytes, page_count,
                has_text_layer, preview_path, status, attention, text_mode, model_id, formats,
                save_to, pages_done, duration_ms, outputs, error, engine, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                ?18, ?19, ?20, ?21)"
        ),
        params![
            row.file_id,
            row.seq,
            row.path,
            row.name,
            enum_text(&row.kind),
            row.size_bytes as i64,
            row.page_count,
            row.has_text_layer.map(i64::from),
            row.preview_path,
            enum_text(&row.status),
            row.attention.as_ref().map(enum_text),
            row.text_mode.as_ref().map(enum_text),
            row.model_id,
            to_text(&row.formats),
            to_text(&row.save_to),
            row.pages_done,
            row.duration_ms.map(|n| n as i64),
            to_text(&row.outputs),
            row.error.as_ref().map(to_text),
            row.engine.as_ref().map(to_text),
            row.added_at,
        ],
    )?;
    Ok(())
}

#[cfg(test)]
pub fn sample_row(file_id: &str, path: &str) -> FileRow {
    FileRow {
        file_id: file_id.to_string(),
        seq: 0,
        path: path.to_string(),
        name: Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        kind: FileKind::Pdf,
        size_bytes: 100,
        page_count: None,
        has_text_layer: None,
        preview_path: None,
        status: QueueStatus::Waiting,
        attention: None,
        text_mode: None,
        model_id: None,
        formats: vec![],
        save_to: SaveLocation::Original,
        pages_done: 0,
        duration_ms: None,
        outputs: vec![],
        error: None,
        engine: None,
        added_at: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::AppErrorCode;

    #[test]
    fn a_new_database_is_migrated_to_the_latest_version_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("q.sqlite3");
        let db = Db::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), MIGRATIONS.len() as i64);
        drop(db);
        // Opening again must not try to create the tables a second time.
        let db = Db::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), MIGRATIONS.len() as i64);
    }

    #[test]
    fn rows_round_trip_with_every_column() {
        let db = Db::open_memory().unwrap();
        let mut row = sample_row("f-1", "C:/docs/a.pdf");
        row.page_count = Some(7);
        row.has_text_layer = Some(true);
        row.preview_path = Some("p.png".into());
        row.status = QueueStatus::NeedsAttention;
        row.attention = Some(AttentionReason::HasTextLayer);
        row.text_mode = Some(TextMode::ExistingText);
        row.model_id = Some("m".into());
        row.formats = vec![ExportFormat::Docx, ExportFormat::Markdown];
        row.save_to = SaveLocation::Folder {
            path: "D:/out".into(),
        };
        row.pages_done = 3;
        row.duration_ms = Some(1234);
        row.outputs = vec![OutputFile {
            format: ExportFormat::Docx,
            path: "a.docx".into(),
            assets_dir: None,
        }];
        row.error = Some(AppError::new(AppErrorCode::BadInput, "x").with_reason("corrupt"));
        row.engine = Some(EngineInfo {
            id: "e".into(),
            model: "m".into(),
            version: "1".into(),
        });
        db.insert(&mut row).unwrap();
        assert_eq!(db.get("f-1").unwrap().unwrap(), row);
    }

    #[test]
    fn rows_come_back_in_insertion_order() {
        let db = Db::open_memory().unwrap();
        for name in ["c", "a", "b"] {
            db.insert(&mut sample_row(
                &format!("f-{name}"),
                &format!("{name}.pdf"),
            ))
            .unwrap();
        }
        let ids: Vec<String> = db.all().unwrap().into_iter().map(|r| r.file_id).collect();
        assert_eq!(ids, vec!["f-c", "f-a", "f-b"]);
    }

    #[test]
    fn update_changes_one_row_and_reports_missing_files() {
        let db = Db::open_memory().unwrap();
        db.insert(&mut sample_row("f-1", "a.pdf")).unwrap();
        db.insert(&mut sample_row("f-2", "b.pdf")).unwrap();
        let updated = db
            .update("f-1", |row| row.status = QueueStatus::Done)
            .unwrap()
            .unwrap();
        assert_eq!(updated.status, QueueStatus::Done);
        assert_eq!(db.get("f-2").unwrap().unwrap().status, QueueStatus::Waiting);
        assert!(db.update("nope", |_| {}).unwrap().is_none());
    }

    #[test]
    fn page_errors_are_kept_per_file_and_cleared_with_it() {
        let db = Db::open_memory().unwrap();
        db.insert(&mut sample_row("f-1", "a.pdf")).unwrap();
        let error = AppError::new(AppErrorCode::RecognitionFailed, "bad page");
        db.set_page_error("f-1", 4, &error).unwrap();
        assert_eq!(db.page_errors("f-1").unwrap().get(&4), Some(&error));
        db.clear_page_errors("f-1").unwrap();
        assert!(db.page_errors("f-1").unwrap().is_empty());
        db.set_page_error("f-1", 1, &error).unwrap();
        db.delete("f-1").unwrap();
        assert!(db.page_errors("f-1").unwrap().is_empty());
        assert!(db.get("f-1").unwrap().is_none());
    }
}
