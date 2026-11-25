mod tag_tree;
mod theme;

use crate::commands;
use crate::config::Config;
use crate::db::{Database, ListFilter, Note, NoteSummary, Statistics};
use crate::frontmatter::{parse_frontmatter, parse_tag_csv, serialize_with_frontmatter};
use anyhow::Result;
use chrono::{DateTime, Utc};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, Gauge, List, ListItem, ListState, Paragraph, Row,
        Scrollbar, ScrollbarOrientation, ScrollbarState, Table, Wrap,
    },
};
use std::collections::HashSet;
use std::io;
use std::path::PathBuf;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use tag_tree::{TagRow, build_tag_rows, format_tag_row};
use theme::ResolvedTheme;

const LARGE_NOTE_HIGHLIGHT_BYTES: usize = 50 * 1024;
const LARGE_NOTE_HIGHLIGHT_LINES: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Tags,
    Notes,
    Content,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModalState {
    None,
    ConfirmDelete {
        id: i64,
        title: String,
    },
    Help,
    RenameNote {
        id: i64,
        input: String,
    },
    EditTags {
        id: i64,
        input: String,
        suggestions: Vec<String>,
    },
}

struct SyntaxAssets {
    syntax_set: SyntaxSet,
    theme: Theme,
}

struct CachedPreview {
    note_id: i64,
    updated: DateTime<Utc>,
    syntax_highlighting: bool,
    no_color: bool,
    truncated_highlight: bool,
    lines: Vec<Line<'static>>,
}

pub struct App<'a> {
    db: &'a Database,
    config: &'a Config,
    resolved_theme: ResolvedTheme,
    notes: Vec<NoteSummary>,
    selected_note: Option<Note>,
    tag_rows: Vec<TagRow>,
    all_known_tags: Vec<String>,
    collapsed_tags: HashSet<String>,
    selected_tag_filter: Option<String>,
    list_state: ListState,
    tag_state: ListState,
    search_query: String,
    status_msg: String,
    status_is_error: bool,
    editing_search: bool,
    active_pane: Pane,
    syntax_assets: Option<SyntaxAssets>,
    show_insights: bool,
    show_archived: bool,
    wrap_lines: bool,
    zoomed: bool,
    modal: ModalState,
    db_path: PathBuf,
    preview_scroll: u16,
    cached_preview: Option<CachedPreview>,
    cached_stats: Option<Statistics>,
    no_color: bool,
}

impl<'a> App<'a> {
    pub fn new(db: &'a Database, config: &'a Config, db_path: PathBuf) -> Result<Self> {
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        let resolved_theme = ResolvedTheme::from_config(&config.theme, no_color);
        let wrap_lines = config.wrap_lines.unwrap_or(true);

        let mut app = Self {
            db,
            config,
            resolved_theme,
            notes: Vec::new(),
            selected_note: None,
            tag_rows: Vec::new(),
            all_known_tags: Vec::new(),
            collapsed_tags: HashSet::new(),
            selected_tag_filter: None,
            list_state: ListState::default(),
            tag_state: ListState::default(),
            search_query: String::new(),
            status_msg: String::from("Ready"),
            status_is_error: false,
            editing_search: false,
            active_pane: Pane::Notes,
            syntax_assets: None, // Lazy-loaded only on first highlighted preview
            show_insights: false,
            show_archived: false,
            wrap_lines,
            zoomed: false,
            modal: ModalState::None,
            db_path,
            preview_scroll: 0,
            cached_preview: None,
            cached_stats: None,
            no_color,
        };

        app.refresh_tags()?;
        app.update_notes()?;
        Ok(app)
    }

    fn set_status(&mut self, msg: impl Into<String>, is_error: bool) {
        self.status_msg = msg.into();
        self.status_is_error = is_error;
    }

    fn active_tag_filter(&self) -> Option<&str> {
        self.selected_tag_filter.as_deref()
    }

    fn refresh_tags(&mut self) -> Result<()> {
        let tags_with_counts = self.db.list_tags_with_counts(self.show_archived)?;
        self.all_known_tags = self.db.list_all_tags()?;

        let stats = self.db.get_statistics()?;
        let active_count = if self.show_archived {
            stats.total_notes
        } else {
            stats.total_notes - stats.archived_count
        };
        self.cached_stats = Some(stats);

        self.tag_rows = build_tag_rows(&tags_with_counts, active_count, &self.collapsed_tags);

        let new_idx = self
            .selected_tag_filter
            .as_ref()
            .and_then(|path| {
                self.tag_rows
                    .iter()
                    .position(|r| r.full_path.as_ref() == Some(path))
            })
            .unwrap_or(0);

        if new_idx == 0 {
            self.selected_tag_filter = None;
        }
        self.tag_state.select(Some(new_idx));
        Ok(())
    }

    fn update_notes(&mut self) -> Result<()> {
        let query = if self.search_query.trim().is_empty() {
            None
        } else {
            Some(self.search_query.as_str())
        };
        let tag = self.selected_tag_filter.as_deref();

        let res = self.db.query_note_summaries(&ListFilter {
            tag,
            source: None,
            include_archived: self.show_archived,
            query,
        });

        match res {
            Ok(notes) => {
                self.notes = notes;
                if query.is_some() {
                    self.set_status(format!("Found {} results", self.notes.len()), false);
                }
            }
            Err(e) => {
                self.notes = Vec::new();
                self.set_status(format!("Error fetching notes: {}", e), true);
            }
        }

        if self.notes.is_empty() {
            self.list_state.select(None);
            self.selected_note = None;
        } else {
            let selected = self.list_state.selected().unwrap_or(0);
            self.list_state
                .select(Some(selected.min(self.notes.len().saturating_sub(1))));
            self.load_selected_note();
        }
        self.preview_scroll = 0;
        Ok(())
    }

    fn load_selected_note(&mut self) {
        let Some(id) = self
            .list_state
            .selected()
            .and_then(|i| self.notes.get(i))
            .map(|n| n.id)
        else {
            self.selected_note = None;
            return;
        };

        if self.selected_note.as_ref().is_some_and(|n| n.id == id) {
            // Check updated timestamp match against summary
            if let Some(summary) = self.notes.iter().find(|s| s.id == id)
                && self.selected_note.as_ref().is_some_and(|n| {
                    n.updated == summary.updated
                        && n.pinned == summary.pinned
                        && n.archived == summary.archived
                })
            {
                return;
            }
        }

        match self.db.get_note(id) {
            Ok(note) => self.selected_note = Some(note),
            Err(e) => {
                self.selected_note = None;
                self.set_status(format!("Failed to load note #{}: {}", id, e), true);
            }
        }
    }

    fn invalidate_caches(&mut self) {
        self.cached_stats = None;
        self.cached_preview = None;
        self.selected_note = None;
    }

    pub fn next(&mut self) {
        match self.active_pane {
            Pane::Notes => {
                if self.notes.is_empty() {
                    return;
                }
                let i = match self.list_state.selected() {
                    Some(i) if i + 1 < self.notes.len() => i + 1,
                    _ => 0,
                };
                self.list_state.select(Some(i));
                self.preview_scroll = 0;
                self.load_selected_note();
            }
            Pane::Tags => {
                if self.tag_rows.is_empty() {
                    return;
                }
                let i = match self.tag_state.selected() {
                    Some(i) if i + 1 < self.tag_rows.len() => i + 1,
                    _ => 0,
                };
                self.tag_state.select(Some(i));
                self.selected_tag_filter = self.tag_rows[i].full_path.clone();
                let _ = self.update_notes();
            }
            Pane::Content => {
                self.preview_scroll = self.preview_scroll.saturating_add(1);
            }
        }
    }

    pub fn previous(&mut self) {
        match self.active_pane {
            Pane::Notes => {
                if self.notes.is_empty() {
                    return;
                }
                let i = match self.list_state.selected() {
                    Some(0) | None => self.notes.len() - 1,
                    Some(i) => i - 1,
                };
                self.list_state.select(Some(i));
                self.preview_scroll = 0;
                self.load_selected_note();
            }
            Pane::Tags => {
                if self.tag_rows.is_empty() {
                    return;
                }
                let i = match self.tag_state.selected() {
                    Some(0) | None => self.tag_rows.len() - 1,
                    Some(i) => i - 1,
                };
                self.tag_state.select(Some(i));
                self.selected_tag_filter = self.tag_rows[i].full_path.clone();
                let _ = self.update_notes();
            }
            Pane::Content => {
                self.preview_scroll = self.preview_scroll.saturating_sub(1);
            }
        }
    }

    pub fn page_down(&mut self) {
        if self.active_pane == Pane::Content {
            self.preview_scroll = self.preview_scroll.saturating_add(10);
        }
    }

    pub fn page_up(&mut self) {
        if self.active_pane == Pane::Content {
            self.preview_scroll = self.preview_scroll.saturating_sub(10);
        }
    }

    pub fn switch_pane(&mut self) {
        self.active_pane = match self.active_pane {
            Pane::Tags => Pane::Notes,
            Pane::Notes => Pane::Content,
            Pane::Content => Pane::Tags,
        };
    }

    pub fn switch_pane_reverse(&mut self) {
        self.active_pane = match self.active_pane {
            Pane::Tags => Pane::Content,
            Pane::Notes => Pane::Tags,
            Pane::Content => Pane::Notes,
        };
    }

    fn toggle_tag_collapse(&mut self, force_collapse: Option<bool>) {
        if self.active_pane != Pane::Tags {
            return;
        }
        let Some(idx) = self.tag_state.selected() else {
            return;
        };
        let Some(row) = self.tag_rows.get(idx).cloned() else {
            return;
        };
        let (Some(path), true) = (row.full_path, row.has_children) else {
            return;
        };

        let should_collapse = force_collapse.unwrap_or(!self.collapsed_tags.contains(&path));
        if should_collapse {
            self.collapsed_tags.insert(path);
        } else {
            self.collapsed_tags.remove(&path);
        }
        let _ = self.refresh_tags();
    }

    fn clear_filters(&mut self) {
        let had_filter = !self.search_query.is_empty() || self.selected_tag_filter.is_some();
        self.search_query.clear();
        self.selected_tag_filter = None;
        self.tag_state.select(Some(0));
        if had_filter {
            let _ = self.update_notes();
            self.set_status("Cleared filters", false);
        }
    }

    fn delete_note_by_id(&mut self, id: i64) -> Result<()> {
        self.db.delete_note(id)?;
        self.set_status(format!("Deleted note #{}", id), false);
        self.invalidate_caches();
        self.refresh_tags()?;
        self.update_notes()?;
        Ok(())
    }

    fn open_delete_modal(&mut self) {
        if let Some(i) = self.list_state.selected()
            && let Some(note) = self.notes.get(i)
        {
            let title = summary_display_title(note);
            self.modal = ModalState::ConfirmDelete { id: note.id, title };
        }
    }

    fn open_rename_modal(&mut self) {
        if let Some(i) = self.list_state.selected()
            && let Some(note) = self.notes.get(i)
        {
            self.modal = ModalState::RenameNote {
                id: note.id,
                input: note.title.clone().unwrap_or_default(),
            };
        }
    }

    fn open_tags_modal(&mut self) {
        if let Some(i) = self.list_state.selected()
            && let Some(note) = self.notes.get(i)
        {
            let input = note.tags.join(", ");
            let suggestions = self.compute_tag_suggestions(&input);
            self.modal = ModalState::EditTags {
                id: note.id,
                input,
                suggestions,
            };
        }
    }

    fn compute_tag_suggestions(&self, input: &str) -> Vec<String> {
        let fragment = input.rsplit(',').next().unwrap_or("").trim().to_lowercase();
        let existing: HashSet<String> = parse_tag_csv(input).into_iter().collect();
        self.all_known_tags
            .iter()
            .filter(|t| !existing.contains(*t))
            .filter(|t| fragment.is_empty() || t.contains(&fragment))
            .take(5)
            .cloned()
            .collect()
    }

    fn toggle_pin_current(&mut self) -> Result<()> {
        if let Some(i) = self.list_state.selected()
            && let Some(note) = self.notes.get(i)
        {
            let id = note.id;
            let next_pinned = !note.pinned;
            self.db.set_pinned(id, next_pinned)?;
            self.invalidate_caches();
            self.update_notes()?;
            if let Some(pos) = self.notes.iter().position(|n| n.id == id) {
                self.list_state.select(Some(pos));
                self.load_selected_note();
            }
            self.set_status(
                if next_pinned {
                    format!("Pinned note #{}", id)
                } else {
                    format!("Unpinned note #{}", id)
                },
                false,
            );
        }
        Ok(())
    }

    fn toggle_archive_current(&mut self) -> Result<()> {
        if let Some(i) = self.list_state.selected()
            && let Some(note) = self.notes.get(i)
        {
            let id = note.id;
            let next_archived = !note.archived;
            self.db.set_archived(id, next_archived)?;
            self.invalidate_caches();
            self.refresh_tags()?;
            self.update_notes()?;
            self.set_status(
                if next_archived {
                    format!("Archived note #{}", id)
                } else {
                    format!("Unarchived note #{}", id)
                },
                false,
            );
        }
        Ok(())
    }

    pub fn copy_current(&mut self) -> Result<()> {
        self.load_selected_note();
        if let Some(ref note) = self.selected_note {
            match commands::copy_to_clipboard(&note.body)? {
                commands::ClipboardMethod::Native(_) => {
                    self.set_status(format!("Copied note #{} to clipboard", note.id), false);
                }
                commands::ClipboardMethod::Osc52 => {
                    self.set_status(
                        format!("Copied note #{} via OSC 52 clipboard", note.id),
                        false,
                    );
                }
            }
        }
        Ok(())
    }

    fn run_external_editor(&self, path: &std::path::Path) -> Result<std::process::ExitStatus> {
        struct RawSuspendGuard;
        impl Drop for RawSuspendGuard {
            fn drop(&mut self) {
                let _ = enable_raw_mode();
                let _ = execute!(io::stdout(), EnterAlternateScreen);
            }
        }

        disable_raw_mode()?;
        execute!(io::stdout(), LeaveAlternateScreen)?;
        let _resume = RawSuspendGuard;

        commands::spawn_editor(self.config, path, false)
    }

    pub fn edit_current(&mut self) -> Result<()> {
        self.load_selected_note();
        let Some(note) = self.selected_note.clone() else {
            return Ok(());
        };
        if note.is_encrypted {
            self.set_status("Note is encrypted; unlock before editing", true);
            return Ok(());
        }

        let use_frontmatter = self.config.frontmatter.unwrap_or(true);
        let initial = if use_frontmatter {
            serialize_with_frontmatter(note.title.as_deref(), &note.tags, &note.body)
        } else {
            note.body.clone()
        };

        let temp_file = commands::create_markdown_tempfile(&initial)?;
        let status = self.run_external_editor(temp_file.path())?;
        if status.success() {
            let edited = commands::read_edited_body(temp_file.path())?;
            let (new_title, new_tags, new_body) = if use_frontmatter {
                parse_frontmatter(&edited, note.title.as_deref(), &note.tags)
            } else {
                (note.title.clone(), note.tags.clone(), edited)
            };

            if new_body != note.body || new_title != note.title || new_tags != note.tags {
                self.db
                    .update_note(note.id, &new_body, new_title.as_deref(), &new_tags)?;
                self.set_status(format!("Updated note #{}", note.id), false);
                self.invalidate_caches();
                self.refresh_tags()?;
                self.update_notes()?;
            } else {
                self.set_status("No changes made", false);
            }
        } else {
            self.set_status("Editor exited with non-zero status", true);
        }
        Ok(())
    }

    pub fn create_new(&mut self) -> Result<()> {
        let use_frontmatter = self.config.frontmatter.unwrap_or(true);
        let initial_tags: Vec<String> = self
            .active_tag_filter()
            .map(|t| vec![t.to_string()])
            .unwrap_or_default();
        let initial = if use_frontmatter {
            serialize_with_frontmatter(None, &initial_tags, "")
        } else {
            String::new()
        };

        let temp_file = commands::create_markdown_tempfile(&initial)?;
        let status = self.run_external_editor(temp_file.path())?;

        if status.success() {
            let raw = commands::read_edited_body(temp_file.path())?;
            let (title, tags, body) = if use_frontmatter {
                parse_frontmatter(&raw, None, &initial_tags)
            } else {
                (None, initial_tags, raw)
            };
            if !body.trim().is_empty() {
                let id = self.db.create_note(&body, title.as_deref(), "tui", &tags)?;
                self.set_status(format!("Created note #{}", id), false);
                self.invalidate_caches();
                self.refresh_tags()?;
                self.update_notes()?;
            } else {
                self.set_status("Discarded empty note", false);
            }
        } else {
            self.set_status("Editor exited with non-zero status", true);
        }
        Ok(())
    }

    fn ensure_syntax_assets(&mut self) {
        if self.syntax_assets.is_none()
            && !self.no_color
            && self.config.syntax_highlighting.unwrap_or(true)
        {
            let syntax_set = SyntaxSet::load_defaults_newlines();
            let ts = ThemeSet::load_defaults();
            let theme = ts
                .themes
                .get("base16-ocean.dark")
                .or_else(|| ts.themes.values().next())
                .cloned()
                .unwrap_or_default();
            self.syntax_assets = Some(SyntaxAssets { syntax_set, theme });
        }
    }

    fn ensure_preview_cache(&mut self) {
        let Some(ref note) = self.selected_note else {
            return;
        };
        let syntax_highlighting = self.config.syntax_highlighting.unwrap_or(true);
        let no_color = self.no_color;

        if let Some(ref cached) = self.cached_preview
            && cached.note_id == note.id
            && cached.updated == note.updated
            && cached.syntax_highlighting == syntax_highlighting
            && cached.no_color == no_color
        {
            return;
        }

        let note_id = note.id;
        let updated = note.updated;
        let is_encrypted = note.is_encrypted;
        let body = note.body.clone();

        let mut lines = Vec::new();
        let mut truncated_highlight = false;

        if is_encrypted {
            lines.push(Line::from(Span::styled(
                "[Encrypted note body — locked]",
                self.resolved_theme.muted_style(),
            )));
        } else if syntax_highlighting && !no_color && !body.is_empty() {
            self.ensure_syntax_assets();
            if let Some(ref assets) = self.syntax_assets {
                let syntax = assets
                    .syntax_set
                    .find_syntax_by_extension("md")
                    .unwrap_or_else(|| assets.syntax_set.find_syntax_plain_text());
                let mut h = HighlightLines::new(syntax, &assets.theme);
                let mut bytes_highlighted = 0usize;

                for (idx, line) in LinesWithEndings::from(&body).enumerate() {
                    if idx < LARGE_NOTE_HIGHLIGHT_LINES
                        && bytes_highlighted < LARGE_NOTE_HIGHLIGHT_BYTES
                    {
                        bytes_highlighted += line.len();
                        let ranges: Vec<(syntect::highlighting::Style, &str)> = h
                            .highlight_line(line, &assets.syntax_set)
                            .unwrap_or_default();
                        let spans: Vec<Span<'static>> = ranges
                            .into_iter()
                            .map(|(style, text)| {
                                let fg = Color::Rgb(
                                    style.foreground.r,
                                    style.foreground.g,
                                    style.foreground.b,
                                );
                                Span::styled(text.to_string(), Style::default().fg(fg))
                            })
                            .collect();
                        lines.push(Line::from(spans));
                    } else {
                        truncated_highlight = true;
                        lines.push(Line::from(line.trim_end_matches(['\r', '\n']).to_string()));
                    }
                }
            }
        } else {
            for line in body.lines() {
                lines.push(Line::from(line.to_string()));
            }
        }

        self.cached_preview = Some(CachedPreview {
            note_id,
            updated,
            syntax_highlighting,
            no_color,
            truncated_highlight,
            lines,
        });
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Result<(bool, bool)> {
        // Returns (should_quit, needs_terminal_clear)
        if self.modal != ModalState::None {
            return self.handle_modal_key(key);
        }

        if self.editing_search {
            match key.code {
                KeyCode::Enter => self.editing_search = false,
                KeyCode::Esc => {
                    self.editing_search = false;
                    if !self.search_query.is_empty() {
                        self.search_query.clear();
                        if let Err(e) = self.update_notes() {
                            self.set_status(format!("{}", e), true);
                        }
                    }
                }
                KeyCode::Char(c) => {
                    self.search_query.push(c);
                    if let Err(e) = self.update_notes() {
                        self.set_status(format!("{}", e), true);
                    }
                }
                KeyCode::Backspace => {
                    self.search_query.pop();
                    if let Err(e) = self.update_notes() {
                        self.set_status(format!("{}", e), true);
                    }
                }
                _ => {}
            }
            return Ok((false, false));
        }

        match key.code {
            KeyCode::Char('q') => return Ok((true, false)),
            KeyCode::Esc => {
                if self.show_insights {
                    self.show_insights = false;
                } else if self.zoomed {
                    self.zoomed = false;
                } else if !self.search_query.is_empty() || self.selected_tag_filter.is_some() {
                    self.clear_filters();
                } else {
                    return Ok((true, false));
                }
            }
            KeyCode::Char('?') => self.modal = ModalState::Help,
            KeyCode::Char('s') | KeyCode::Char('/') => self.editing_search = true,
            KeyCode::Char('c') => self.clear_filters(),
            KeyCode::Tab => self.switch_pane(),
            KeyCode::BackTab => self.switch_pane_reverse(),
            KeyCode::Down | KeyCode::Char('j') => self.next(),
            KeyCode::Up | KeyCode::Char('k') => self.previous(),
            KeyCode::PageDown => self.page_down(),
            KeyCode::PageUp => self.page_up(),
            KeyCode::Char(' ') | KeyCode::Enter => self.toggle_tag_collapse(None),
            KeyCode::Char('h') | KeyCode::Left => self.toggle_tag_collapse(Some(true)),
            KeyCode::Char('l') | KeyCode::Right => self.toggle_tag_collapse(Some(false)),
            KeyCode::Char('w') => {
                self.wrap_lines = !self.wrap_lines;
                self.set_status(
                    if self.wrap_lines {
                        "Line wrap: ON"
                    } else {
                        "Line wrap: OFF"
                    },
                    false,
                );
            }
            KeyCode::Char('z') => {
                self.zoomed = !self.zoomed;
            }
            KeyCode::Char('p') => {
                if let Err(e) = self.toggle_pin_current() {
                    self.set_status(format!("{}", e), true);
                }
            }
            KeyCode::Char('a') => {
                if let Err(e) = self.toggle_archive_current() {
                    self.set_status(format!("{}", e), true);
                }
            }
            KeyCode::Char('A') => {
                self.show_archived = !self.show_archived;
                let _ = self.refresh_tags();
                let _ = self.update_notes();
                self.set_status(
                    if self.show_archived {
                        "Showing archived notes"
                    } else {
                        "Hiding archived notes"
                    },
                    false,
                );
            }
            KeyCode::Char('r') => self.open_rename_modal(),
            KeyCode::Char('t') => self.open_tags_modal(),
            KeyCode::Char('y') => {
                if let Err(e) = self.copy_current() {
                    self.set_status(format!("{}", e), true);
                }
            }
            KeyCode::Char('d') => self.open_delete_modal(),
            KeyCode::Char('e') => {
                if let Err(e) = self.edit_current() {
                    self.set_status(format!("{}", e), true);
                }
                return Ok((false, true));
            }
            KeyCode::Char('n') => {
                if let Err(e) = self.create_new() {
                    self.set_status(format!("{}", e), true);
                }
                return Ok((false, true));
            }
            KeyCode::Char('i') => {
                self.show_insights = !self.show_insights;
                if self.show_insights && self.cached_stats.is_none() {
                    match self.db.get_statistics() {
                        Ok(stats) => self.cached_stats = Some(stats),
                        Err(e) => self.set_status(format!("{}", e), true),
                    }
                }
            }
            _ => {
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    return Ok((true, false));
                }
            }
        }
        Ok((false, false))
    }

    fn handle_modal_key(&mut self, key: KeyEvent) -> Result<(bool, bool)> {
        let current = std::mem::replace(&mut self.modal, ModalState::None);
        match current {
            ModalState::None => {}
            ModalState::Help => match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?') | KeyCode::Char('q') => {}
                _ => self.modal = ModalState::Help,
            },
            ModalState::ConfirmDelete { id, title } => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    if let Err(e) = self.delete_note_by_id(id) {
                        self.set_status(format!("{}", e), true);
                    }
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.set_status("Delete cancelled", false);
                }
                _ => self.modal = ModalState::ConfirmDelete { id, title },
            },
            ModalState::RenameNote { id, mut input } => match key.code {
                KeyCode::Esc => self.set_status("Rename cancelled", false),
                KeyCode::Enter => {
                    let trimmed = input.trim();
                    let next_title = (!trimmed.is_empty()).then_some(trimmed);
                    let tags = self
                        .notes
                        .iter()
                        .find(|n| n.id == id)
                        .map(|n| n.tags.clone())
                        .unwrap_or_default();
                    if let Err(e) = self.db.update_note_metadata(id, next_title, &tags) {
                        self.set_status(format!("{}", e), true);
                    } else {
                        self.invalidate_caches();
                        let _ = self.update_notes();
                        self.set_status(format!("Renamed note #{}", id), false);
                    }
                }
                KeyCode::Backspace => {
                    input.pop();
                    self.modal = ModalState::RenameNote { id, input };
                }
                KeyCode::Char(c) => {
                    input.push(c);
                    self.modal = ModalState::RenameNote { id, input };
                }
                _ => self.modal = ModalState::RenameNote { id, input },
            },
            ModalState::EditTags {
                id,
                mut input,
                mut suggestions,
            } => match key.code {
                KeyCode::Esc => self.set_status("Tag edit cancelled", false),
                KeyCode::Enter => {
                    let parsed_tags = parse_tag_csv(&input);
                    let title = self
                        .notes
                        .iter()
                        .find(|n| n.id == id)
                        .and_then(|n| n.title.clone());
                    if let Err(e) = self
                        .db
                        .update_note_metadata(id, title.as_deref(), &parsed_tags)
                    {
                        self.set_status(format!("{}", e), true);
                    } else {
                        self.invalidate_caches();
                        let _ = self.refresh_tags();
                        let _ = self.update_notes();
                        self.set_status(
                            format!("Updated tags on #{} ({} tags)", id, parsed_tags.len()),
                            false,
                        );
                    }
                }
                KeyCode::Tab => {
                    if let Some(first) = suggestions.first().cloned() {
                        if let Some(idx) = input.rfind(',') {
                            input.truncate(idx + 1);
                            input.push(' ');
                            input.push_str(&first);
                        } else {
                            input = first;
                        }
                        input.push_str(", ");
                        suggestions = self.compute_tag_suggestions(&input);
                    }
                    self.modal = ModalState::EditTags {
                        id,
                        input,
                        suggestions,
                    };
                }
                KeyCode::Backspace => {
                    input.pop();
                    suggestions = self.compute_tag_suggestions(&input);
                    self.modal = ModalState::EditTags {
                        id,
                        input,
                        suggestions,
                    };
                }
                KeyCode::Char(c) => {
                    input.push(c);
                    suggestions = self.compute_tag_suggestions(&input);
                    self.modal = ModalState::EditTags {
                        id,
                        input,
                        suggestions,
                    };
                }
                _ => {
                    self.modal = ModalState::EditTags {
                        id,
                        input,
                        suggestions,
                    };
                }
            },
        }
        Ok((false, false))
    }
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, crossterm::cursor::Show);
    }
}

pub fn run_tui(db: &Database, config: &Config, db_path: PathBuf) -> Result<()> {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, crossterm::cursor::Show);
        original_hook(panic_info);
    }));

    enable_raw_mode()?;
    let _guard = TerminalGuard;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = App::new(db, config, db_path)?;
    run_app(&mut terminal, app)
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> Result<()> {
    let mut needs_redraw = true;

    loop {
        if needs_redraw {
            terminal.draw(|f| ui(f, &mut app))?;
            needs_redraw = false;
        }

        match event::read()? {
            Event::Resize(_, _) => {
                needs_redraw = true;
            }
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                needs_redraw = true;
                let (should_quit, clear_term) = app.handle_key(key)?;
                if clear_term {
                    terminal.clear()?;
                }
                if should_quit {
                    return Ok(());
                }
            }
            _ => {}
        }
    }
}

fn summary_display_title(note: &NoteSummary) -> String {
    match &note.title {
        Some(t) if !t.trim().is_empty() => t.clone(),
        _ => {
            let line = note.preview.lines().next().unwrap_or("").trim();
            if line.is_empty() {
                format!("Note #{}", note.id)
            } else {
                truncate_chars(line, 32)
            }
        }
    }
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        s.to_string()
    } else if max_chars <= 1 {
        "…".to_string()
    } else {
        let mut out: String = s.chars().take(max_chars - 1).collect();
        out.push('…');
        out
    }
}

fn format_relative_time(dt: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = now.signed_duration_since(dt).num_seconds().max(0);
    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else if secs < 30 * 86_400 {
        format!("{}d ago", secs / 86_400)
    } else {
        dt.format("%Y-%m-%d").to_string()
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    let size = f.size();
    let theme = app.resolved_theme.clone();

    if !theme.no_color {
        f.render_widget(Block::default().style(theme.surface_style()), size);
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Search bar
            Constraint::Min(0),    // Workspace or Insights
            Constraint::Length(2), // Compact status + hint footer
        ])
        .split(size);

    render_search_bar(f, app, &theme, chunks[0]);

    if app.show_insights {
        render_insights(f, app, &theme, chunks[1]);
    } else {
        render_workspace(f, app, &theme, chunks[1]);
    }

    render_footer(f, app, &theme, chunks[2]);
    render_modal(f, app, &theme, size);
}

fn render_search_bar(f: &mut Frame, app: &App, theme: &ResolvedTheme, area: Rect) {
    let mut title_spans = vec![Span::styled(
        " ◆ Search [/] ",
        theme.title_style(app.editing_search),
    )];
    if let Some(tag) = app.active_tag_filter() {
        title_spans.push(Span::styled("· Tag: ", theme.muted_style()));
        title_spans.push(Span::styled(
            format!("{} [c:clear] ", tag),
            theme.chip_style(),
        ));
    }
    if app.show_archived {
        title_spans.push(Span::styled("· +Archived ", theme.chip_style()));
    }

    let right_meta = format!(
        " {} notes · {} · wrap:{} ",
        app.notes.len(),
        theme.preset_name,
        if app.wrap_lines { "on" } else { "off" }
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(if app.editing_search {
            BorderType::Rounded
        } else {
            BorderType::Plain
        })
        .border_style(theme.border_style(app.editing_search))
        .title(Line::from(title_spans))
        .title(
            ratatui::widgets::block::Title::from(Span::styled(right_meta, theme.muted_style()))
                .alignment(Alignment::Right),
        );

    let search_line = if app.editing_search {
        Line::from(vec![
            Span::styled(" › ", theme.primary_accent()),
            Span::styled(
                &app.search_query,
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled("█", theme.primary_accent()),
        ])
    } else if app.search_query.is_empty() {
        Line::from(vec![
            Span::styled(" › ", theme.muted_style()),
            Span::styled("Press / to search titles and content…", theme.muted_style()),
        ])
    } else {
        Line::from(vec![
            Span::styled(" › ", theme.primary_accent()),
            Span::raw(&app.search_query),
        ])
    };

    let search = Paragraph::new(search_line).block(block);
    f.render_widget(search, area);
}

fn render_workspace(f: &mut Frame, app: &mut App, theme: &ResolvedTheme, area: Rect) {
    let compact_width = area.width < 80;

    if app.zoomed {
        match app.active_pane {
            Pane::Tags => render_tags_pane(f, app, theme, area),
            Pane::Notes => render_notes_pane(f, app, theme, area),
            Pane::Content => render_preview_pane(f, app, theme, area),
        }
        return;
    }

    if compact_width {
        if app.active_pane == Pane::Tags {
            let body_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
                .split(area);
            render_tags_pane(f, app, theme, body_chunks[0]);
            render_notes_pane(f, app, theme, body_chunks[1]);
        } else {
            let body_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
                .split(area);
            render_notes_pane(f, app, theme, body_chunks[0]);
            render_preview_pane(f, app, theme, body_chunks[1]);
        }
        return;
    }

    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(22), // Tags
            Constraint::Percentage(32), // Notes
            Constraint::Percentage(46), // Preview
        ])
        .split(area);

    render_tags_pane(f, app, theme, body_chunks[0]);
    render_notes_pane(f, app, theme, body_chunks[1]);
    render_preview_pane(f, app, theme, body_chunks[2]);
}

fn render_tags_pane(f: &mut Frame, app: &mut App, theme: &ResolvedTheme, area: Rect) {
    let active = app.active_pane == Pane::Tags;
    let items: Vec<ListItem> = app
        .tag_rows
        .iter()
        .map(|row| ListItem::new(format_tag_row(row)))
        .collect();

    let title = format!(" [Tags · {}] ", app.tag_rows.len().saturating_sub(1));
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(if active {
            BorderType::Rounded
        } else {
            BorderType::Plain
        })
        .border_style(theme.border_style(active))
        .title(Span::styled(title, theme.title_style(active)));

    let list = List::new(items)
        .block(block)
        .highlight_style(theme.selected_item_style())
        .highlight_symbol("▸ ");

    f.render_stateful_widget(list, area, &mut app.tag_state);
}

fn render_notes_pane(f: &mut Frame, app: &mut App, theme: &ResolvedTheme, area: Rect) {
    let active = app.active_pane == Pane::Notes;
    let now = Utc::now();
    let max_title_width = area.width.saturating_sub(10) as usize;
    let max_meta_width = area.width.saturating_sub(6) as usize;

    let title = format!(" [Notes · {}] ", app.notes.len());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(if active {
            BorderType::Rounded
        } else {
            BorderType::Plain
        })
        .border_style(theme.border_style(active))
        .title(Span::styled(title, theme.title_style(active)));

    if app.notes.is_empty() {
        let empty_msg = if app.search_query.is_empty() && app.selected_tag_filter.is_none() {
            vec![
                Line::from(""),
                Line::from(Span::styled("No notes yet", theme.primary_accent())),
                Line::from(Span::styled(
                    "Press [n] to capture your first thought",
                    theme.muted_style(),
                )),
            ]
        } else {
            vec![
                Line::from(""),
                Line::from(Span::styled("No matching notes", theme.primary_accent())),
                Line::from(Span::styled(
                    "Press [Esc] or [c] to clear",
                    theme.muted_style(),
                )),
            ]
        };
        let p = Paragraph::new(empty_msg)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true })
            .block(block);
        f.render_widget(p, area);
        return;
    }

    let items: Vec<ListItem> = app
        .notes
        .iter()
        .map(|n| {
            let pin = if n.pinned { "★ " } else { "" };
            let lock = if n.is_encrypted { "◆ " } else { "" };
            let disp = summary_display_title(n);
            let title_str = truncate_chars(
                &format!("{}{}#{} {}", pin, lock, n.id, disp),
                max_title_width,
            );

            let rel = format_relative_time(n.updated, now);
            let tags_str = if n.tags.is_empty() {
                String::new()
            } else {
                format!(" · #{}", n.tags.join(" #"))
            };
            let meta_raw = if let Some(ref snip) = n.snippet {
                format!("  {} · {}", rel, snip.replace('\n', " "))
            } else {
                format!("  {}{} · {}", rel, tags_str, n.source)
            };
            let meta_str = truncate_chars(&meta_raw, max_meta_width);

            ListItem::new(vec![
                Line::from(Span::raw(title_str)),
                Line::from(Span::styled(meta_str, theme.muted_style())),
            ])
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(theme.selected_item_style())
        .highlight_symbol("▸ ");

    f.render_stateful_widget(list, area, &mut app.list_state);

    // Render scrollbar when note items overflow viewport (each item takes 2 lines)
    let visible_items = (area.height.saturating_sub(2) / 2) as usize;
    if app.notes.len() > visible_items && visible_items > 0 {
        let pos = app.list_state.selected().unwrap_or(0);
        let mut sb_state = ScrollbarState::new(app.notes.len()).position(pos);
        let sb = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .thumb_symbol("█");
        let inner = Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: area.height.saturating_sub(2),
        };
        f.render_stateful_widget(sb, inner, &mut sb_state);
    }
}

fn render_preview_pane(f: &mut Frame, app: &mut App, theme: &ResolvedTheme, area: Rect) {
    let active = app.active_pane == Pane::Content;

    app.ensure_preview_cache();

    let Some(ref note) = app.selected_note else {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(if active {
                BorderType::Rounded
            } else {
                BorderType::Plain
            })
            .border_style(theme.border_style(active))
            .title(Span::styled(" [Preview] ", theme.title_style(active)));

        let msg = if app.notes.is_empty() {
            vec![
                Line::from(""),
                Line::from(Span::styled("Welcome to Jotun", theme.primary_accent())),
                Line::from(Span::styled(
                    "Press [n] to open $EDITOR or [/] to search",
                    theme.muted_style(),
                )),
            ]
        } else {
            vec![Line::from(Span::styled(
                "Select a note to preview",
                theme.muted_style(),
            ))]
        };
        f.render_widget(
            Paragraph::new(msg)
                .alignment(Alignment::Center)
                .block(block),
            area,
        );
        return;
    };

    let mut preview_lines: Vec<Line<'_>> = Vec::new();
    let title_text = note.title.as_deref().unwrap_or("Untitled");
    let mut header_spans = vec![Span::styled(title_text, theme.primary_accent())];
    if note.pinned {
        header_spans.push(Span::styled("  ★ pinned", theme.success_style()));
    }
    if note.archived {
        header_spans.push(Span::styled("  [archived]", theme.muted_style()));
    }
    preview_lines.push(Line::from(header_spans));

    let mut chip_spans: Vec<Span<'_>> = Vec::new();
    if note.tags.is_empty() {
        chip_spans.push(Span::styled("no tags", theme.muted_style()));
    } else {
        for (idx, tag) in note.tags.iter().enumerate() {
            if idx > 0 {
                chip_spans.push(Span::raw(" "));
            }
            chip_spans.push(Span::styled(format!("#{}", tag), theme.chip_style()));
        }
    }
    chip_spans.push(Span::styled(
        format!("   ● {}", note.source),
        theme.muted_style(),
    ));
    preview_lines.push(Line::from(chip_spans));

    let word_count = note.body.split_whitespace().count();
    let meta_line = format!(
        "Created {} · Updated {} · {}w / {}c",
        note.created.format("%Y-%m-%d"),
        format_relative_time(note.updated, Utc::now()),
        word_count,
        note.body.chars().count()
    );
    preview_lines.push(Line::from(Span::styled(meta_line, theme.muted_style())));

    let rule_width = area.width.saturating_sub(3) as usize;
    preview_lines.push(Line::from(Span::styled(
        "─".repeat(rule_width),
        theme.border_style(false),
    )));

    if let Some(ref cached) = app.cached_preview {
        for line in &cached.lines {
            let borrowed_spans: Vec<Span<'_>> = line
                .spans
                .iter()
                .map(|sp| Span::styled(sp.content.as_ref(), sp.style))
                .collect();
            preview_lines.push(Line::from(borrowed_spans));
        }
        if cached.truncated_highlight {
            preview_lines.push(Line::from(""));
            preview_lines.push(Line::from(Span::styled(
                "─ syntax highlight capped at 50 KB / 500 lines for instant scroll ─",
                theme.muted_style(),
            )));
        }
    }

    let total_lines = preview_lines.len().max(1);
    let inner_height = area.height.saturating_sub(2) as usize;
    let max_scroll = total_lines.saturating_sub(inner_height.max(1));
    let clamped_scroll = (app.preview_scroll as usize).min(max_scroll) as u16;
    app.preview_scroll = clamped_scroll;

    let pct = if max_scroll == 0 {
        100
    } else {
        ((clamped_scroll as usize * 100) / max_scroll).min(100)
    };

    let content_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if active {
            BorderType::Rounded
        } else {
            BorderType::Plain
        })
        .border_style(theme.border_style(active))
        .title(Span::styled(
            format!(" [Preview · #{}] ", note.id),
            theme.title_style(active),
        ))
        .title(
            ratatui::widgets::block::Title::from(Span::styled(
                format!(" {}% ", pct),
                theme.muted_style(),
            ))
            .alignment(Alignment::Right)
            .position(ratatui::widgets::block::Position::Bottom),
        );

    let mut viewer = Paragraph::new(Text::from(preview_lines))
        .block(content_block)
        .scroll((clamped_scroll, 0));
    if app.wrap_lines {
        viewer = viewer.wrap(Wrap { trim: false });
    }
    f.render_widget(viewer, area);

    if total_lines > inner_height && inner_height > 0 {
        let mut sb_state = ScrollbarState::new(total_lines).position(clamped_scroll as usize);
        let sb = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .thumb_symbol("█");
        let sb_area = Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: area.height.saturating_sub(2),
        };
        f.render_stateful_widget(sb, sb_area, &mut sb_state);
    }
}

fn render_insights(f: &mut Frame, app: &mut App, theme: &ResolvedTheme, area: Rect) {
    if app.cached_stats.is_none()
        && let Ok(s) = app.db.get_statistics()
    {
        app.cached_stats = Some(s);
    }

    let Some(ref stats) = app.cached_stats else {
        f.render_widget(
            Paragraph::new("Error loading statistics")
                .block(Block::default().borders(Borders::ALL)),
            area,
        );
        return;
    };

    let top_height = if area.height <= 19 { 7 } else { 9 };
    let stats_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(top_height), Constraint::Min(0)])
        .split(area);

    let top_chunks = if area.width < 70 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(stats_chunks[0])
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(48), Constraint::Percentage(52)])
            .split(stats_chunks[0])
    };

    // Sources Card
    let sources_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.border_style(false))
        .title(Span::styled(" [Sources] ", theme.title_style(true)));
    let sources_inner = sources_block.inner(top_chunks[0]);
    f.render_widget(sources_block, top_chunks[0]);

    if stats.top_sources.is_empty() {
        f.render_widget(
            Paragraph::new("No source data yet").style(theme.muted_style()),
            sources_inner,
        );
    } else {
        let rows_avail = sources_inner.height as usize;
        let show_count = stats.top_sources.len().min(rows_avail);
        if show_count > 0 {
            let source_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![Constraint::Length(1); show_count])
                .split(sources_inner);
            for (i, (source, count)) in stats.top_sources.iter().take(show_count).enumerate() {
                let percent = if stats.total_notes > 0 {
                    (*count as f64 / stats.total_notes as f64 * 100.0).min(100.0)
                } else {
                    0.0
                };
                let gauge_style = if theme.no_color {
                    Style::default()
                } else {
                    Style::default()
                        .fg(theme.accent_secondary)
                        .bg(theme.bg_elevated)
                };
                let gauge = Gauge::default()
                    .gauge_style(gauge_style)
                    .percent(percent as u16)
                    .label(format!("{:<8} {}/{}", source, count, stats.total_notes));
                f.render_widget(gauge, source_layout[i]);
            }
        }
    }

    // Knowledge Base Card
    let summary_text = vec![
        Line::from(vec![
            Span::styled("Notes:   ", theme.primary_accent()),
            Span::raw(format!(
                "{} ({} pinned, {} archived)",
                stats.total_notes, stats.pinned_count, stats.archived_count
            )),
        ]),
        Line::from(vec![
            Span::styled("Tags:    ", theme.primary_accent()),
            Span::raw(format!("{}", stats.tag_count)),
        ]),
        Line::from(vec![
            Span::styled("Storage: ", theme.primary_accent()),
            Span::raw(format!(
                "{:.1} KB (avg {:.0} chars)",
                stats.total_chars as f64 / 1024.0,
                stats.avg_note_length
            )),
        ]),
        Line::from(vec![
            Span::styled("DB:      ", theme.primary_accent()),
            Span::styled(
                app.db_path.to_string_lossy().to_string(),
                theme.muted_style(),
            ),
        ]),
    ];
    let summary = Paragraph::new(summary_text)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme.border_style(false))
                .title(Span::styled(" [Knowledge Base] ", theme.title_style(true))),
        );
    f.render_widget(summary, top_chunks[1]);

    // Recent Notes Table
    let header_style = if theme.no_color {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme.accent_primary)
            .add_modifier(Modifier::BOLD)
    };
    let header_bg = if theme.no_color {
        Style::default()
    } else {
        Style::default().bg(theme.bg_elevated)
    };
    let header_cells = ["ID", "TITLE", "TAGS", "SRC", "UPDATED"]
        .iter()
        .map(|h| Cell::from(*h).style(header_style));
    let header = Row::new(header_cells).style(header_bg).height(1);

    let now = Utc::now();
    let rows = app.notes.iter().take(12).map(|n| {
        let cells = vec![
            Cell::from(format!("#{}", n.id)),
            Cell::from(summary_display_title(n)),
            Cell::from(n.tags.join(", ")),
            Cell::from(n.source.clone()),
            Cell::from(format_relative_time(n.updated, now)).style(theme.muted_style()),
        ];
        Row::new(cells).height(1)
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Percentage(34),
            Constraint::Percentage(31),
            Constraint::Length(9),
            Constraint::Min(11),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.border_style(false))
            .title(Span::styled(" [Recent Notes] ", theme.title_style(true))),
    );
    f.render_widget(table, stats_chunks[1]);
}

fn render_footer(f: &mut Frame, app: &App, theme: &ResolvedTheme, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    let (glyph, status_style) = if app.status_is_error {
        ("!", theme.danger_style())
    } else {
        ("●", theme.success_style())
    };
    let status_line = Line::from(vec![
        Span::styled(format!(" {} ", glyph), status_style),
        Span::raw(&app.status_msg),
    ]);
    f.render_widget(Paragraph::new(status_line), rows[0]);

    let hint_text = if app.editing_search {
        " Enter Done · Esc Clear & Exit Search"
    } else if app.show_insights {
        " i Back to Notes · q Quit · ? Help"
    } else if area.width < 95 {
        " Tab Pane · j/k Nav · / Search · n New · r Ren · t Tag · d Del · ? Help"
    } else {
        " Tab Pane · j/k Nav · Space Fold · / Search · n/e New/Edit · r/t Ren/Tag · p/a Pin/Arch · w Wrap · z Zoom · i Stats · ? Help"
    };
    f.render_widget(
        Paragraph::new(Span::styled(hint_text, theme.muted_style())),
        rows[1],
    );
}

fn centered_rect(percent_x: u16, height: u16, r: Rect) -> Rect {
    let w = (r.width as u32 * percent_x as u32 / 100)
        .max(36.min(r.width as u32))
        .min(r.width as u32) as u16;
    let h = height.min(r.height);
    let x = r.x + r.width.saturating_sub(w) / 2;
    let y = r.y + r.height.saturating_sub(h) / 2;
    Rect::new(x, y, w, h)
}

fn render_modal(f: &mut Frame, app: &App, theme: &ResolvedTheme, size: Rect) {
    match &app.modal {
        ModalState::None => {}
        ModalState::ConfirmDelete { id, title } => {
            let area = centered_rect(64, 7, size);
            f.render_widget(Clear, area);
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme.danger_style())
                .style(theme.surface_style())
                .title(Span::styled(" Confirm Delete ", theme.danger_style()));
            let lines = vec![
                Line::from(""),
                Line::from(vec![
                    Span::raw("  Delete note "),
                    Span::styled(
                        format!("#{} \"{}\"", id, truncate_chars(title, 36)),
                        theme.primary_accent(),
                    ),
                    Span::raw("?"),
                ]),
                Line::from(Span::styled(
                    "  Removes note and prunes any orphan tags.",
                    theme.muted_style(),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  [y / Enter] ", theme.danger_style()),
                    Span::raw("Confirm    "),
                    Span::styled("[n / Esc] ", theme.primary_accent()),
                    Span::raw("Cancel"),
                ]),
            ];
            f.render_widget(
                Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .block(block),
                area,
            );
        }
        ModalState::Help => {
            let area = centered_rect(76, 13, size);
            f.render_widget(Clear, area);
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme.border_style(true))
                .style(theme.surface_style())
                .title(Span::styled(" Jotun Keybindings ", theme.title_style(true)));
            let lines = vec![
                Line::from(""),
                Line::from("  Tab / S-Tab   Cycle panes        n / e      New / Edit ($EDITOR)"),
                Line::from("  j/k, ↓/↑      Navigate / Scroll  r / t      Rename / Edit tags"),
                Line::from(
                    "  Space / h / l Fold tag tree      p / a / A  Pin / Archive / Show arch",
                ),
                Line::from("  PgUp / PgDn   Page preview       d / y      Delete / Copy (OSC 52)"),
                Line::from("  / or s        Search (c: clear)  w / z      Toggle wrap / Zoom pane"),
                Line::from("  i / ?         Insights / Help    q / Esc    Quit / Close"),
                Line::from(""),
                Line::from(Span::styled(
                    "  Press Esc, ?, or Enter to close",
                    theme.muted_style(),
                )),
            ];
            f.render_widget(Paragraph::new(lines).block(block), area);
        }
        ModalState::RenameNote { id, input } => {
            let area = centered_rect(64, 6, size);
            f.render_widget(Clear, area);
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme.border_style(true))
                .style(theme.surface_style())
                .title(Span::styled(
                    format!(" Rename Note #{} ", id),
                    theme.title_style(true),
                ));
            let lines = vec![
                Line::from(vec![
                    Span::styled(" › ", theme.primary_accent()),
                    Span::raw(input.as_str()),
                    Span::styled("█", theme.primary_accent()),
                ]),
                Line::from(""),
                Line::from(Span::styled(
                    "  Enter Save (empty clears title) · Esc Cancel",
                    theme.muted_style(),
                )),
            ];
            f.render_widget(Paragraph::new(lines).block(block), area);
        }
        ModalState::EditTags {
            id,
            input,
            suggestions,
        } => {
            let area = centered_rect(68, 8, size);
            f.render_widget(Clear, area);
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme.border_style(true))
                .style(theme.surface_style())
                .title(Span::styled(
                    format!(" Edit Tags · #{} ", id),
                    theme.title_style(true),
                ));
            let sugg_line = if suggestions.is_empty() {
                "  No tag suggestions".to_string()
            } else {
                format!("  Suggestions [Tab]: {}", suggestions.join(", "))
            };
            let lines = vec![
                Line::from(vec![
                    Span::styled(" › ", theme.primary_accent()),
                    Span::raw(input.as_str()),
                    Span::styled("█", theme.primary_accent()),
                ]),
                Line::from(""),
                Line::from(Span::styled(sugg_line, theme.chip_style())),
                Line::from(""),
                Line::from(Span::styled(
                    "  Comma-separated · Tab Complete · Enter Save · Esc Cancel",
                    theme.muted_style(),
                )),
            ];
            f.render_widget(Paragraph::new(lines).block(block), area);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::db::Database;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn test_app_navigation_and_pinning() -> Result<()> {
        let db = Database::in_memory()?;
        let config = Config::default();
        db.create_note("Note 1", Some("First"), "test", &[])?;
        let id2 = db.create_note("Note 2", Some("Second"), "test", &[])?;

        let mut app = App::new(&db, &config, PathBuf::from("test.db"))?;
        assert_eq!(app.notes.len(), 2);
        assert_eq!(app.list_state.selected(), Some(0));

        app.next();
        assert_eq!(app.list_state.selected(), Some(1));

        app.next();
        assert_eq!(app.list_state.selected(), Some(0));

        app.previous();
        assert_eq!(app.list_state.selected(), Some(1));

        // Pin currently selected note and verify it moves to index 0
        let selected_id = app.notes[1].id;
        app.handle_key(press(KeyCode::Char('p')))?;
        assert_eq!(app.notes[0].id, selected_id);
        assert!(app.notes[0].pinned);
        let _ = id2;
        Ok(())
    }

    #[test]
    fn test_app_search_filtering() -> Result<()> {
        let db = Database::in_memory()?;
        let config = Config::default();
        db.create_note("Apple pie recipe", Some("Apple"), "test", &[])?;
        db.create_note("Banana bread", Some("Banana"), "test", &[])?;

        let mut app = App::new(&db, &config, PathBuf::from("test.db"))?;
        app.search_query = String::from("Apple");
        app.update_notes()?;

        assert_eq!(app.notes.len(), 1);
        assert_eq!(app.notes[0].title.as_deref(), Some("Apple"));
        Ok(())
    }

    #[test]
    fn test_delete_modal_confirm_and_cancel() -> Result<()> {
        let db = Database::in_memory()?;
        let config = Config::default();
        db.create_note(
            "Only tagged note",
            Some("Target"),
            "test",
            &["ephemeral".to_string()],
        )?;

        let mut app = App::new(&db, &config, PathBuf::from("test.db"))?;
        assert_eq!(app.notes.len(), 1);

        // Press 'd' opens ConfirmDelete instead of immediate delete
        app.handle_key(press(KeyCode::Char('d')))?;
        assert!(matches!(app.modal, ModalState::ConfirmDelete { .. }));
        assert_eq!(app.notes.len(), 1);

        // Press 'n' cancels
        app.handle_key(press(KeyCode::Char('n')))?;
        assert_eq!(app.modal, ModalState::None);
        assert_eq!(app.notes.len(), 1);

        // Press 'd' then 'y' confirms deletion and prunes orphan tags
        app.handle_key(press(KeyCode::Char('d')))?;
        app.handle_key(press(KeyCode::Char('y')))?;
        assert_eq!(app.modal, ModalState::None);
        assert!(app.notes.is_empty());
        assert_eq!(app.list_state.selected(), None);
        assert_eq!(app.tag_rows.len(), 1); // Only "All"
        Ok(())
    }

    #[test]
    fn test_inline_rename_and_tags_modal_and_archive() -> Result<()> {
        let db = Database::in_memory()?;
        let config = Config::default();
        db.create_note("Body 2", Some("Other"), "test", &["personal".to_string()])?;
        let id = db.create_note("Body", Some("Old"), "test", &["work/jotun".to_string()])?;

        let mut app = App::new(&db, &config, PathBuf::from("test.db"))?;
        assert!(app.syntax_assets.is_none()); // Lazy until draw/ensure_preview_cache
        // Ensure we target `id` regardless of timestamp tie-breaking
        let pos = app.notes.iter().position(|n| n.id == id).unwrap();
        app.list_state.select(Some(pos));
        app.load_selected_note();

        // Rename via modal
        app.handle_key(press(KeyCode::Char('r')))?;
        assert!(matches!(app.modal, ModalState::RenameNote { .. }));
        app.handle_key(press(KeyCode::Backspace))?;
        app.handle_key(press(KeyCode::Backspace))?;
        app.handle_key(press(KeyCode::Backspace))?;
        for c in "Renamed".chars() {
            app.handle_key(press(KeyCode::Char(c)))?;
        }
        app.handle_key(press(KeyCode::Enter))?;
        assert_eq!(db.get_note(id)?.title.as_deref(), Some("Renamed"));

        // Edit tags via modal
        app.handle_key(press(KeyCode::Char('t')))?;
        assert!(matches!(app.modal, ModalState::EditTags { .. }));
        app.handle_key(press(KeyCode::Char(',')))?;
        for c in "rust".chars() {
            app.handle_key(press(KeyCode::Char(c)))?;
        }
        app.handle_key(press(KeyCode::Enter))?;
        let note = db.get_note(id)?;
        assert!(note.tags.contains(&"rust".to_string()));

        // Archive and toggle show_archived ('A')
        app.handle_key(press(KeyCode::Char('a')))?;
        assert_eq!(app.notes.len(), 1);
        app.handle_key(KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT))?;
        assert_eq!(app.notes.len(), 2);
        Ok(())
    }

    #[test]
    fn test_responsive_renders_modals_and_insights_snapshots() -> Result<()> {
        let db = Database::in_memory()?;
        let config = Config::default();
        db.create_note(
            "# Heading\n\n```rust\nfn main() {}\n```",
            Some("Auth Fix"),
            "tui",
            &["work/jotun".to_string(), "critical".to_string()],
        )?;

        let mut app = App::new(&db, &config, PathBuf::from("/tmp/jotun.db"))?;

        // 80x24 main + help modal + insights
        let backend_80 = ratatui::backend::TestBackend::new(80, 24);
        let mut term_80 = Terminal::new(backend_80)?;
        term_80.draw(|f| ui(f, &mut app))?;
        assert!(app.syntax_assets.is_some()); // Loaded on first draw

        app.modal = ModalState::Help;
        term_80.draw(|f| ui(f, &mut app))?;
        app.modal = ModalState::None;

        app.show_insights = true;
        term_80.draw(|f| ui(f, &mut app))?;
        app.show_insights = false;

        // 120x36 wide render
        let backend_120 = ratatui::backend::TestBackend::new(120, 36);
        let mut term_120 = Terminal::new(backend_120)?;
        term_120.draw(|f| ui(f, &mut app))?;

        // < 80 cols compact breakpoint (64x20)
        let backend_narrow = ratatui::backend::TestBackend::new(64, 20);
        let mut term_narrow = Terminal::new(backend_narrow)?;
        term_narrow.draw(|f| ui(f, &mut app))?;
        Ok(())
    }

    #[test]
    fn test_empty_db_navigation_pane_cycle_and_render() -> Result<()> {
        let db = Database::in_memory()?;
        let config = Config::default();
        let mut app = App::new(&db, &config, PathBuf::from("empty.db"))?;

        assert!(app.notes.is_empty());
        assert_eq!(app.list_state.selected(), None);
        app.next();
        app.previous();
        assert_eq!(app.list_state.selected(), None);

        assert!(app.active_pane == Pane::Notes);
        app.switch_pane();
        assert!(app.active_pane == Pane::Content);
        app.switch_pane();
        assert!(app.active_pane == Pane::Tags);
        app.switch_pane();
        assert!(app.active_pane == Pane::Notes);

        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| ui(f, &mut app))?;
        assert!(app.syntax_assets.is_none()); // Still not loaded on empty DB!
        app.show_insights = true;
        terminal.draw(|f| ui(f, &mut app))?;
        Ok(())
    }
}
