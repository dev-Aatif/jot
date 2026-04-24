use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::path::PathBuf;

pub const MAX_NOTE_SIZE: usize = 1_048_576; // 1MB

#[derive(Debug, Clone)]
pub struct Note {
    pub id: i64,
    pub title: Option<String>,
    pub body: String,
    pub source: String,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    pub tags: Vec<String>,
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
        db.migrate()?;
        Ok(db)
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.initialize()?;
        db.migrate()?;
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

        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS tags (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS note_tags (
                note_id INTEGER,
                tag_id INTEGER,
                PRIMARY KEY (note_id, tag_id),
                FOREIGN KEY (note_id) REFERENCES notes(id) ON DELETE CASCADE,
                FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // FTS5 Virtual Table for searching
        self.conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
                body,
                title,
                content='notes',
                content_rowid='id'
            )",
            [],
        )?;

        // Triggers to keep FTS index in sync
        self.conn.execute_batch(
            "CREATE TRIGGER IF NOT EXISTS notes_ai AFTER INSERT ON notes BEGIN
                INSERT INTO notes_fts(rowid, body, title) VALUES (new.id, new.body, new.title);
            END;
            CREATE TRIGGER IF NOT EXISTS notes_ad AFTER DELETE ON notes BEGIN
                INSERT INTO notes_fts(notes_fts, rowid, body, title) VALUES('delete', old.id, old.body, old.title);
            END;
            CREATE TRIGGER IF NOT EXISTS notes_au AFTER UPDATE ON notes BEGIN
                INSERT INTO notes_fts(notes_fts, rowid, body, title) VALUES('delete', old.id, old.body, old.title);
                INSERT INTO notes_fts(rowid, body, title) VALUES (new.id, new.body, new.title);
            END;",
        )?;

        Ok(())
    }

    fn migrate(&self) -> Result<()> {
        // Add title column if it doesn't exist
        let has_title = self.conn.query_row(
            "SELECT count(*) FROM pragma_table_info('notes') WHERE name='title'",
            [],
            |row| row.get::<_, i32>(0),
        )?;

        if has_title == 0 {
            self.conn.execute("ALTER TABLE notes ADD COLUMN title TEXT", [])?;
        }

        Ok(())
    }

    pub fn create_note(&self, body: &str, title: Option<&str>, source: &str, tags: &[String]) -> Result<i64> {
        if body.len() > MAX_NOTE_SIZE {
            return Err(anyhow!("Note body exceeds 1MB limit ({} bytes)", body.len()));
        }

        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO notes (body, title, source, created, updated) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![body, title, source, now, now],
        )?;

        let note_id = self.conn.last_insert_rowid();
        
        for tag in tags {
            self.add_tag_to_note(note_id, tag)?;
        }

        Ok(note_id)
    }

    fn add_tag_to_note(&self, note_id: i64, tag_name: &str) -> Result<()> {
        let tag_name = tag_name.trim().to_lowercase();
        if tag_name.is_empty() { return Ok(()); }

        self.conn.execute(
            "INSERT OR IGNORE INTO tags (name) VALUES (?1)",
            params![tag_name],
        )?;

        let tag_id: i64 = self.conn.query_row(
            "SELECT id FROM tags WHERE name = ?1",
            params![tag_name],
            |row| row.get(0),
        )?;

        self.conn.execute(
            "INSERT OR IGNORE INTO note_tags (note_id, tag_id) VALUES (?1, ?2)",
            params![note_id, tag_id],
        )?;

        Ok(())
    }

    pub fn get_note(&self, id: i64) -> Result<Note> {
        let mut stmt = self.conn.prepare(
            "SELECT id, body, title, source, created, updated FROM notes WHERE id = ?1",
        )?;
        let mut note = stmt.query_row(params![id], |row| {
            let created_str: String = row.get(4)?;
            let updated_str: String = row.get(5)?;
            Ok(Note {
                id: row.get(0)?,
                body: row.get(1)?,
                title: row.get(2)?,
                source: row.get(3)?,
                created: DateTime::parse_from_rfc3339(&created_str)
                    .unwrap_or_default()
                    .with_timezone(&Utc),
                updated: DateTime::parse_from_rfc3339(&updated_str)
                    .unwrap_or_default()
                    .with_timezone(&Utc),
                tags: Vec::new(),
            })
        })?;

        note.tags = self.get_note_tags(id)?;
        Ok(note)
    }

    fn get_note_tags(&self, note_id: i64) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.name FROM tags t 
             JOIN note_tags nt ON t.id = nt.tag_id 
             WHERE nt.note_id = ?1",
        )?;
        let tag_iter = stmt.query_map(params![note_id], |row| row.get(0))?;
        
        let mut tags = Vec::new();
        for tag in tag_iter {
            tags.push(tag?);
        }
        Ok(tags)
    }

    pub fn list_notes(&self) -> Result<Vec<Note>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, body, title, source, created, updated FROM notes ORDER BY updated DESC",
        )?;
        let note_iter = stmt.query_map([], |row| {
            let created_str: String = row.get(4)?;
            let updated_str: String = row.get(5)?;
            Ok(Note {
                id: row.get(0)?,
                body: row.get(1)?,
                title: row.get(2)?,
                source: row.get(3)?,
                created: DateTime::parse_from_rfc3339(&created_str)
                    .unwrap_or_default()
                    .with_timezone(&Utc),
                updated: DateTime::parse_from_rfc3339(&updated_str)
                    .unwrap_or_default()
                    .with_timezone(&Utc),
                tags: Vec::new(),
            })
        })?;

        let mut notes = Vec::new();
        for note in note_iter {
            let mut n = note?;
            n.tags = self.get_note_tags(n.id)?;
            notes.push(n);
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
        let sanitized = query.replace("\"", "\"\"");
        let ftsbox_query = format!("\"{}\"*", sanitized);
        
        let mut stmt = self.conn.prepare(
            "SELECT id, body, title, source, created, updated FROM notes
             WHERE id IN (SELECT rowid FROM notes_fts WHERE notes_fts MATCH ?1)
             ORDER BY updated DESC",
        )?;
        let note_iter = stmt.query_map(params![ftsbox_query], |row| {
            let created_str: String = row.get(4)?;
            let updated_str: String = row.get(5)?;
            Ok(Note {
                id: row.get(0)?,
                body: row.get(1)?,
                title: row.get(2)?,
                source: row.get(3)?,
                created: DateTime::parse_from_rfc3339(&created_str)
                    .unwrap_or_default()
                    .with_timezone(&Utc),
                updated: DateTime::parse_from_rfc3339(&updated_str)
                    .unwrap_or_default()
                    .with_timezone(&Utc),
                tags: Vec::new(),
            })
        })?;

        let mut notes = Vec::new();
        for note in note_iter {
            let mut n = note?;
            n.tags = self.get_note_tags(n.id)?;
            notes.push(n);
        }
        Ok(notes)
    }

    pub fn find_by_tag(&self, tag_name: &str) -> Result<Vec<Note>> {
        let mut stmt = self.conn.prepare(
            "SELECT n.id, n.body, n.title, n.source, n.created, n.updated FROM notes n
             JOIN note_tags nt ON n.id = nt.note_id
             JOIN tags t ON nt.tag_id = t.id
             WHERE t.name LIKE ?1
             ORDER BY n.updated DESC",
        )?;
        
        let tag_query = format!("{}%", tag_name.to_lowercase());
        
        let note_iter = stmt.query_map(params![tag_query], |row| {
            let created_str: String = row.get(4)?;
            let updated_str: String = row.get(5)?;
            Ok(Note {
                id: row.get(0)?,
                body: row.get(1)?,
                title: row.get(2)?,
                source: row.get(3)?,
                created: DateTime::parse_from_rfc3339(&created_str)
                    .unwrap_or_default()
                    .with_timezone(&Utc),
                updated: DateTime::parse_from_rfc3339(&updated_str)
                    .unwrap_or_default()
                    .with_timezone(&Utc),
                tags: Vec::new(),
            })
        })?;

        let mut notes = Vec::new();
        for note in note_iter {
            let mut n = note?;
            n.tags = self.get_note_tags(n.id)?;
            notes.push(n);
        }
        Ok(notes)
    }

    pub fn list_all_tags(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT name FROM tags ORDER BY name ASC")?;
        let tag_iter = stmt.query_map([], |row| row.get(0))?;
        let mut tags = Vec::new();
        for tag in tag_iter {
            tags.push(tag?);
        }
        Ok(tags)
    }

    pub fn update_note(&self, id: i64, body: &str, title: Option<&str>, tags: &[String]) -> Result<()> {
        if body.len() > MAX_NOTE_SIZE {
            return Err(anyhow!("Note body exceeds 1MB limit ({} bytes)", body.len()));
        }

        let now = Utc::now().to_rfc3339();
        let rows = self.conn.execute(
            "UPDATE notes SET body = ?1, title = ?2, updated = ?3 WHERE id = ?4",
            params![body, title, now, id],
        )?;

        if rows == 0 {
            return Err(anyhow!("Note #{} not found.", id));
        }

        self.conn.execute("DELETE FROM note_tags WHERE note_id = ?1", params![id])?;
        for tag in tags {
            self.add_tag_to_note(id, tag)?;
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
        let id = db.create_note("Test body", Some("Title"), "manual", &["work".to_string()])?;
        let note = db.get_note(id)?;
        assert_eq!(note.body, "Test body");
        assert_eq!(note.title, Some("Title".to_string()));
        assert_eq!(note.source, "manual");
        assert_eq!(note.tags, vec!["work".to_string()]);
        Ok(())
    }

    #[test]
    fn test_hierarchical_tag_search() -> Result<()> {
        let db = Database::in_memory()?;
        db.create_note("Body 1", None, "m", &["work/p1".to_string()])?;
        db.create_note("Body 2", None, "m", &["work/p2".to_string()])?;
        db.create_note("Body 3", None, "m", &["personal".to_string()])?;

        let work_notes = db.find_by_tag("work")?;
        assert_eq!(work_notes.len(), 2);

        let p1_notes = db.find_by_tag("work/p1")?;
        assert_eq!(p1_notes.len(), 1);
        Ok(())
    }

    #[test]
    fn test_update_note_with_tags() -> Result<()> {
        let db = Database::in_memory()?;
        let id = db.create_note("Init", None, "m", &["old".to_string()])?;
        db.update_note(id, "New", Some("New Title"), &["new".to_string()])?;
        
        let note = db.get_note(id)?;
        assert_eq!(note.body, "New");
        assert_eq!(note.title, Some("New Title".to_string()));
        assert_eq!(note.tags, vec!["new".to_string()]);
        Ok(())
    }

    #[test]
    fn test_list_all_tags() -> Result<()> {
        let db = Database::in_memory()?;
        db.create_note("1", None, "m", &["b".to_string(), "a".to_string()])?;
        db.create_note("2", None, "m", &["c".to_string(), "a".to_string()])?;
        
        let tags = db.list_all_tags()?;
        assert_eq!(tags, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        Ok(())
    }
}

