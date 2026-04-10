use crate::db::{Database, Note};
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
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;

pub struct App<'a> {
    db: &'a Database,
    notes: Vec<Note>,
    list_state: ListState,
    search_query: String,
    status_msg: String,
    editing_search: bool,
    should_quit: bool,
}

impl<'a> App<'a> {
    pub fn new(db: &'a Database) -> Result<Self> {
        let notes = db.list_notes()?;
        let mut list_state = ListState::default();
        if !notes.is_empty() {
            list_state.select(Some(0));
        }
        Ok(Self {
            db,
            notes,
            list_state,
            search_query: String::new(),
            status_msg: String::from("Welcome to Jotun!"),
            editing_search: false,
            should_quit: false,
        })
    }

    fn update_notes(&mut self) -> Result<()> {
        self.notes = if self.search_query.is_empty() {
            self.db.list_notes()?
        } else {
            self.db.search_notes(&self.search_query)?
        };
        
        if self.notes.is_empty() {
            self.list_state.select(None);
        } else {
            let selected = self.list_state.selected().unwrap_or(0);
            self.list_state.select(Some(selected.min(self.notes.len() - 1)));
        }
        Ok(())
    }

    pub fn next(&mut self) {
        if self.notes.is_empty() { return; }
        let i = match self.list_state.selected() {
            Some(i) => if i >= self.notes.len() - 1 { 0 } else { i + 1 },
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn previous(&mut self) {
        if self.notes.is_empty() { return; }
        let i = match self.list_state.selected() {
            Some(i) => if i == 0 { self.notes.len() - 1 } else { i - 1 },
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn delete_current(&mut self) -> Result<()> {
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
            let note = &self.notes[i];
            let editor = std::env::var("VISUAL")
                .or_else(|_| std::env::var("EDITOR"))
                .unwrap_or_else(|_| "nano".to_string());
            
            let temp_file = tempfile::NamedTempFile::new()?;
            std::fs::write(temp_file.path(), &note.body)?;

            disable_raw_mode()?;
            execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
            
            let status = std::process::Command::new(editor)
                .arg(temp_file.path())
                .status()?;

            enable_raw_mode()?;
            execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;

            if status.success() {
                let updated_body = std::fs::read_to_string(temp_file.path())?;
                let updated_body = updated_body.trim();
                if updated_body != note.body {
                    self.db.update_note(note.id, updated_body)?;
                    self.status_msg = format!("Updated note #{}", note.id);
                    self.update_notes()?;
                }
            }
        }
        Ok(())
    }

    pub fn create_new(&mut self) -> Result<()> {
        let editor = std::env::var("VISUAL")
            .or_else(|_| std::env::var("EDITOR"))
            .unwrap_or_else(|_| "nano".to_string());
        
        let temp_file = tempfile::NamedTempFile::new()?;

        disable_raw_mode()?;
        execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
        
        let status = std::process::Command::new(editor)
            .arg(temp_file.path())
            .status()?;

        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;

        if status.success() {
            let content = std::fs::read_to_string(temp_file.path())?;
            let content = content.trim();
            if !content.is_empty() {
                let id = self.db.create_note(content, "tui")?;
                self.status_msg = format!("Created note #{}", id);
                self.update_notes()?;
            }
        }
        Ok(())
    }
}

pub fn run_tui(db: &Database) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = App::new(db)?;
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
                        KeyCode::Down | KeyCode::Char('j') => app.next(),
                        KeyCode::Up | KeyCode::Char('k') => app.previous(),
                        KeyCode::Char('y') => { app.copy_current()?; },
                        KeyCode::Char('d') => { app.delete_current()?; },
                        KeyCode::Char('e') => { 
                            app.edit_current()?;
                            terminal.clear()?; // Fix UI glitch
                        },
                        KeyCode::Char('n') => { 
                            app.create_new()?;
                            terminal.clear()?; // Fix UI glitch
                        },
                        _ => {}
                    }
                }
            }
        }
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

    // 2. Main Body
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(chunks[1]);

    let items: Vec<ListItem> = app.notes.iter()
        .map(|n| {
            let preview = n.body.lines().next().unwrap_or("").chars().take(25).collect::<String>();
            ListItem::new(format!("[{}] {}", n.id, preview))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Notes"))
        .highlight_style(Style::default().bg(Color::Cyan).fg(Color::Black).add_modifier(Modifier::BOLD))
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, body_chunks[0], &mut app.list_state);

    let content = if let Some(i) = app.list_state.selected() {
        if i < app.notes.len() { &app.notes[i].body } else { "" }
    } else { "" };

    let viewer = Paragraph::new(content)
        .block(Block::default().borders(Borders::ALL).title("Content"))
        .wrap(Wrap { trim: true });
    f.render_widget(viewer, body_chunks[1]);

    // 3. Status & Help
    let help_text = if app.editing_search {
        " [Enter] Done  [Esc] Cancel search "
    } else {
        " [j/k] Nav  [/] Search  [n] New  [y] Copy  [e] Edit  [d] Delete  [q] Quit "
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
