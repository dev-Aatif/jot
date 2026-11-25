use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use serde::{Serialize, Serializer};
use std::collections::HashMap;
use std::path::PathBuf;

pub const MAX_NOTE_SIZE: usize = 1_048_576; // 1MB

fn serialize_rfc3339<S>(dt: &DateTime<Utc>, serializer: S) -> std::result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&dt.to_rfc3339())
}

#[derive(Debug, Clone, Serialize)]
pub struct Note {
    pub id: i64,
    pub title: Option<String>,
    pub body: String,
    pub source: String,
    #[serde(serialize_with = "serialize_rfc3339")]
    pub created: DateTime<Utc>,
    #[serde(serialize_with = "serialize_rfc3339")]
    pub updated: DateTime<Utc>,
    pub pinned: bool,
    pub archived: bool,
    pub is_encrypted: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NoteSummary {
    pub id: i64,
    pub title: Option<String>,
    pub preview: String,
    pub char_len: usize,
    pub source: String,
    #[serde(serialize_with = "serialize_rfc3339")]
    pub created: DateTime<Utc>,
    #[serde(serialize_with = "serialize_rfc3339")]
    pub updated: DateTime<Utc>,
    pub pinned: bool,
    pub archived: bool,
    pub is_encrypted: bool,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct ListFilter<'a> {
    pub tag: Option<&'a str>,
    pub source: Option<&'a str>,
    pub include_archived: bool,
    pub query: Option<&'a str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Statistics {
    pub total_notes: i64,
    pub pinned_count: i64,
    pub archived_count: i64,
    pub total_chars: i64,
    pub avg_note_length: f64,
    pub tag_count: i64,
    pub top_tags: Vec<(String, i64)>,
    pub top_sources: Vec<(String, i64)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DbHealth {
    pub integrity_ok: bool,
    pub integrity_message: String,
    pub notes_count: i64,
    pub plaintext_notes_count: i64,
    pub fts_rows_count: i64,
    pub fts_in_sync: bool,
    pub journal_mode: String,
    pub foreign_keys: bool,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new(path: PathBuf) -> Result<Self> {
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
                updated TEXT NOT NULL,
                pinned INTEGER NOT NULL DEFAULT 0,
                archived INTEGER NOT NULL DEFAULT 0,
                is_encrypted INTEGER NOT NULL DEFAULT 0
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

    fn has_column(&self, table: &str, col: &str) -> Result<bool> {
        let sql = format!(
            "SELECT count(*) FROM pragma_table_info('{}') WHERE name = ?1",
            table
        );
        let count: i32 = self.conn.query_row(&sql, params![col], |row| row.get(0))?;
        Ok(count > 0)
    }

    fn migrate(&self) -> Result<()> {
        // Add title column if it doesn't exist (v0.1/v0.2 -> v0.3+)
        if !self.has_column("notes", "title")? {
            self.conn
                .execute("ALTER TABLE notes ADD COLUMN title TEXT", [])?;
        }

        // Add v0.5.0 columns if missing (v0.3/v0.4 -> v0.5.0)
        if !self.has_column("notes", "pinned")? {
            self.conn.execute(
                "ALTER TABLE notes ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        if !self.has_column("notes", "archived")? {
            self.conn.execute(
                "ALTER TABLE notes ADD COLUMN archived INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        if !self.has_column("notes", "is_encrypted")? {
            self.conn.execute(
                "ALTER TABLE notes ADD COLUMN is_encrypted INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }

        // Rebuild legacy FTS5 table/triggers if notes_fts predates the title column
        let fts_exists: i32 = self.conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='notes_fts'",
            [],
            |row| row.get(0),
        )?;

        if fts_exists > 0 {
            if !self.has_column("notes_fts", "title")? {
                self.conn.execute_batch(
                    "DROP TRIGGER IF EXISTS notes_ai;
                     DROP TRIGGER IF EXISTS notes_ad;
                     DROP TRIGGER IF EXISTS notes_au;
                     DROP TABLE IF EXISTS notes_fts;",
                )?;
            } else {
                // Upgrade triggers to exclude encrypted notes if still using v0.4 triggers
                let trigger_sql: Option<String> = self
                    .conn
                    .query_row(
                        "SELECT sql FROM sqlite_master WHERE type='trigger' AND name='notes_ai'",
                        [],
                        |row| row.get(0),
                    )
                    .ok();
                if let Some(sql) = trigger_sql
                    && !sql.contains("is_encrypted")
                {
                    self.conn.execute_batch(
                        "DROP TRIGGER IF EXISTS notes_ai;
                         DROP TRIGGER IF EXISTS notes_ad;
                         DROP TRIGGER IF EXISTS notes_au;",
                    )?;
                }
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

        // Triggers to keep FTS index in sync while excluding encrypted bodies
        self.conn.execute_batch(
            "CREATE TRIGGER IF NOT EXISTS notes_ai AFTER INSERT ON notes WHEN new.is_encrypted = 0 BEGIN
                INSERT INTO notes_fts(rowid, body, title) VALUES (new.id, new.body, new.title);
            END;
            CREATE TRIGGER IF NOT EXISTS notes_ad AFTER DELETE ON notes WHEN old.is_encrypted = 0 BEGIN
                INSERT INTO notes_fts(notes_fts, rowid, body, title) VALUES('delete', old.id, old.body, old.title);
            END;
            CREATE TRIGGER IF NOT EXISTS notes_au AFTER UPDATE ON notes BEGIN
                INSERT INTO notes_fts(notes_fts, rowid, body, title)
                    SELECT 'delete', old.id, old.body, old.title WHERE old.is_encrypted = 0;
                INSERT INTO notes_fts(rowid, body, title)
                    SELECT new.id, new.body, new.title WHERE new.is_encrypted = 0;
            END;
            CREATE INDEX IF NOT EXISTS idx_note_tags_tag_id ON note_tags(tag_id);
            CREATE INDEX IF NOT EXISTS idx_notes_updated ON notes(updated DESC);
            CREATE INDEX IF NOT EXISTS idx_notes_pin_arch_updated ON notes(archived ASC, pinned DESC, updated DESC);",
        )?;

        if fts_existed == 0 {
            let note_count: i64 = self.conn.query_row(
                "SELECT count(*) FROM notes WHERE is_encrypted = 0",
                [],
                |row| row.get(0),
            )?;
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
            "INSERT INTO notes (body, title, source, created, updated, pinned, archived, is_encrypted)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, 0)",
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
            pinned: row.get::<_, i64>(6)? != 0,
            archived: row.get::<_, i64>(7)? != 0,
            is_encrypted: row.get::<_, i64>(8)? != 0,
            tags: Vec::new(),
        })
    }

    fn map_summary_row(row: &rusqlite::Row, with_search: bool) -> rusqlite::Result<NoteSummary> {
        let created_str: String = row.get(5)?;
        let updated_str: String = row.get(6)?;
        let char_len_raw: i64 = row.get(2)?;
        let snippet = if with_search { row.get(10)? } else { None };
        let rank = if with_search { row.get(11)? } else { None };
        Ok(NoteSummary {
            id: row.get(0)?,
            preview: row.get(1)?,
            char_len: char_len_raw.max(0) as usize,
            title: row.get(3)?,
            source: row.get(4)?,
            created: Self::parse_timestamp(5, created_str)?,
            updated: Self::parse_timestamp(6, updated_str)?,
            pinned: row.get::<_, i64>(7)? != 0,
            archived: row.get::<_, i64>(8)? != 0,
            is_encrypted: row.get::<_, i64>(9)? != 0,
            tags: Vec::new(),
            snippet,
            rank,
        })
    }

    fn fetch_tags_for_ids(&self, ids: &[i64]) -> Result<HashMap<i64, Vec<String>>> {
        let mut tags_by_note: HashMap<i64, Vec<String>> = HashMap::with_capacity(ids.len());
        if ids.is_empty() {
            return Ok(tags_by_note);
        }

        if ids.len() == 1 {
            let mut stmt = self.conn.prepare_cached(
                "SELECT t.name
                 FROM note_tags nt
                 JOIN tags t ON nt.tag_id = t.id
                 WHERE nt.note_id = ?1
                 ORDER BY t.name ASC",
            )?;
            let tags = stmt
                .query_map(params![ids[0]], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            if !tags.is_empty() {
                tags_by_note.insert(ids[0], tags);
            }
            return Ok(tags_by_note);
        }

        for chunk in ids.chunks(500) {
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
            let params = rusqlite::params_from_iter(chunk.iter().copied());
            let rows = stmt.query_map(params, |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (note_id, tag) = row?;
                tags_by_note.entry(note_id).or_default().push(tag);
            }
        }

        Ok(tags_by_note)
    }

    fn populate_tags(&self, notes: &mut [Note]) -> Result<()> {
        if notes.is_empty() {
            return Ok(());
        }
        let ids: Vec<i64> = notes.iter().map(|n| n.id).collect();
        let mut tags_by_note = self.fetch_tags_for_ids(&ids)?;
        for note in notes {
            if let Some(tags) = tags_by_note.remove(&note.id) {
                note.tags = tags;
            }
        }
        Ok(())
    }

    fn populate_summary_tags(&self, summaries: &mut [NoteSummary]) -> Result<()> {
        if summaries.is_empty() {
            return Ok(());
        }
        let ids: Vec<i64> = summaries.iter().map(|n| n.id).collect();
        let mut tags_by_note = self.fetch_tags_for_ids(&ids)?;
        for note in summaries {
            if let Some(tags) = tags_by_note.remove(&note.id) {
                note.tags = tags;
            }
        }
        Ok(())
    }

    pub fn build_fts_query(query: &str) -> Option<String> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return None;
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
            None
        } else {
            Some(fts_terms.join(" "))
        }
    }

    fn tag_prefix_patterns(tag_name: &str) -> Option<(String, String)> {
        let normalized = tag_name.trim().to_lowercase();
        if normalized.is_empty() {
            return None;
        }
        let escaped = normalized
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let prefix_pattern = format!("{}/%", escaped);
        Some((normalized, prefix_pattern))
    }

    pub fn get_note(&self, id: i64) -> Result<Note> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, body, title, source, created, updated, pinned, archived, is_encrypted
             FROM notes WHERE id = ?1",
        )?;
        let note = stmt.query_row(params![id], Self::map_note_row)?;
        let mut notes = vec![note];
        self.populate_tags(&mut notes)?;
        Ok(notes.into_iter().next().expect("single queried note"))
    }

    #[cfg(test)]
    pub fn list_notes(&self) -> Result<Vec<Note>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, body, title, source, created, updated, pinned, archived, is_encrypted
             FROM notes
             WHERE archived = 0
             ORDER BY pinned DESC, updated DESC",
        )?;
        let mut notes = stmt
            .query_map([], Self::map_note_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        self.populate_tags(&mut notes)?;
        Ok(notes)
    }

    pub fn list_all_notes_for_export(&self, filter: &ListFilter<'_>) -> Result<Vec<Note>> {
        let summaries = self.query_note_summaries(filter)?;
        let mut notes = Vec::with_capacity(summaries.len());
        for summary in summaries {
            notes.push(self.get_note(summary.id)?);
        }
        Ok(notes)
    }

    pub fn query_note_summaries(&self, filter: &ListFilter<'_>) -> Result<Vec<NoteSummary>> {
        let include_archived = if filter.include_archived { 1i64 } else { 0i64 };
        let source_filter = filter.source.map(str::trim).filter(|s| !s.is_empty());
        let tag_patterns = filter.tag.and_then(Self::tag_prefix_patterns);

        if let Some(raw_query) = filter.query.map(str::trim).filter(|q| !q.is_empty()) {
            let Some(fts_query) = Self::build_fts_query(raw_query) else {
                return Ok(Vec::new());
            };

            let mut summaries = if let Some((ref exact_tag, ref prefix_pattern)) = tag_patterns {
                let mut stmt = self.conn.prepare_cached(
                    "SELECT n.id,
                            CASE WHEN n.is_encrypted = 1 THEN '[Encrypted note]' ELSE substr(n.body, 1, 240) END,
                            length(n.body),
                            n.title, n.source, n.created, n.updated, n.pinned, n.archived, n.is_encrypted,
                            snippet(notes_fts, -1, '«', '»', '…', 12),
                            bm25(notes_fts, 1.0, 2.0) AS score
                     FROM notes_fts
                     JOIN notes n ON notes_fts.rowid = n.id
                     WHERE notes_fts MATCH ?1
                       AND (?2 = 1 OR n.archived = 0)
                       AND (?3 IS NULL OR n.source = ?3)
                       AND EXISTS (
                           SELECT 1 FROM note_tags nt
                           JOIN tags t ON nt.tag_id = t.id
                           WHERE nt.note_id = n.id
                             AND (t.name = ?4 OR t.name LIKE ?5 ESCAPE '\\')
                       )
                     ORDER BY n.pinned DESC, score ASC, n.updated DESC, n.id DESC",
                )?;
                stmt.query_map(
                    params![
                        fts_query,
                        include_archived,
                        source_filter,
                        exact_tag,
                        prefix_pattern
                    ],
                    |row| Self::map_summary_row(row, true),
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?
            } else {
                let mut stmt = self.conn.prepare_cached(
                    "SELECT n.id,
                            CASE WHEN n.is_encrypted = 1 THEN '[Encrypted note]' ELSE substr(n.body, 1, 240) END,
                            length(n.body),
                            n.title, n.source, n.created, n.updated, n.pinned, n.archived, n.is_encrypted,
                            snippet(notes_fts, -1, '«', '»', '…', 12),
                            bm25(notes_fts, 1.0, 2.0) AS score
                     FROM notes_fts
                     JOIN notes n ON notes_fts.rowid = n.id
                     WHERE notes_fts MATCH ?1
                       AND (?2 = 1 OR n.archived = 0)
                       AND (?3 IS NULL OR n.source = ?3)
                     ORDER BY n.pinned DESC, score ASC, n.updated DESC, n.id DESC",
                )?;
                stmt.query_map(params![fts_query, include_archived, source_filter], |row| {
                    Self::map_summary_row(row, true)
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
            };

            self.populate_summary_tags(&mut summaries)?;
            return Ok(summaries);
        }

        let mut summaries = if let Some((ref exact_tag, ref prefix_pattern)) = tag_patterns {
            let mut stmt = self.conn.prepare_cached(
                "SELECT DISTINCT n.id,
                        CASE WHEN n.is_encrypted = 1 THEN '[Encrypted note]' ELSE substr(n.body, 1, 240) END,
                        length(n.body),
                        n.title, n.source, n.created, n.updated, n.pinned, n.archived, n.is_encrypted
                 FROM notes n
                 JOIN note_tags nt ON n.id = nt.note_id
                 JOIN tags t ON nt.tag_id = t.id
                 WHERE (t.name = ?1 OR t.name LIKE ?2 ESCAPE '\\')
                   AND (?3 = 1 OR n.archived = 0)
                   AND (?4 IS NULL OR n.source = ?4)
                 ORDER BY n.pinned DESC, n.updated DESC, n.id DESC",
            )?;
            stmt.query_map(
                params![exact_tag, prefix_pattern, include_archived, source_filter],
                |row| Self::map_summary_row(row, false),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            let mut stmt = self.conn.prepare_cached(
                "SELECT id,
                        CASE WHEN is_encrypted = 1 THEN '[Encrypted note]' ELSE substr(body, 1, 240) END,
                        length(body),
                        title, source, created, updated, pinned, archived, is_encrypted
                 FROM notes
                 WHERE (?1 = 1 OR archived = 0)
                   AND (?2 IS NULL OR source = ?2)
                 ORDER BY pinned DESC, updated DESC, id DESC",
            )?;
            stmt.query_map(params![include_archived, source_filter], |row| {
                Self::map_summary_row(row, false)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };

        self.populate_summary_tags(&mut summaries)?;
        Ok(summaries)
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

    #[cfg(test)]
    pub fn search_notes(&self, query: &str) -> Result<Vec<Note>> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return self.list_notes();
        }

        let Some(fts_query) = Self::build_fts_query(trimmed) else {
            return Ok(Vec::new());
        };

        let mut stmt = self.conn.prepare_cached(
            "SELECT n.id, n.body, n.title, n.source, n.created, n.updated, n.pinned, n.archived, n.is_encrypted
             FROM notes_fts
             JOIN notes n ON notes_fts.rowid = n.id
             WHERE notes_fts MATCH ?1 AND n.archived = 0
             ORDER BY n.pinned DESC, bm25(notes_fts, 1.0, 2.0) ASC, n.updated DESC",
        )?;
        let mut notes = stmt
            .query_map(params![fts_query], Self::map_note_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        self.populate_tags(&mut notes)?;
        Ok(notes)
    }

    #[cfg(test)]
    pub fn find_by_tag(&self, tag_name: &str) -> Result<Vec<Note>> {
        let Some((normalized, prefix_pattern)) = Self::tag_prefix_patterns(tag_name) else {
            return self.list_notes();
        };

        let mut stmt = self.conn.prepare_cached(
            "SELECT DISTINCT n.id, n.body, n.title, n.source, n.created, n.updated, n.pinned, n.archived, n.is_encrypted
             FROM notes n
             JOIN note_tags nt ON n.id = nt.note_id
             JOIN tags t ON nt.tag_id = t.id
             WHERE (t.name = ?1 OR t.name LIKE ?2 ESCAPE '\\') AND n.archived = 0
             ORDER BY n.pinned DESC, n.updated DESC",
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
            .prepare_cached("SELECT name FROM tags ORDER BY name ASC")?;
        let tag_iter = stmt.query_map([], |row| row.get(0))?;
        let mut tags = Vec::new();
        for tag in tag_iter {
            tags.push(tag?);
        }
        Ok(tags)
    }

    pub fn list_tags_with_counts(&self, include_archived: bool) -> Result<Vec<(String, i64)>> {
        let inc = if include_archived { 1i64 } else { 0i64 };
        let mut stmt = self.conn.prepare_cached(
            "SELECT t.name, count(nt.note_id) AS cnt
             FROM tags t
             JOIN note_tags nt ON t.id = nt.tag_id
             JOIN notes n ON nt.note_id = n.id
             WHERE (?1 = 1 OR n.archived = 0)
             GROUP BY t.name
             HAVING cnt > 0
             ORDER BY t.name ASC",
        )?;
        let rows = stmt
            .query_map(params![inc], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
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

    pub fn update_note_metadata(
        &self,
        id: i64,
        title: Option<&str>,
        tags: &[String],
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let now = Utc::now().to_rfc3339();
        let rows = tx.execute(
            "UPDATE notes SET title = ?1, updated = ?2 WHERE id = ?3",
            params![title, now, id],
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

    pub fn set_pinned(&self, id: i64, pinned: bool) -> Result<()> {
        let rows = self.conn.execute(
            "UPDATE notes SET pinned = ?1 WHERE id = ?2",
            params![if pinned { 1i64 } else { 0i64 }, id],
        )?;
        if rows == 0 {
            return Err(anyhow!("Note #{} not found.", id));
        }
        Ok(())
    }

    pub fn set_archived(&self, id: i64, archived: bool) -> Result<()> {
        let rows = self.conn.execute(
            "UPDATE notes SET archived = ?1 WHERE id = ?2",
            params![if archived { 1i64 } else { 0i64 }, id],
        )?;
        if rows == 0 {
            return Err(anyhow!("Note #{} not found.", id));
        }
        Ok(())
    }

    pub fn get_statistics(&self) -> Result<Statistics> {
        let mut agg_stmt = self.conn.prepare_cached(
            "SELECT count(*),
                    coalesce(sum(CASE WHEN pinned != 0 THEN 1 ELSE 0 END), 0),
                    coalesce(sum(CASE WHEN archived != 0 THEN 1 ELSE 0 END), 0),
                    coalesce(sum(length(body)), 0),
                    coalesce(avg(length(body)), 0.0)
             FROM notes",
        )?;
        let (total_notes, pinned_count, archived_count, total_chars, avg_note_length): (
            i64,
            i64,
            i64,
            i64,
            f64,
        ) = agg_stmt.query_row([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })?;

        let tag_count: i64 = self
            .conn
            .prepare_cached("SELECT count(*) FROM tags")?
            .query_row([], |row| row.get(0))?;

        let mut stmt = self.conn.prepare_cached(
            "SELECT t.name, count(nt.note_id) as count
             FROM tags t
             JOIN note_tags nt ON t.id = nt.tag_id
             GROUP BY t.name
             ORDER BY count DESC, t.name ASC
             LIMIT 5",
        )?;
        let top_tags = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;

        let mut stmt = self.conn.prepare_cached(
            "SELECT source, count(*) as count
             FROM notes
             GROUP BY source
             ORDER BY count DESC, source ASC
             LIMIT 5",
        )?;
        let top_sources = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Statistics {
            total_notes,
            pinned_count,
            archived_count,
            total_chars,
            avg_note_length,
            tag_count,
            top_tags,
            top_sources,
        })
    }

    pub fn check_health(&self) -> Result<DbHealth> {
        let integrity_message: String =
            self.conn
                .query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        let integrity_ok = integrity_message.eq_ignore_ascii_case("ok");

        let notes_count: i64 = self
            .conn
            .query_row("SELECT count(*) FROM notes", [], |r| r.get(0))?;
        let plaintext_notes_count: i64 = self.conn.query_row(
            "SELECT count(*) FROM notes WHERE is_encrypted = 0",
            [],
            |r| r.get(0),
        )?;
        let fts_rows_count: i64 =
            self.conn
                .query_row("SELECT count(*) FROM notes_fts", [], |r| r.get(0))?;

        let journal_mode: String = self
            .conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
        let foreign_keys: i64 = self
            .conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;

        Ok(DbHealth {
            integrity_ok,
            integrity_message,
            notes_count,
            plaintext_notes_count,
            fts_rows_count,
            fts_in_sync: plaintext_notes_count == fts_rows_count,
            journal_mode,
            foreign_keys: foreign_keys != 0,
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
        assert!(!note.pinned);
        assert!(!note.archived);
        assert!(!note.is_encrypted);
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

        let counts = db.list_tags_with_counts(false)?;
        assert_eq!(
            counts,
            vec![
                ("a".to_string(), 2),
                ("b".to_string(), 1),
                ("c".to_string(), 1)
            ]
        );
        Ok(())
    }

    #[test]
    fn test_get_statistics() -> Result<()> {
        let db = Database::in_memory()?;
        let id1 = db.create_note("Note 1", None, "manual", &["work".to_string()])?;
        let id2 = db.create_note(
            "Note 2",
            None,
            "tui",
            &["work".to_string(), "urgent".to_string()],
        )?;
        db.set_pinned(id1, true)?;
        db.set_archived(id2, true)?;

        let stats = db.get_statistics()?;
        assert_eq!(stats.total_notes, 2);
        assert_eq!(stats.pinned_count, 1);
        assert_eq!(stats.archived_count, 1);
        assert_eq!(stats.tag_count, 2);
        assert!(stats.total_chars > 0);
        assert!(stats.avg_note_length > 0.0);

        let work_tag = stats.top_tags.iter().find(|(t, _)| t == "work").unwrap();
        assert_eq!(work_tag.1, 2);

        Ok(())
    }

    #[test]
    fn test_pin_archive_and_bm25_snippets() -> Result<()> {
        let db = Database::in_memory()?;
        let id_body_match = db.create_note(
            "Some introductory text mentioning authentication once in passing.",
            Some("General Note"),
            "cli",
            &["work/auth".to_string()],
        )?;
        let id_title_match = db.create_note(
            "PKCE verifier details for auth callback flow and auth token exchange.",
            Some("Auth Fix"),
            "tui",
            &["work/jotun".to_string()],
        )?;

        let results = db.query_note_summaries(&ListFilter {
            query: Some("auth"),
            ..Default::default()
        })?;
        assert_eq!(results.len(), 2);
        // Title weighted 2.0 + multiple body hits should rank Auth Fix first
        assert_eq!(results[0].id, id_title_match);
        assert!(results[0].snippet.as_deref().unwrap_or("").contains('«'));

        // Pinning promotes body_match to top of list
        db.set_pinned(id_body_match, true)?;
        let listed = db.query_note_summaries(&ListFilter::default())?;
        assert_eq!(listed[0].id, id_body_match);
        assert!(listed[0].pinned);

        // Archiving hides by default unless include_archived = true
        db.set_archived(id_body_match, true)?;
        let active = db.query_note_summaries(&ListFilter::default())?;
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, id_title_match);

        let all = db.query_note_summaries(&ListFilter {
            include_archived: true,
            ..Default::default()
        })?;
        assert_eq!(all.len(), 2);
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
    fn test_legacy_v02_and_v04_migration_and_health() -> Result<()> {
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

        let health = db.check_health()?;
        assert!(health.integrity_ok);
        assert!(health.fts_in_sync);
        assert_eq!(health.notes_count, 2);
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

    #[test]
    fn test_10000_note_summary_and_fts_latency() -> Result<()> {
        let db = Database::in_memory()?;
        {
            let tx = db.conn.unchecked_transaction()?;
            let mut insert_note = tx.prepare(
                "INSERT INTO notes (body, title, source, created, updated, pinned, archived, is_encrypted)
                 VALUES (?1, ?2, 'bench', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 0, 0, 0)",
            )?;
            for i in 0..10_000 {
                let body = if i == 9_999 {
                    "Needle authentication token benchmark entry"
                } else {
                    "Routine engineering log entry with standard notes content"
                };
                let title = format!("Note {}", i);
                insert_note.execute(params![body, title])?;
            }
            drop(insert_note);
            tx.commit()?;
        }

        let start_list = std::time::Instant::now();
        let summaries = db.query_note_summaries(&ListFilter::default())?;
        let list_elapsed = start_list.elapsed();
        assert_eq!(summaries.len(), 10_000);
        assert!(
            list_elapsed.as_millis() < 2000,
            "10k summary list took {:?}",
            list_elapsed
        );

        let start_search = std::time::Instant::now();
        let found = db.query_note_summaries(&ListFilter {
            query: Some("Needle"),
            ..Default::default()
        })?;
        let search_elapsed = start_search.elapsed();
        assert_eq!(found.len(), 1);
        assert!(
            search_elapsed.as_millis() < 500,
            "10k FTS search took {:?}",
            search_elapsed
        );
        Ok(())
    }
}
