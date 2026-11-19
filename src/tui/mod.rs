pub mod app;
pub mod ui;

use anyhow::{Context, Result};
use chrono::Local;
use crossterm::{
    event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures::StreamExt;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::fs;
use std::io::{self, stdout};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;

use crate::batch::{BatchEntry, EntryStatus, persist_outputs};
use crate::clipboard::ClipboardManager;
use crate::gemini::ListingRequest;
use app::{App, AppMode, ItemStatus, WorkerEvent, run_worker};

pub async fn run_tui(
    api_key: String,
    initial_file: Option<PathBuf>,
    clipboard: ClipboardManager,
) -> Result<()> {
    fs::create_dir_all("output").context("Falha ao criar diretório ./output")?;
    let stamp = Local::now().format("%Y%m%d_%H%M%S");
    let json_path = PathBuf::from(format!("output/tui_{stamp}.json"));
    let md_path = PathBuf::from(format!("output/tui_{stamp}.md"));

    // Garante restauração do terminal mesmo em caso de panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(panic_info);
    }));

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (worker_tx, worker_rx) = mpsc::channel(64);
    let (event_tx, mut event_rx) = mpsc::channel(64);

    tokio::spawn(run_worker(api_key, worker_rx, event_tx));

    let mut app = App::new(worker_tx, clipboard);

    // Carrega arquivo inicial no mesmo formato do batch (`url` ou `url | metadados`)
    if let Some(path) = initial_file
        && let Ok(content) = fs::read_to_string(&path)
    {
        let mut count = 0;
        for line in content
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
        {
            let (url_part, info_part) = match line.split_once('|') {
                Some((u, m)) => (u.trim(), Some(m.trim().to_string())),
                None => (line, None),
            };
            let req = ListingRequest {
                url: (!url_part.is_empty()).then(|| url_part.to_string()),
                info: info_part.filter(|s| !s.is_empty()),
                image: None,
            };
            if app.enqueue_request(req).await.is_ok() {
                count += 1;
            }
        }
        app.set_toast(format!("{} itens carregados de {}", count, path.display()));
    }

    let res = run_event_loop(&mut terminal, &mut app, &mut event_rx, &json_path, &md_path).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    res
}

async fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    event_rx: &mut mpsc::Receiver<WorkerEvent>,
    json_path: &Path,
    md_path: &Path,
) -> Result<()> {
    let mut reader = EventStream::new();
    let mut tick_interval = tokio::time::interval(Duration::from_millis(100));

    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        if app.should_quit {
            break;
        }

        tokio::select! {
            _ = tick_interval.tick() => {
                app.on_tick();
            }
            Some(worker_event) = event_rx.recv() => {
                let is_finished = matches!(
                    worker_event,
                    WorkerEvent::Success { .. } | WorkerEvent::Error { .. }
                );
                app.handle_worker_event(worker_event);
                if is_finished {
                    let _ = save_tui_snapshot(app, json_path, md_path);
                }
            }
            Some(Ok(Event::Key(key))) = reader.next() => {
                handle_key_event(app, key, json_path, md_path).await;
            }
        }
    }

    Ok(())
}

async fn handle_key_event(app: &mut App, key: KeyEvent, json_path: &Path, md_path: &Path) {
    match app.mode {
        AppMode::Normal => match key.code {
            KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.should_quit = true;
            }
            KeyCode::Char('j') | KeyCode::Down => app.select_next(),
            KeyCode::Char('k') | KeyCode::Up => app.select_prev(),
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.scroll_preview_down(5);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.scroll_preview_up(5);
            }
            KeyCode::Char('a') | KeyCode::Char('i') => {
                app.modal.clear();
                app.mode = AppMode::InputModal;
            }
            KeyCode::Char('y') => app.copy_title(),
            KeyCode::Char('Y') => app.copy_description(),
            KeyCode::Char('c') => app.copy_both(),
            KeyCode::Char('r') => app.retry_selected().await,
            KeyCode::Char('d') => app.delete_selected(),
            KeyCode::Char('s') => match save_tui_snapshot(app, json_path, md_path) {
                Ok(_) => app.set_toast(format!("Salvo em {} e .md", json_path.display())),
                Err(e) => app.set_toast(format!("Erro ao salvar: {e}")),
            },
            _ => {}
        },
        AppMode::InputModal => match key.code {
            KeyCode::Esc => {
                app.mode = AppMode::Normal;
            }
            KeyCode::Tab => {
                app.modal.active_field = app.modal.active_field.next();
            }
            KeyCode::BackTab => {
                app.modal.active_field = app.modal.active_field.prev();
            }
            KeyCode::Enter => {
                app.submit_modal().await;
            }
            KeyCode::Backspace => {
                app.modal.active_buffer_mut().pop();
            }
            KeyCode::Char(c) => {
                app.modal.active_buffer_mut().push(c);
            }
            _ => {}
        },
    }
}

/// Converte a fila da TUI em `BatchEntry` e reaproveita `batch::persist_outputs`
fn save_tui_snapshot(app: &App, json_path: &Path, md_path: &Path) -> Result<()> {
    if app.items.is_empty() {
        anyhow::bail!("A fila está vazia");
    }

    let entries: Vec<BatchEntry> = app
        .items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let status = match &item.status {
                ItemStatus::Pending => EntryStatus::Pending,
                ItemStatus::Processing => EntryStatus::Processing,
                ItemStatus::Completed => match &item.listing {
                    Some(listing) => EntryStatus::Completed {
                        listing: listing.clone(),
                    },
                    None => EntryStatus::Pending,
                },
                ItemStatus::Failed(error) => EntryStatus::Failed {
                    error: error.clone(),
                },
            };

            BatchEntry {
                index: idx,
                url: item.request.url.clone(),
                info: item.request.info.clone(),
                image: item.request.image.clone(),
                status,
            }
        })
        .collect();

    persist_outputs(&entries, json_path, md_path)
}
