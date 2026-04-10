use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::path::PathBuf;

pub const MAX_NOTE_SIZE: usize = 1_048_576; // 1MB

#[derive(Debug)]
pub struct Note {
    pub id: i64,
    pub body: String,
    pub source: String,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new(path: PathBuf) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.initialize()?;
        Ok(db)
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.initialize()?;
        Ok(db)
    }

    fn initialize(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS notes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                body TEXT NOT NULL,
                source TEXT NOT NULL,
                created TEXT NOT NULL,
                updated TEXT NOT NULL
            )",
            [],
        )?;

        // FTS5 Virtual Table for searching
        self.conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
                body,
                content='notes',
                content_rowid='id'
            )",
            [],
        )?;

        // Triggers to keep FTS index in sync
        self.conn.execute_batch(
            "CREATE TRIGGER IF NOT EXISTS notes_ai AFTER INSERT ON notes BEGIN
                INSERT INTO notes_fts(rowid, body) VALUES (new.id, new.body);
            END;
            CREATE TRIGGER IF NOT EXISTS notes_ad AFTER DELETE ON notes BEGIN
                INSERT INTO notes_fts(notes_fts, rowid, body) VALUES('delete', old.id, old.body);
            END;
            CREATE TRIGGER IF NOT EXISTS notes_au AFTER UPDATE ON notes BEGIN
                INSERT INTO notes_fts(notes_fts, rowid, body) VALUES('delete', old.id, old.body);
                INSERT INTO notes_fts(rowid, body) VALUES (new.id, new.body);
            END;",
        )?;

        Ok(())
    }

    pub fn create_note(&self, body: &str, source: &str) -> Result<i64> {
        if body.len() > MAX_NOTE_SIZE {
            return Err(anyhow!("Note body exceeds 1MB limit ({} bytes)", body.len()));
        }

        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO notes (body, source, created, updated) VALUES (?1, ?2, ?3, ?4)",
            params![body, source, now, now],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_note(&self, id: i64) -> Result<Note> {
        let mut stmt = self.conn.prepare(
            "SELECT id, body, source, created, updated FROM notes WHERE id = ?1",
        )?;
        let note = stmt.query_row(params![id], |row| {
            let created_str: String = row.get(3)?;
            let updated_str: String = row.get(4)?;
            Ok(Note {
                id: row.get(0)?,
                body: row.get(1)?,
                source: row.get(2)?,
                created: DateTime::parse_from_rfc3339(&created_str)
                    .unwrap_or_default()
                    .with_timezone(&Utc),
                updated: DateTime::parse_from_rfc3339(&updated_str)
                    .unwrap_or_default()
                    .with_timezone(&Utc),
            })
        })?;

        Ok(note)
    }

    pub fn list_notes(&self) -> Result<Vec<Note>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, body, source, created, updated FROM notes ORDER BY updated DESC",
        )?;
        let note_iter = stmt.query_map([], |row| {
            let created_str: String = row.get(3)?;
            let updated_str: String = row.get(4)?;
            Ok(Note {
                id: row.get(0)?,
                body: row.get(1)?,
                source: row.get(2)?,
                created: DateTime::parse_from_rfc3339(&created_str)
                    .unwrap_or_default()
                    .with_timezone(&Utc),
                updated: DateTime::parse_from_rfc3339(&updated_str)
                    .unwrap_or_default()
                    .with_timezone(&Utc),
            })
        })?;

        let mut notes = Vec::new();
        for note in note_iter {
            notes.push(note?);
        }
        Ok(notes)
    }

    pub fn delete_note(&self, id: i64) -> Result<()> {
        let rows = self.conn.execute("DELETE FROM notes WHERE id = ?1", params![id])?;
        if rows == 0 {
            return Err(anyhow!("Note #{} not found.", id));
        }
        Ok(())
    }

    pub fn search_notes(&self, query: &str) -> Result<Vec<Note>> {
        // Sanitize search query to prevent FTS5 syntax errors
        // We wrap the term in quotes for a more literal search
        let sanitized = query.replace("\"", "\"\"");
        let ftsbox_query = format!("\"{}\"*", sanitized);
        
        let mut stmt = self.conn.prepare(
            "SELECT id, body, source, created, updated FROM notes
             WHERE id IN (SELECT rowid FROM notes_fts WHERE body MATCH ?1)
             ORDER BY updated DESC",
        )?;
        let note_iter = stmt.query_map(params![ftsbox_query], |row| {
            let created_str: String = row.get(3)?;
            let updated_str: String = row.get(4)?;
            Ok(Note {
                id: row.get(0)?,
                body: row.get(1)?,
                source: row.get(2)?,
                created: DateTime::parse_from_rfc3339(&created_str)
                    .unwrap_or_default()
                    .with_timezone(&Utc),
                updated: DateTime::parse_from_rfc3339(&updated_str)
                    .unwrap_or_default()
                    .with_timezone(&Utc),
            })
        })?;

        let mut notes = Vec::new();
        for note in note_iter {
            notes.push(note?);
        }
        Ok(notes)
    }

    pub fn update_note(&self, id: i64, body: &str) -> Result<()> {
        if body.len() > MAX_NOTE_SIZE {
            return Err(anyhow!("Note body exceeds 1MB limit ({} bytes)", body.len()));
        }

        let now = Utc::now().to_rfc3339();
        let rows = self.conn.execute(
            "UPDATE notes SET body = ?1, updated = ?2 WHERE id = ?3",
            params![body, now, id],
        )?;

        if rows == 0 {
            return Err(anyhow!("Note #{} not found.", id));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_initialization() -> Result<()> {
        let _db = Database::in_memory()?;
        Ok(())
    }

    #[test]
    fn test_create_and_get_note() -> Result<()> {
        let db = Database::in_memory()?;
        let id = db.create_note("Test body", "manual")?;
        let note = db.get_note(id)?;
        assert_eq!(note.body, "Test body");
        assert_eq!(note.source, "manual");
        Ok(())
    }

    #[test]
    fn test_note_size_limit() -> Result<()> {
        let db = Database::in_memory()?;
        let huge_body = "a".repeat(MAX_NOTE_SIZE + 1);
        let result = db.create_note(&huge_body, "manual");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exceeds 1MB"));
        Ok(())
    }

    #[test]
    fn test_search_fts() -> Result<()> {
        let db = Database::in_memory()?;
        db.create_note("Rust is great", "manual")?;
        db.create_note("SQLite is fast", "manual")?;
        db.create_note("Developing apps", "manual")?;

        let results = db.search_notes("Rust")?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].body, "Rust is great");

        let results = db.search_notes("fast")?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].body, "SQLite is fast");
        Ok(())
    }

    #[test]
    fn test_update_note() -> Result<()> {
        let db = Database::in_memory()?;
        let id = db.create_note("Initial content", "manual")?;
        db.update_note(id, "Updated content")?;
        let note = db.get_note(id)?;
        assert_eq!(note.body, "Updated content");
        Ok(())
    }

    #[test]
    fn test_delete_note() -> Result<()> {
        let db = Database::in_memory()?;
        let id = db.create_note("To be deleted", "manual")?;
        db.delete_note(id)?;
        let result = db.get_note(id);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_search_special_chars() -> Result<()> {
        let db = Database::in_memory()?;
        db.create_note("Specials: @#$%^&*", "manual")?;
        
        // Test searching for symbols - FTS5 simple tokenizer might return 0 results, 
        // but it MUST NOT crash.
        let results = db.search_notes("@#$");
        assert!(results.is_ok()); 
        
        // Test a query with quotes that would normally cause a syntax error
        let results = db.search_notes("\"*--");
        assert!(results.is_ok()); 
        Ok(())
    }
}
