use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use std::collections::HashMap;
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

#[derive(Debug, Clone)]
pub struct Statistics {
    pub total_notes: i64,
    pub total_chars: i64,
    pub avg_note_length: f64,
    pub tag_count: i64,
    pub top_tags: Vec<(String, i64)>,
    pub top_sources: Vec<(String, i64)>,
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
        db.configure_connection(true)?;
        db.initialize_tables()?;
        db.migrate()?;
        db.initialize_fts_and_indexes()?;
        Ok(db)
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.configure_connection(false)?;
        db.initialize_tables()?;
        db.migrate()?;
        db.initialize_fts_and_indexes()?;
        Ok(db)
    }

    fn configure_connection(&self, is_file_backed: bool) -> Result<()> {
        self.conn
            .execute_batch("PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")?;
        if is_file_backed {
            self.conn.pragma_update(None, "journal_mode", "WAL")?;
        }
        Ok(())
    }

    fn initialize_tables(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS notes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                body TEXT NOT NULL,
                title TEXT,
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

        Ok(())
    }

    fn migrate(&self) -> Result<()> {
        // Add title column if it doesn't exist (v0.1/v0.2 -> v0.3+)
        let has_title = self.conn.query_row(
            "SELECT count(*) FROM pragma_table_info('notes') WHERE name='title'",
            [],
            |row| row.get::<_, i32>(0),
        )?;

        if has_title == 0 {
            self.conn
                .execute("ALTER TABLE notes ADD COLUMN title TEXT", [])?;
        }

        // Rebuild legacy FTS5 table/triggers if notes_fts predates the title column
        let fts_exists: i32 = self.conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='notes_fts'",
            [],
            |row| row.get(0),
        )?;

        if fts_exists > 0 {
            let fts_has_title: i32 = self.conn.query_row(
                "SELECT count(*) FROM pragma_table_info('notes_fts') WHERE name='title'",
                [],
                |row| row.get(0),
            )?;

            if fts_has_title == 0 {
                self.conn.execute_batch(
                    "DROP TRIGGER IF EXISTS notes_ai;
                     DROP TRIGGER IF EXISTS notes_ad;
                     DROP TRIGGER IF EXISTS notes_au;
                     DROP TABLE IF EXISTS notes_fts;",
                )?;
            }
        }

        Ok(())
    }

    fn initialize_fts_and_indexes(&self) -> Result<()> {
        let fts_existed: i32 = self.conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='notes_fts'",
            [],
            |row| row.get(0),
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
            END;
            CREATE INDEX IF NOT EXISTS idx_note_tags_tag_id ON note_tags(tag_id);
            CREATE INDEX IF NOT EXISTS idx_notes_updated ON notes(updated DESC);",
        )?;

        if fts_existed == 0 {
            let note_count: i64 = self
                .conn
                .query_row("SELECT count(*) FROM notes", [], |row| row.get(0))?;
            if note_count > 0 {
                self.conn
                    .execute("INSERT INTO notes_fts(notes_fts) VALUES('rebuild')", [])?;
            }
        }

        Ok(())
    }

    pub fn create_note(
        &self,
        body: &str,
        title: Option<&str>,
        source: &str,
        tags: &[String],
    ) -> Result<i64> {
        if body.len() > MAX_NOTE_SIZE {
            return Err(anyhow!(
                "Note body exceeds 1MB limit ({} bytes)",
                body.len()
            ));
        }

        let tx = self.conn.unchecked_transaction()?;
        let now = Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO notes (body, title, source, created, updated) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![body, title, source, now, now],
        )?;

        let note_id = tx.last_insert_rowid();

        for tag in tags {
            Self::add_tag_to_note_conn(&tx, note_id, tag)?;
        }

        tx.commit()?;
        Ok(note_id)
    }

    fn add_tag_to_note_conn(conn: &Connection, note_id: i64, tag_name: &str) -> Result<()> {
        let tag_name = tag_name.trim().to_lowercase();
        if tag_name.is_empty() {
            return Ok(());
        }

        conn.execute(
            "INSERT OR IGNORE INTO tags (name) VALUES (?1)",
            params![tag_name],
        )?;

        let tag_id: i64 = conn.query_row(
            "SELECT id FROM tags WHERE name = ?1",
            params![tag_name],
            |row| row.get(0),
        )?;

        conn.execute(
            "INSERT OR IGNORE INTO note_tags (note_id, tag_id) VALUES (?1, ?2)",
            params![note_id, tag_id],
        )?;

        Ok(())
    }

    fn prune_orphan_tags(conn: &Connection) -> Result<()> {
        conn.execute(
            "DELETE FROM tags WHERE id NOT IN (SELECT DISTINCT tag_id FROM note_tags)",
            [],
        )?;
        Ok(())
    }

    fn parse_timestamp(col: usize, raw: String) -> rusqlite::Result<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(&raw)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    col,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })
    }

    fn map_note_row(row: &rusqlite::Row) -> rusqlite::Result<Note> {
        let created_str: String = row.get(4)?;
        let updated_str: String = row.get(5)?;
        Ok(Note {
            id: row.get(0)?,
            body: row.get(1)?,
            title: row.get(2)?,
            source: row.get(3)?,
            created: Self::parse_timestamp(4, created_str)?,
            updated: Self::parse_timestamp(5, updated_str)?,
            tags: Vec::new(),
        })
    }

    fn populate_tags(&self, notes: &mut [Note]) -> Result<()> {
        if notes.is_empty() {
            return Ok(());
        }

        let mut tags_by_note: HashMap<i64, Vec<String>> = HashMap::with_capacity(notes.len());
        for chunk in notes.chunks(500) {
            let placeholders = std::iter::repeat_n("?", chunk.len())
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "SELECT nt.note_id, t.name
                 FROM note_tags nt
                 JOIN tags t ON nt.tag_id = t.id
                 WHERE nt.note_id IN ({})
                 ORDER BY t.name ASC",
                placeholders
            );
            let mut stmt = self.conn.prepare(&sql)?;
            let params = rusqlite::params_from_iter(chunk.iter().map(|n| n.id));
            let rows = stmt.query_map(params, |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (note_id, tag) = row?;
                tags_by_note.entry(note_id).or_default().push(tag);
            }
        }

        for note in notes {
            if let Some(tags) = tags_by_note.remove(&note.id) {
                note.tags = tags;
            }
        }
        Ok(())
    }

    pub fn get_note(&self, id: i64) -> Result<Note> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, body, title, source, created, updated FROM notes WHERE id = ?1")?;
        let note = stmt.query_row(params![id], Self::map_note_row)?;
        let mut notes = vec![note];
        self.populate_tags(&mut notes)?;
        Ok(notes.into_iter().next().expect("single queried note"))
    }

    pub fn list_notes(&self) -> Result<Vec<Note>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, body, title, source, created, updated FROM notes ORDER BY updated DESC",
        )?;
        let mut notes = stmt
            .query_map([], Self::map_note_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        self.populate_tags(&mut notes)?;
        Ok(notes)
    }

    pub fn delete_note(&self, id: i64) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let rows = tx.execute("DELETE FROM notes WHERE id = ?1", params![id])?;
        if rows == 0 {
            return Err(anyhow!("Note #{} not found.", id));
        }
        Self::prune_orphan_tags(&tx)?;
        tx.commit()?;
        Ok(())
    }

    pub fn search_notes(&self, query: &str) -> Result<Vec<Note>> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return self.list_notes();
        }

        let fts_terms: Vec<String> = trimmed
            .split_whitespace()
            .filter(|term| term.chars().any(|c| c.is_alphanumeric()))
            .map(|term| {
                let escaped = term.replace('"', "\"\"");
                format!("\"{}\"*", escaped)
            })
            .collect();

        if fts_terms.is_empty() {
            return Ok(Vec::new());
        }

        let ftsbox_query = fts_terms.join(" ");

        let mut stmt = self.conn.prepare(
            "SELECT id, body, title, source, created, updated FROM notes
             WHERE id IN (SELECT rowid FROM notes_fts WHERE notes_fts MATCH ?1)
             ORDER BY updated DESC",
        )?;
        let mut notes = stmt
            .query_map(params![ftsbox_query], Self::map_note_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        self.populate_tags(&mut notes)?;
        Ok(notes)
    }

    pub fn find_by_tag(&self, tag_name: &str) -> Result<Vec<Note>> {
        let normalized = tag_name.trim().to_lowercase();
        if normalized.is_empty() {
            return self.list_notes();
        }

        let escaped = normalized
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let prefix_pattern = format!("{}/%", escaped);

        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT n.id, n.body, n.title, n.source, n.created, n.updated FROM notes n
             JOIN note_tags nt ON n.id = nt.note_id
             JOIN tags t ON nt.tag_id = t.id
             WHERE t.name = ?1 OR t.name LIKE ?2 ESCAPE '\\'
             ORDER BY n.updated DESC",
        )?;

        let mut notes = stmt
            .query_map(params![normalized, prefix_pattern], Self::map_note_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        self.populate_tags(&mut notes)?;
        Ok(notes)
    }

    pub fn list_all_tags(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM tags ORDER BY name ASC")?;
        let tag_iter = stmt.query_map([], |row| row.get(0))?;
        let mut tags = Vec::new();
        for tag in tag_iter {
            tags.push(tag?);
        }
        Ok(tags)
    }

    pub fn update_note(
        &self,
        id: i64,
        body: &str,
        title: Option<&str>,
        tags: &[String],
    ) -> Result<()> {
        if body.len() > MAX_NOTE_SIZE {
            return Err(anyhow!(
                "Note body exceeds 1MB limit ({} bytes)",
                body.len()
            ));
        }

        let tx = self.conn.unchecked_transaction()?;
        let now = Utc::now().to_rfc3339();
        let rows = tx.execute(
            "UPDATE notes SET body = ?1, title = ?2, updated = ?3 WHERE id = ?4",
            params![body, title, now, id],
        )?;

        if rows == 0 {
            return Err(anyhow!("Note #{} not found.", id));
        }

        tx.execute("DELETE FROM note_tags WHERE note_id = ?1", params![id])?;
        for tag in tags {
            Self::add_tag_to_note_conn(&tx, id, tag)?;
        }
        Self::prune_orphan_tags(&tx)?;

        tx.commit()?;
        Ok(())
    }

    pub fn get_statistics(&self) -> Result<Statistics> {
        let (total_notes, total_chars, avg_note_length): (i64, i64, f64) = self.conn.query_row(
            "SELECT count(*), sum(length(body)), avg(length(body)) FROM notes",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    row.get::<_, Option<f64>>(2)?.unwrap_or(0.0),
                ))
            },
        )?;

        let tag_count: i64 = self
            .conn
            .query_row("SELECT count(*) FROM tags", [], |row| row.get(0))?;

        let mut stmt = self.conn.prepare(
            "SELECT t.name, count(nt.note_id) as count 
             FROM tags t 
             JOIN note_tags nt ON t.id = nt.tag_id 
             GROUP BY t.name 
             ORDER BY count DESC 
             LIMIT 5",
        )?;
        let top_tags = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;

        let mut stmt = self.conn.prepare(
            "SELECT source, count(*) as count 
             FROM notes 
             GROUP BY source 
             ORDER BY count DESC 
             LIMIT 5",
        )?;
        let top_sources = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Statistics {
            total_notes,
            total_chars,
            avg_note_length,
            tag_count,
            top_tags,
            top_sources,
        })
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
        assert_eq!(
            tags,
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
        Ok(())
    }

    #[test]
    fn test_get_statistics() -> Result<()> {
        let db = Database::in_memory()?;
        db.create_note("Note 1", None, "manual", &["work".to_string()])?;
        db.create_note(
            "Note 2",
            None,
            "tui",
            &["work".to_string(), "urgent".to_string()],
        )?;

        let stats = db.get_statistics()?;
        assert_eq!(stats.total_notes, 2);
        assert_eq!(stats.tag_count, 2);
        assert!(stats.total_chars > 0);
        assert!(stats.avg_note_length > 0.0);

        let work_tag = stats.top_tags.iter().find(|(t, _)| t == "work").unwrap();
        assert_eq!(work_tag.1, 2);

        Ok(())
    }

    #[test]
    fn test_delete_note_cascades_and_cleans_tags_and_fts() -> Result<()> {
        let db = Database::in_memory()?;
        let id = db.create_note(
            "Secret needle",
            Some("Title"),
            "manual",
            &["ephemeral".to_string()],
        )?;
        assert_eq!(db.search_notes("needle")?.len(), 1);
        assert_eq!(db.list_all_tags()?, vec!["ephemeral".to_string()]);

        db.delete_note(id)?;

        assert!(db.list_notes()?.is_empty());
        assert!(db.search_notes("needle")?.is_empty());
        assert!(db.list_all_tags()?.is_empty());
        let stats = db.get_statistics()?;
        assert_eq!(stats.total_notes, 0);
        assert_eq!(stats.tag_count, 0);
        assert!(stats.top_tags.is_empty());

        let orphan_links: i64 = db
            .conn
            .query_row("SELECT count(*) FROM note_tags", [], |r| r.get(0))?;
        assert_eq!(orphan_links, 0);
        Ok(())
    }

    #[test]
    fn test_find_by_tag_prefix_boundary_wildcards_and_distinct() -> Result<()> {
        let db = Database::in_memory()?;
        db.create_note(
            "Work note",
            None,
            "m",
            &["work/p1".to_string(), "work/p2".to_string()],
        )?;
        db.create_note("Workshop note", None, "m", &["workshop".to_string()])?;
        db.create_note("Literal percent", None, "m", &["100%".to_string()])?;
        db.create_note("Other", None, "m", &["1000".to_string()])?;

        let work_notes = db.find_by_tag("work")?;
        assert_eq!(work_notes.len(), 1);
        assert_eq!(work_notes[0].body, "Work note");

        let pct_notes = db.find_by_tag("100%")?;
        assert_eq!(pct_notes.len(), 1);
        assert_eq!(pct_notes[0].body, "Literal percent");
        Ok(())
    }

    #[test]
    fn test_search_notes_special_chars_empty_and_multiword() -> Result<()> {
        let db = Database::in_memory()?;
        db.create_note(
            "Rust SQLite FTS5 guide",
            Some("Database \"Notes\""),
            "m",
            &[],
        )?;
        db.create_note("Other entry", None, "m", &[])?;

        assert_eq!(db.search_notes("")?.len(), 2);
        assert_eq!(db.search_notes("   ")?.len(), 2);
        assert!(db.search_notes("?")?.is_empty());
        assert!(db.search_notes("---")?.is_empty());
        assert_eq!(db.search_notes("\"Notes\"")?.len(), 1);
        assert_eq!(db.search_notes("rust fts5")?.len(), 1);
        assert!(db.search_notes("OR")?.is_empty());
        Ok(())
    }

    #[test]
    fn test_legacy_v02_migration_rebuilds_fts_with_title() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE notes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                body TEXT NOT NULL,
                source TEXT NOT NULL,
                created TEXT NOT NULL,
                updated TEXT NOT NULL
            );
            CREATE TABLE tags (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE
            );
            CREATE TABLE note_tags (
                note_id INTEGER,
                tag_id INTEGER,
                PRIMARY KEY (note_id, tag_id)
            );
            CREATE VIRTUAL TABLE notes_fts USING fts5(
                body,
                content='notes',
                content_rowid='id'
            );
            CREATE TRIGGER notes_ai AFTER INSERT ON notes BEGIN
                INSERT INTO notes_fts(rowid, body) VALUES (new.id, new.body);
            END;
            INSERT INTO notes (body, source, created, updated)
            VALUES ('Legacy body content', 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');",
        )?;

        let db = Database { conn };
        db.configure_connection(false)?;
        db.initialize_tables()?;
        db.migrate()?;
        db.initialize_fts_and_indexes()?;

        assert_eq!(db.search_notes("Legacy")?.len(), 1);
        let new_id = db.create_note("Body", Some("MigratedTitle"), "manual", &[])?;
        let found = db.search_notes("MigratedTitle")?;
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, new_id);
        Ok(())
    }

    #[test]
    fn test_max_note_size_and_missing_id_errors_and_tag_dedup() -> Result<()> {
        let db = Database::in_memory()?;
        let oversized = "a".repeat(MAX_NOTE_SIZE + 1);
        assert!(db.create_note(&oversized, None, "manual", &[]).is_err());

        let id = db.create_note(
            "Valid",
            None,
            "manual",
            &["  Work ".to_string(), "work".to_string(), "".to_string()],
        )?;
        let note = db.get_note(id)?;
        assert_eq!(note.tags, vec!["work".to_string()]);

        assert!(db.update_note(id, &oversized, None, &[]).is_err());
        assert!(db.get_note(9999).is_err());
        assert!(db.update_note(9999, "x", None, &[]).is_err());
        assert!(db.delete_note(9999).is_err());
        Ok(())
    }
}
