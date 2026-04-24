use crate::db::{Database, Note};
use crate::config::Config;
use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;
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
}

impl<'a> App<'a> {
    pub fn new(db: &'a Database, config: &'a Config) -> Result<Self> {
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
        })
    }

    fn update_notes(&mut self) -> Result<()> {
        let selected_tag = self.tag_state.selected().map(|i| &self.tags[i]);
        
        let res = if !self.search_query.is_empty() {
            self.db.search_notes(&self.search_query)
        } else if let Some(tag) = selected_tag {
            if tag == "All" {
                self.db.list_notes()
            } else {
                self.db.find_by_tag(tag)
            }
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
            Err(_) => {
                self.notes = Vec::new();
                self.status_msg = String::from("❌ Error fetching notes");
            }
        }
        
        if self.notes.is_empty() {
            self.list_state.select(None);
        } else {
            let selected = self.list_state.selected().unwrap_or(0);
            self.list_state.select(Some(selected.min(self.notes.len().saturating_sub(1))));
        }
        Ok(())
    }

    pub fn next(&mut self) {
        match self.active_pane {
            Pane::Notes => {
                if self.notes.is_empty() { return; }
                let i = match self.list_state.selected() {
                    Some(i) => if i >= self.notes.len() - 1 { 0 } else { i + 1 },
                    None => 0,
                };
                self.list_state.select(Some(i));
            }
            Pane::Tags => {
                if self.tags.is_empty() { return; }
                let i = match self.tag_state.selected() {
                    Some(i) => if i >= self.tags.len() - 1 { 0 } else { i + 1 },
                    None => 0,
                };
                self.tag_state.select(Some(i));
                let _ = self.update_notes();
            }
            _ => {}
        }
    }

    pub fn previous(&mut self) {
        match self.active_pane {
            Pane::Notes => {
                if self.notes.is_empty() { return; }
                let i = match self.list_state.selected() {
                    Some(i) => if i == 0 { self.notes.len() - 1 } else { i - 1 },
                    None => 0,
                };
                self.list_state.select(Some(i));
            }
            Pane::Tags => {
                if self.tags.is_empty() { return; }
                let i = match self.tag_state.selected() {
                    Some(i) => if i == 0 { self.tags.len() - 1 } else { i - 1 },
                    None => 0,
                };
                self.tag_state.select(Some(i));
                let _ = self.update_notes();
            }
            _ => {}
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
        if self.active_pane != Pane::Notes { return Ok(()); }
        if let Some(i) = self.list_state.selected() {
            let id = self.notes[i].id;
            self.db.delete_note(id)?;
            self.status_msg = format!("Deleted note #{}", id);
            self.update_notes()?;
        }
        Ok(())
    }

    pub fn copy_current(&mut self) -> Result<()> {
        if let Some(i) = self.list_state.selected() {
            crate::copy_to_clipboard(&self.notes[i].body)?;
            self.status_msg = format!("Copied note #{} to clipboard", self.notes[i].id);
        }
        Ok(())
    }

    pub fn edit_current(&mut self) -> Result<()> {
        if let Some(i) = self.list_state.selected() {
            let note = self.notes[i].clone();
            let preferred_editor = self.config.editor.clone()
                .or_else(|| std::env::var("VISUAL").ok())
                .or_else(|| std::env::var("EDITOR").ok())
                .unwrap_or_else(|| "nano".to_string());
            
            let temp_file = tempfile::NamedTempFile::new()?;
            std::fs::write(temp_file.path(), &note.body)?;

            disable_raw_mode()?;
            execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
            
            let status = std::process::Command::new(&preferred_editor)
                .arg(temp_file.path())
                .status();

            let status = match status {
                Ok(s) => s,
                Err(_) if preferred_editor != "nano" => {
                    std::process::Command::new("nano")
                        .arg(temp_file.path())
                        .status()?
                }
                Err(e) => {
                    enable_raw_mode()?;
                    execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
                    return Err(anyhow::anyhow!("Failed to launch editor: {}", e));
                }
            };

            enable_raw_mode()?;
            execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;

            if status.success() {
                let updated_body = std::fs::read_to_string(temp_file.path())?;
                let updated_body = updated_body.trim();
                if updated_body != note.body {
                    self.db.update_note(note.id, updated_body, note.title.as_deref(), &note.tags)?;
                    self.status_msg = format!("Updated note #{}", note.id);
                    self.update_notes()?;
                }
            }
        }
        Ok(())
    }

    pub fn create_new(&mut self) -> Result<()> {
        let preferred_editor = self.config.editor.clone()
            .or_else(|| std::env::var("VISUAL").ok())
            .or_else(|| std::env::var("EDITOR").ok())
            .unwrap_or_else(|| "nano".to_string());
        
        let temp_file = tempfile::NamedTempFile::new()?;

        disable_raw_mode()?;
        execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
        
        let status = std::process::Command::new(&preferred_editor)
            .arg(temp_file.path())
            .status();

        let status = match status {
            Ok(s) => s,
            Err(_) if preferred_editor != "nano" => {
                std::process::Command::new("nano")
                    .arg(temp_file.path())
                    .status()?
            }
            Err(e) => {
                enable_raw_mode()?;
                execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
                return Err(anyhow::anyhow!("Failed to launch editor: {}", e));
            }
        };

        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;

        if status.success() {
            let content = std::fs::read_to_string(temp_file.path())?;
            let content = content.trim();
            if !content.is_empty() {
                let id = self.db.create_note(content, None, "tui", &[])?;
                self.status_msg = format!("Created note #{}", id);
                self.update_notes()?;
            }
        }
        Ok(())
    }
}

pub fn run_tui(db: &Database, config: &Config) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = App::new(db, config)?;
    let res = run_app(&mut terminal, app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    if let Err(err) = res { println!("{:?}", err) }
    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if app.editing_search {
                    match key.code {
                        KeyCode::Enter | KeyCode::Esc => app.editing_search = false,
                        KeyCode::Char(c) => {
                            app.search_query.push(c);
                            app.update_notes()?;
                        }
                        KeyCode::Backspace => {
                            app.search_query.pop();
                            app.update_notes()?;
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
                        KeyCode::Char('y') => { app.copy_current()?; },
                        KeyCode::Char('d') => { app.delete_current()?; },
                        KeyCode::Char('e') => { 
                            app.edit_current()?;
                            terminal.clear()?;
                        },
                        KeyCode::Char('n') => { 
                            app.create_new()?;
                            terminal.clear()?;
                        },
                        _ => {}
                    }
                }
            }
        }
    }
}

fn parse_color(color: &str) -> Color {
    let color = color.trim();
    if color.starts_with('#') && color.len() == 7 {
        if let Ok(r) = u8::from_str_radix(&color[1..3], 16) {
            if let Ok(g) = u8::from_str_radix(&color[3..5], 16) {
                if let Ok(b) = u8::from_str_radix(&color[5..7], 16) {
                    return Color::Rgb(r, g, b);
                }
            }
        }
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

fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Search
            Constraint::Min(0),    // Main
            Constraint::Length(3), // Status & Help
        ])
        .split(f.size());

    let active_color = parse_color(&app.config.theme.active_border);
    let highlight_bg = parse_color(&app.config.theme.highlight_bg);
    let highlight_fg = parse_color(&app.config.theme.highlight_fg);

    // 1. Search Bar
    let search_style = if app.editing_search {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    let search = Paragraph::new(format!(" 🔍 SEARCH: {}", app.search_query))
        .style(search_style)
        .block(Block::default().borders(Borders::ALL).title("Jotun Search (Press '/' to type)"));
    f.render_widget(search, chunks[0]);

    // 2. Main Body (Three Panes)
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20), // Tags
            Constraint::Percentage(30), // Notes
            Constraint::Percentage(50), // Content
        ])
        .split(chunks[1]);

    // 2.1 Tags Sidebar
    let tags_items: Vec<ListItem> = app.tags.iter()
        .map(|t| ListItem::new(t.as_str()))
        .collect();
    
    let tags_block = Block::default()
        .borders(Borders::ALL)
        .title("Tags")
        .border_style(if app.active_pane == Pane::Tags { Style::default().fg(active_color) } else { Style::default() });

    let tags_list = List::new(tags_items)
        .block(tags_block)
        .highlight_style(Style::default().bg(highlight_bg).fg(highlight_fg).add_modifier(Modifier::BOLD))
        .highlight_symbol("> ");
    
    f.render_stateful_widget(tags_list, body_chunks[0], &mut app.tag_state);

    // 2.2 Notes List
    let notes_items: Vec<ListItem> = app.notes.iter()
        .map(|n| {
            let title = match &n.title {
                Some(t) => t.clone(),
                None => n.body.lines().next().unwrap_or("").chars().take(20).collect::<String>(),
            };
            ListItem::new(format!("[{}] {}", n.id, title))
        })
        .collect();

    let notes_block = Block::default()
        .borders(Borders::ALL)
        .title("Notes")
        .border_style(if app.active_pane == Pane::Notes { Style::default().fg(active_color) } else { Style::default() });

    let notes_list = List::new(notes_items)
        .block(notes_block)
        .highlight_style(Style::default().bg(highlight_bg).fg(highlight_fg).add_modifier(Modifier::BOLD))
        .highlight_symbol(">> ");

    f.render_stateful_widget(notes_list, body_chunks[1], &mut app.list_state);

    // 2.3 Content Viewer
    let (content_text, meta_header) = if let Some(i) = app.list_state.selected() {
        if i < app.notes.len() {
            let note = &app.notes[i];
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
            (note.body.as_str(), format!("{}\n{}", meta, "-".repeat(meta.len())))
        } else { ("", "".to_string()) }
    } else { ("", "".to_string()) };

    let mut preview_lines = vec![
        Line::from(Span::styled(meta_header, Style::default().add_modifier(Modifier::DIM))),
        Line::from(""),
    ];
    
    if app.config.syntax_highlighting.unwrap_or(true) && !content_text.is_empty() {
        let syntax = app.syntax_set.find_syntax_by_extension("md").unwrap_or_else(|| app.syntax_set.find_syntax_plain_text());
        let mut h = HighlightLines::new(syntax, &app.theme);
        
        for line in LinesWithEndings::from(content_text) {
            let ranges: Vec<(syntect::highlighting::Style, &str)> = h.highlight_line(line, &app.syntax_set).unwrap_or_default();
            let mut spans = vec![];
            for (style, text) in ranges {
                let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
                spans.push(Span::styled(text.to_string(), Style::default().fg(fg)));
            }
            preview_lines.push(Line::from(spans));
        }
    } else {
        for line in content_text.lines() {
            preview_lines.push(Line::from(line));
        }
    }

    let content_block = Block::default()
        .borders(Borders::ALL)
        .title("Preview")
        .border_style(if app.active_pane == Pane::Content { Style::default().fg(active_color) } else { Style::default() });

    let viewer = Paragraph::new(Text::from(preview_lines))
        .block(content_block)
        .wrap(Wrap { trim: false });
    f.render_widget(viewer, body_chunks[2]);

    // 3. Status & Help
    let help_text = if app.editing_search {
        " [Enter] Done  [Esc] Cancel search "
    } else {
        " [Tab] Pane  [j/k] Nav  [/] Search  [n] New  [y] Copy  [e] Edit  [d] Delete  [q] Quit "
    };

    let status_bar = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[2]);

    let msg = Paragraph::new(format!(" 📟 {}", app.status_msg))
        .block(Block::default().borders(Borders::ALL).title("Status"));
    f.render_widget(msg, status_bar[0]);

    let help = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL).title("Help"));
    f.render_widget(help, status_bar[1]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::config::Config;

    #[test]
    fn test_app_navigation() -> Result<()> {
        let db = Database::in_memory()?;
        let config = Config::default();
        db.create_note("Note 1", None, "test", &[])?;
        db.create_note("Note 2", None, "test", &[])?;
        
        let mut app = App::new(&db, &config)?;
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
        
        let mut app = App::new(&db, &config)?;
        app.search_query = String::from("Apple");
        app.update_notes()?;
        
        assert_eq!(app.notes.len(), 1);
        assert_eq!(app.notes[0].body, "Apple");
        
        Ok(())
    }
}




