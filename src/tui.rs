use crate::config::Config;
use crate::db::{Database, Note, Statistics};
use anyhow::Result;
use chrono::{DateTime, Utc};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::style::Stylize;
use ratatui::{
    Frame, Terminal,
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Cell, Gauge, List, ListItem, ListState, Paragraph, Row, Table, Wrap,
    },
};
use std::io;
use std::path::PathBuf;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

#[derive(PartialEq)]
enum Pane {
    Tags,
    Notes,
    Content,
}

struct CachedPreview {
    note_id: i64,
    updated: DateTime<Utc>,
    syntax_highlighting: bool,
    no_color: bool,
    lines: Vec<Line<'static>>,
}

pub struct App<'a> {
    db: &'a Database,
    config: &'a Config,
    notes: Vec<Note>,
    tags: Vec<String>,
    list_state: ListState,
    tag_state: ListState,
    search_query: String,
    status_msg: String,
    editing_search: bool,
    active_pane: Pane,
    syntax_set: SyntaxSet,
    theme: Theme,
    show_insights: bool,
    db_path: PathBuf,
    preview_scroll: u16,
    cached_preview: Option<CachedPreview>,
    cached_stats: Option<Statistics>,
    no_color: bool,
}

impl<'a> App<'a> {
    pub fn new(db: &'a Database, config: &'a Config, db_path: PathBuf) -> Result<Self> {
        let notes = db.list_notes()?;
        let mut tags = vec!["All".to_string()];
        tags.extend(db.list_all_tags()?);

        let mut list_state = ListState::default();
        if !notes.is_empty() {
            list_state.select(Some(0));
        }

        let mut tag_state = ListState::default();
        tag_state.select(Some(0));

        let ps = SyntaxSet::load_defaults_newlines();
        let ts = ThemeSet::load_defaults();
        let theme = ts.themes["base16-ocean.dark"].clone();
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());

        Ok(Self {
            db,
            config,
            notes,
            tags,
            list_state,
            tag_state,
            search_query: String::new(),
            status_msg: String::from("Welcome to Jotun!"),
            editing_search: false,
            active_pane: Pane::Notes,
            syntax_set: ps,
            theme,
            show_insights: false,
            db_path,
            preview_scroll: 0,
            cached_preview: None,
            cached_stats: None,
            no_color,
        })
    }

    fn selected_tag_filter(&self) -> Option<&str> {
        self.tag_state
            .selected()
            .and_then(|i| self.tags.get(i))
            .map(String::as_str)
            .filter(|tag| *tag != "All")
    }

    fn refresh_tags(&mut self) -> Result<()> {
        let prev_tag = self
            .tag_state
            .selected()
            .and_then(|i| self.tags.get(i))
            .cloned();

        let mut tags = vec!["All".to_string()];
        tags.extend(self.db.list_all_tags()?);
        self.tags = tags;

        let new_idx = prev_tag
            .and_then(|name| self.tags.iter().position(|t| t == &name))
            .unwrap_or(0);
        self.tag_state.select(Some(new_idx));
        Ok(())
    }

    fn update_notes(&mut self) -> Result<()> {
        let selected_tag = self.selected_tag_filter().map(str::to_owned);

        let res = if !self.search_query.is_empty() {
            self.db.search_notes(&self.search_query).map(|notes| {
                if let Some(ref tag) = selected_tag {
                    let child_prefix = format!("{}/", tag);
                    notes
                        .into_iter()
                        .filter(|n| {
                            n.tags
                                .iter()
                                .any(|t| t == tag || t.starts_with(&child_prefix))
                        })
                        .collect()
                } else {
                    notes
                }
            })
        } else if let Some(ref tag) = selected_tag {
            self.db.find_by_tag(tag)
        } else {
            self.db.list_notes()
        };

        match res {
            Ok(notes) => {
                self.notes = notes;
                if !self.search_query.is_empty() {
                    self.status_msg = format!("Found {} results", self.notes.len());
                }
            }
            Err(e) => {
                self.notes = Vec::new();
                self.status_msg = format!("❌ Error fetching notes: {}", e);
            }
        }

        if self.notes.is_empty() {
            self.list_state.select(None);
        } else {
            let selected = self.list_state.selected().unwrap_or(0);
            self.list_state
                .select(Some(selected.min(self.notes.len().saturating_sub(1))));
        }
        self.preview_scroll = 0;
        Ok(())
    }

    pub fn next(&mut self) {
        match self.active_pane {
            Pane::Notes => {
                if self.notes.is_empty() {
                    return;
                }
                let i = match self.list_state.selected() {
                    Some(i) => {
                        if i >= self.notes.len() - 1 {
                            0
                        } else {
                            i + 1
                        }
                    }
                    None => 0,
                };
                self.list_state.select(Some(i));
                self.preview_scroll = 0;
            }
            Pane::Tags => {
                if self.tags.is_empty() {
                    return;
                }
                let i = match self.tag_state.selected() {
                    Some(i) => {
                        if i >= self.tags.len() - 1 {
                            0
                        } else {
                            i + 1
                        }
                    }
                    None => 0,
                };
                self.tag_state.select(Some(i));
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
                    Some(i) => {
                        if i == 0 {
                            self.notes.len() - 1
                        } else {
                            i - 1
                        }
                    }
                    None => 0,
                };
                self.list_state.select(Some(i));
                self.preview_scroll = 0;
            }
            Pane::Tags => {
                if self.tags.is_empty() {
                    return;
                }
                let i = match self.tag_state.selected() {
                    Some(i) => {
                        if i == 0 {
                            self.tags.len() - 1
                        } else {
                            i - 1
                        }
                    }
                    None => 0,
                };
                self.tag_state.select(Some(i));
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

    pub fn delete_current(&mut self) -> Result<()> {
        if self.active_pane != Pane::Notes {
            return Ok(());
        }
        if let Some(i) = self.list_state.selected()
            && let Some(note) = self.notes.get(i)
        {
            let id = note.id;
            self.db.delete_note(id)?;
            self.status_msg = format!("Deleted note #{}", id);
            self.cached_stats = None;
            self.cached_preview = None;
            self.refresh_tags()?;
            self.update_notes()?;
        }
        Ok(())
    }

    pub fn copy_current(&mut self) -> Result<()> {
        if let Some(i) = self.list_state.selected()
            && let Some(note) = self.notes.get(i)
        {
            crate::copy_to_clipboard(&note.body)?;
            self.status_msg = format!("Copied note #{} to clipboard", note.id);
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

        crate::spawn_editor(self.config, path, false)
    }

    pub fn edit_current(&mut self) -> Result<()> {
        if let Some(i) = self.list_state.selected()
            && let Some(note) = self.notes.get(i).cloned()
        {
            let temp_file = crate::create_markdown_tempfile(&note.body)?;
            let status = self.run_external_editor(temp_file.path())?;
            if status.success() {
                let updated_body = crate::read_edited_body(temp_file.path())?;
                if updated_body != note.body {
                    self.db.update_note(
                        note.id,
                        &updated_body,
                        note.title.as_deref(),
                        &note.tags,
                    )?;
                    self.status_msg = format!("Updated note #{}", note.id);
                    self.cached_stats = None;
                    self.cached_preview = None;
                    self.refresh_tags()?;
                    self.update_notes()?;
                }
            } else {
                self.status_msg = String::from("❌ Editor exited with non-zero status");
            }
        }
        Ok(())
    }

    pub fn create_new(&mut self) -> Result<()> {
        let temp_file = crate::create_markdown_tempfile("")?;
        let status = self.run_external_editor(temp_file.path())?;

        if status.success() {
            let content = crate::read_edited_body(temp_file.path())?;
            if !content.trim().is_empty() {
                let initial_tags: Vec<String> = self
                    .selected_tag_filter()
                    .map(|t| vec![t.to_string()])
                    .unwrap_or_default();
                let id = self.db.create_note(&content, None, "tui", &initial_tags)?;
                self.status_msg = format!("Created note #{}", id);
                self.cached_stats = None;
                self.cached_preview = None;
                self.refresh_tags()?;
                self.update_notes()?;
            }
        } else {
            self.status_msg = String::from("❌ Editor exited with non-zero status");
        }
        Ok(())
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
                if app.editing_search {
                    match key.code {
                        KeyCode::Enter | KeyCode::Esc => app.editing_search = false,
                        KeyCode::Char(c) => {
                            app.search_query.push(c);
                            if let Err(e) = app.update_notes() {
                                app.status_msg = format!("❌ {}", e);
                            }
                        }
                        KeyCode::Backspace => {
                            app.search_query.pop();
                            if let Err(e) = app.update_notes() {
                                app.status_msg = format!("❌ {}", e);
                            }
                        }
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Char('s') | KeyCode::Char('/') => app.editing_search = true,
                        KeyCode::Tab => app.switch_pane(),
                        KeyCode::Down | KeyCode::Char('j') => app.next(),
                        KeyCode::Up | KeyCode::Char('k') => app.previous(),
                        KeyCode::PageDown => app.page_down(),
                        KeyCode::PageUp => app.page_up(),
                        KeyCode::Char('y') => {
                            if let Err(e) = app.copy_current() {
                                app.status_msg = format!("❌ {}", e);
                            }
                        }
                        KeyCode::Char('d') => {
                            if let Err(e) = app.delete_current() {
                                app.status_msg = format!("❌ {}", e);
                            }
                        }
                        KeyCode::Char('e') => {
                            if let Err(e) = app.edit_current() {
                                app.status_msg = format!("❌ {}", e);
                            }
                            terminal.clear()?;
                        }
                        KeyCode::Char('n') => {
                            if let Err(e) = app.create_new() {
                                app.status_msg = format!("❌ {}", e);
                            }
                            terminal.clear()?;
                        }
                        KeyCode::Char('i') => {
                            app.show_insights = !app.show_insights;
                            if app.show_insights && app.cached_stats.is_none() {
                                match app.db.get_statistics() {
                                    Ok(stats) => app.cached_stats = Some(stats),
                                    Err(e) => app.status_msg = format!("❌ {}", e),
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

fn parse_color(color: &str) -> Color {
    let color = color.trim();
    if color.starts_with('#')
        && color.len() == 7
        && color.is_ascii()
        && let Ok(r) = u8::from_str_radix(&color[1..3], 16)
        && let Ok(g) = u8::from_str_radix(&color[3..5], 16)
        && let Ok(b) = u8::from_str_radix(&color[5..7], 16)
    {
        return Color::Rgb(r, g, b);
    }
    match color.to_lowercase().as_str() {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" => Color::Gray,
        "dark_gray" => Color::DarkGray,
        "light_red" => Color::LightRed,
        "light_green" => Color::LightGreen,
        "light_yellow" => Color::LightYellow,
        "light_blue" => Color::LightBlue,
        "light_magenta" => Color::LightMagenta,
        "light_cyan" => Color::LightCyan,
        "white" => Color::White,
        _ => Color::Reset,
    }
}

fn render_highlighted_lines(app: &mut App, note: &Note) -> Vec<Line<'static>> {
    let syntax_highlighting = app.config.syntax_highlighting.unwrap_or(true);
    let no_color = app.no_color;

    if let Some(ref cached) = app.cached_preview
        && cached.note_id == note.id
        && cached.updated == note.updated
        && cached.syntax_highlighting == syntax_highlighting
        && cached.no_color == no_color
    {
        return cached.lines.clone();
    }

    let mut lines = Vec::new();
    if syntax_highlighting && !no_color && !note.body.is_empty() {
        let syntax = app
            .syntax_set
            .find_syntax_by_extension("md")
            .unwrap_or_else(|| app.syntax_set.find_syntax_plain_text());
        let mut h = HighlightLines::new(syntax, &app.theme);

        for line in LinesWithEndings::from(&note.body) {
            let ranges: Vec<(syntect::highlighting::Style, &str)> =
                h.highlight_line(line, &app.syntax_set).unwrap_or_default();
            let spans: Vec<Span<'static>> = ranges
                .into_iter()
                .map(|(style, text)| {
                    let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
                    Span::styled(text.to_string(), Style::default().fg(fg))
                })
                .collect();
            lines.push(Line::from(spans));
        }
    } else {
        for line in note.body.lines() {
            lines.push(Line::from(line.to_string()));
        }
    }

    app.cached_preview = Some(CachedPreview {
        note_id: note.id,
        updated: note.updated,
        syntax_highlighting,
        no_color,
        lines: lines.clone(),
    });
    lines
}

fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Search
            Constraint::Min(0),    // Main
            Constraint::Length(4), // Status & Help (2 inner lines for 80x24 wrapping)
        ])
        .split(f.size());

    let (active_color, highlight_bg, highlight_fg) = if app.no_color {
        (Color::Reset, Color::Reset, Color::Reset)
    } else {
        (
            parse_color(&app.config.theme.active_border),
            parse_color(&app.config.theme.highlight_bg),
            parse_color(&app.config.theme.highlight_fg),
        )
    };

    // 1. Search Bar
    let search_style = if app.editing_search && !app.no_color {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else if app.editing_search {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    let search = Paragraph::new(format!(" 🔍 SEARCH: {}", app.search_query))
        .style(search_style)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Jotun Search (Press '/' to type)"),
        );
    f.render_widget(search, chunks[0]);

    // 2. Main Body (Three Panes or Insights)
    if app.show_insights {
        if app.cached_stats.is_none() {
            match app.db.get_statistics() {
                Ok(s) => app.cached_stats = Some(s),
                Err(e) => app.status_msg = format!("❌ Error loading statistics: {}", e),
            }
        }

        if let Some(stats) = app.cached_stats.clone() {
            let stats_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(10), // Top Dash
                    Constraint::Min(0),     // Latest Notes Table
                ])
                .split(chunks[1]);

            let top_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(stats_chunks[0]);

            // --- Left: Source Meters (htop style) ---
            let source_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![Constraint::Length(2); stats.top_sources.len()])
                .split(top_chunks[0]);

            for (i, (source, count)) in stats.top_sources.iter().enumerate() {
                if i >= source_layout.len() {
                    break;
                }
                let percent = if stats.total_notes > 0 {
                    (*count as f64 / stats.total_notes as f64 * 100.0).min(100.0)
                } else {
                    0.0
                };
                let gauge_style = if app.no_color {
                    Style::default()
                } else {
                    Style::default().fg(Color::Cyan).bg(Color::Rgb(68, 71, 90))
                };
                let gauge = Gauge::default()
                    .block(Block::default().title(format!(" {} ", source.to_uppercase())))
                    .gauge_style(gauge_style)
                    .percent(percent as u16)
                    .label(format!("{} / {}", count, stats.total_notes));
                f.render_widget(gauge, source_layout[i]);
            }

            // --- Right: Summary Stats (Card style) ---
            let summary_text = vec![
                Line::from(vec![
                    Span::raw("Notes:   ").cyan(),
                    stats.total_notes.to_string().bold().white(),
                ]),
                Line::from(vec![
                    Span::raw("Tags:    ").cyan(),
                    stats.tag_count.to_string().bold().white(),
                ]),
                Line::from(vec![
                    Span::raw("Storage: ").cyan(),
                    format!("{:.1} KB", stats.total_chars as f64 / 1024.0)
                        .bold()
                        .white(),
                ]),
                Line::from(vec![
                    Span::raw("Average: ").cyan(),
                    format!("{:.0} chars", stats.avg_note_length).bold().white(),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::raw("DB:      ").cyan(),
                    app.db_path.to_string_lossy().to_string().dim(),
                ]),
            ];
            let summary = Paragraph::new(summary_text)
                .wrap(Wrap { trim: true })
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" SYSTEM INFO "),
                );
            f.render_widget(summary, top_chunks[1]);

            // --- Bottom: Latest Notes Table (htop process list style) ---
            let header_style = if app.no_color {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(Color::Rgb(255, 121, 198))
                    .add_modifier(Modifier::BOLD)
            };
            let header_bg = if app.no_color {
                Style::default()
            } else {
                Style::default().bg(Color::Rgb(68, 71, 90))
            };
            let header_cells = ["ID", "TITLE", "TAGS", "SOURCE", "UPDATED"]
                .iter()
                .map(|h| Cell::from(*h).style(header_style));
            let header = Row::new(header_cells)
                .style(header_bg)
                .height(1)
                .bottom_margin(1);

            let rows = app.notes.iter().take(10).map(|n| {
                let cells = vec![
                    Cell::from(n.id.to_string()).fg(Color::Cyan),
                    Cell::from(n.title.as_deref().unwrap_or("Untitled")).bold(),
                    Cell::from(n.tags.join(", ")).fg(Color::Yellow),
                    Cell::from(n.source.clone()).fg(Color::Green),
                    Cell::from(n.updated.format("%Y-%m-%d %H:%M").to_string()).dim(),
                ];
                Row::new(cells).height(1)
            });

            let table = Table::new(
                rows,
                [
                    Constraint::Length(5),
                    Constraint::Percentage(30),
                    Constraint::Percentage(30),
                    Constraint::Length(10),
                    Constraint::Min(20),
                ],
            )
            .header(header)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" RECENT NOTES "),
            );
            f.render_widget(table, stats_chunks[1]);
        } else {
            f.render_widget(
                Paragraph::new("Error loading statistics")
                    .block(Block::default().borders(Borders::ALL)),
                chunks[1],
            );
        }
    } else {
        let body_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(20), // Tags
                Constraint::Percentage(30), // Notes
                Constraint::Percentage(50), // Content
            ])
            .split(chunks[1]);

        // 2.1 Tags Sidebar
        let tags_items: Vec<ListItem> =
            app.tags.iter().map(|t| ListItem::new(t.as_str())).collect();

        let tags_block = Block::default()
            .borders(Borders::ALL)
            .title("Tags")
            .border_style(if app.active_pane == Pane::Tags {
                Style::default().fg(active_color)
            } else {
                Style::default()
            });

        let tags_list = List::new(tags_items)
            .block(tags_block)
            .highlight_style(
                Style::default()
                    .bg(highlight_bg)
                    .fg(highlight_fg)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        f.render_stateful_widget(tags_list, body_chunks[0], &mut app.tag_state);

        // 2.2 Notes List
        let notes_items: Vec<ListItem> = app
            .notes
            .iter()
            .map(|n| {
                let title = match &n.title {
                    Some(t) => t.clone(),
                    None => n
                        .body
                        .lines()
                        .next()
                        .unwrap_or("")
                        .chars()
                        .take(20)
                        .collect::<String>(),
                };
                ListItem::new(format!("[{}] {}", n.id, title))
            })
            .collect();

        let notes_block = Block::default()
            .borders(Borders::ALL)
            .title("Notes")
            .border_style(if app.active_pane == Pane::Notes {
                Style::default().fg(active_color)
            } else {
                Style::default()
            });

        let notes_list = List::new(notes_items)
            .block(notes_block)
            .highlight_style(
                Style::default()
                    .bg(highlight_bg)
                    .fg(highlight_fg)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">> ");

        f.render_stateful_widget(notes_list, body_chunks[1], &mut app.list_state);

        // 2.3 Content Viewer
        let selected_note = app
            .list_state
            .selected()
            .and_then(|i| app.notes.get(i))
            .cloned();

        let mut preview_lines = Vec::new();
        if let Some(note) = selected_note {
            let tags = if note.tags.is_empty() {
                "No tags".to_string()
            } else {
                note.tags.join(", ")
            };
            let meta = format!(
                "Tags: {} | Source: {} | Updated: {}",
                tags,
                note.source,
                note.updated.format("%Y-%m-%d %H:%M")
            );
            let sep_len = meta
                .chars()
                .count()
                .min(body_chunks[2].width.saturating_sub(2) as usize);
            let dim_style = Style::default().add_modifier(Modifier::DIM);
            preview_lines.push(Line::from(Span::styled(meta, dim_style)));
            preview_lines.push(Line::from(Span::styled("-".repeat(sep_len), dim_style)));
            preview_lines.push(Line::from(""));
            preview_lines.extend(render_highlighted_lines(app, &note));
        }

        let content_block = Block::default()
            .borders(Borders::ALL)
            .title("Preview")
            .border_style(if app.active_pane == Pane::Content {
                Style::default().fg(active_color)
            } else {
                Style::default()
            });

        let viewer = Paragraph::new(Text::from(preview_lines))
            .block(content_block)
            .scroll((app.preview_scroll, 0))
            .wrap(Wrap { trim: false });
        f.render_widget(viewer, body_chunks[2]);
    }

    // 3. Status & Help
    let help_text = if app.editing_search {
        "[Enter] Done  [Esc] Cancel search"
    } else if app.show_insights {
        "[i] Back to Notes  [q] Quit"
    } else {
        "[Tab] Pane [j/k] Nav [/] Search [n] New [y] Copy [e] Edit [d] Del [i] Stats [q] Quit"
    };

    let status_bar = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(chunks[2]);

    let msg = Paragraph::new(format!(" 📟 {}", app.status_msg))
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::ALL).title("Status"));
    f.render_widget(msg, status_bar[0]);

    let help = Paragraph::new(help_text)
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::ALL).title("Help"));
    f.render_widget(help, status_bar[1]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::db::Database;

    #[test]
    fn test_app_navigation() -> Result<()> {
        let db = Database::in_memory()?;
        let config = Config::default();
        db.create_note("Note 1", None, "test", &[])?;
        db.create_note("Note 2", None, "test", &[])?;

        let mut app = App::new(&db, &config, PathBuf::from("test.db"))?;
        assert_eq!(app.notes.len(), 2);
        assert_eq!(app.list_state.selected(), Some(0));

        app.next();
        assert_eq!(app.list_state.selected(), Some(1));

        app.next(); // Wrap around
        assert_eq!(app.list_state.selected(), Some(0));

        app.previous(); // Wrap around
        assert_eq!(app.list_state.selected(), Some(1));

        Ok(())
    }

    #[test]
    fn test_app_search_filtering() -> Result<()> {
        let db = Database::in_memory()?;
        let config = Config::default();
        db.create_note("Apple", None, "test", &[])?;
        db.create_note("Banana", None, "test", &[])?;

        let mut app = App::new(&db, &config, PathBuf::from("test.db"))?;
        app.search_query = String::from("Apple");
        app.update_notes()?;

        assert_eq!(app.notes.len(), 1);
        assert_eq!(app.notes[0].body, "Apple");

        Ok(())
    }

    #[test]
    fn test_app_delete_refreshes_tags_and_selection() -> Result<()> {
        let db = Database::in_memory()?;
        let config = Config::default();
        db.create_note("Only tagged note", None, "test", &["ephemeral".to_string()])?;

        let mut app = App::new(&db, &config, PathBuf::from("test.db"))?;
        assert_eq!(app.tags, vec!["All".to_string(), "ephemeral".to_string()]);
        assert_eq!(app.notes.len(), 1);

        app.delete_current()?;
        assert!(app.notes.is_empty());
        assert_eq!(app.list_state.selected(), None);
        assert_eq!(app.tags, vec!["All".to_string()]);
        assert_eq!(app.tag_state.selected(), Some(0));

        Ok(())
    }

    #[test]
    fn test_parse_color_utf8_and_fallbacks() {
        assert_eq!(parse_color("#bd93f9"), Color::Rgb(189, 147, 249));
        assert_eq!(parse_color("cyan"), Color::Cyan);
        // 7-byte multi-byte UTF-8 string must not panic when slicing
        assert_eq!(parse_color("#ééé"), Color::Reset);
        assert_eq!(parse_color("invalid"), Color::Reset);
    }

    #[test]
    fn test_content_pane_scroll_and_80x24_render() -> Result<()> {
        let db = Database::in_memory()?;
        let config = Config::default();
        db.create_note(
            "Line 1\nLine 2\nLine 3",
            Some("Title"),
            "manual",
            &["work".to_string()],
        )?;

        let mut app = App::new(&db, &config, PathBuf::from("/tmp/long/path/to/jotun.db"))?;
        app.switch_pane(); // Notes -> Content
        assert!(app.active_pane == Pane::Content);
        app.next();
        assert_eq!(app.preview_scroll, 1);
        app.page_down();
        assert_eq!(app.preview_scroll, 11);
        app.previous();
        assert_eq!(app.preview_scroll, 10);

        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| ui(f, &mut app))?;

        app.show_insights = true;
        terminal.draw(|f| ui(f, &mut app))?;
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
        app.show_insights = true;
        terminal.draw(|f| ui(f, &mut app))?;
        Ok(())
    }
}
