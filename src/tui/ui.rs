use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use super::app::{App, AppMode, ItemStatus, ModalField};

pub fn draw(frame: &mut Frame, app: &App) {
    let root_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(3)])
        .split(frame.area());

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(root_chunks[0]);

    draw_queue_panel(frame, app, main_chunks[0]);
    draw_preview_panel(frame, app, main_chunks[1]);
    draw_footer(frame, app, root_chunks[1]);

    if app.mode == AppMode::InputModal {
        draw_input_modal(frame, app);
    }
}

fn draw_queue_panel(frame: &mut Frame, app: &App, area: Rect) {
    let completed = app
        .items
        .iter()
        .filter(|i| matches!(i.status, ItemStatus::Completed))
        .count();

    let title = format!(" Fila ({}/{}) ", completed, app.items.len());
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let items: Vec<ListItem> = app
        .items
        .iter()
        .map(|item| {
            let (badge, badge_color) = match &item.status {
                ItemStatus::Pending => ("•".to_string(), Color::DarkGray),
                ItemStatus::Processing => (app.spinner_char().to_string(), Color::Yellow),
                ItemStatus::Completed => ("✓".to_string(), Color::Green),
                ItemStatus::Failed(_) => ("✗".to_string(), Color::Red),
            };

            let line = Line::from(vec![
                Span::styled(
                    format!("[{}] ", badge),
                    Style::default()
                        .fg(badge_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(item.display_title()),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(40, 44, 52))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    let mut state = ListState::default();
    if !app.items.is_empty() {
        state.select(Some(app.selected_index));
    }

    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_preview_panel(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Preview do Anúncio (Shopee) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let Some(item) = app.selected_item() else {
        let empty =
            Paragraph::new("Nenhum item na fila. Pressione [a] para adicionar uma miniatura.")
                .style(Style::default().fg(Color::DarkGray))
                .wrap(Wrap { trim: true });
        frame.render_widget(empty, inner_area);
        return;
    };

    let mut lines: Vec<Line> = Vec::new();

    if let Some(ref url) = item.request.url {
        lines.push(Line::from(vec![
            Span::styled("URL: ", Style::default().fg(Color::DarkGray)),
            Span::styled(url, Style::default().fg(Color::Blue)),
        ]));
    }
    if let Some(ref info) = item.request.info {
        lines.push(Line::from(vec![
            Span::styled("Meta: ", Style::default().fg(Color::DarkGray)),
            Span::raw(info),
        ]));
    }
    if let Some(ref img) = item.request.image {
        lines.push(Line::from(vec![
            Span::styled("Imagem: ", Style::default().fg(Color::DarkGray)),
            Span::raw(img.display().to_string()),
        ]));
    }

    lines.push(Line::from(""));

    match &item.status {
        ItemStatus::Pending => {
            lines.push(Line::from(Span::styled(
                "Aguardando na fila...",
                Style::default().fg(Color::Yellow),
            )));
        }
        ItemStatus::Processing => {
            lines.push(Line::from(Span::styled(
                format!("{} Gerando anúncio via Gemini...", app.spinner_char()),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )));
        }
        ItemStatus::Failed(err) => {
            lines.push(Line::from(Span::styled(
                "Falha na geração:",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )));
            for err_line in err.lines() {
                lines.push(Line::from(Span::styled(
                    err_line.to_string(),
                    Style::default().fg(Color::Red),
                )));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Pressione [r] para tentar novamente.",
                Style::default().fg(Color::DarkGray),
            )));
        }
        ItemStatus::Completed => {
            if let Some(ref listing) = item.listing {
                let char_count = listing.titulo.chars().count();
                let count_color = if char_count <= 60 {
                    Color::Green
                } else {
                    Color::Red
                };

                lines.push(Line::from(vec![
                    Span::styled(
                        "TÍTULO ",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("({}/60 chars)", char_count),
                        Style::default().fg(count_color),
                    ),
                    if !listing.model_used.is_empty() {
                        Span::styled(
                            format!("  •  [{}]", listing.model_used),
                            Style::default().fg(Color::DarkGray),
                        )
                    } else {
                        Span::raw("")
                    },
                ]));

                lines.push(Line::from(Span::styled(
                    &listing.titulo,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )));

                lines.push(Line::from(Span::styled(
                    "─".repeat(inner_area.width as usize),
                    Style::default().fg(Color::DarkGray),
                )));

                lines.push(Line::from(Span::styled(
                    "DESCRIÇÃO:",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )));

                for desc_line in listing.descricao.lines() {
                    lines.push(Line::from(desc_line.to_string()));
                }
            }
        }
    }

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((app.preview_scroll, 0));

    frame.render_widget(paragraph, inner_area);
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let content = if let Some((ref msg, _)) = app.status_toast {
        Line::from(Span::styled(
            format!(" ✦ {} ", msg),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ))
    } else if app.mode == AppMode::InputModal {
        Line::from(
            " [Tab] Próximo Campo  [Shift+Tab] Anterior  [Enter] Adicionar à Fila  [Esc] Cancelar ",
        )
    } else {
        Line::from(
            " [j/k] Navegar  [Ctrl+d/u] Scroll  [a] Adicionar  [y] Título  [Y] Desc  [c] Ambos  [r] Retry  [d] Remover  [s] Salvar  [q] Sair ",
        )
    };

    let footer = Paragraph::new(content).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    frame.render_widget(footer, area);
}

fn draw_input_modal(frame: &mut Frame, app: &App) {
    let area = centered_rect(70, 55, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Adicionar Miniatura à Fila ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(2),
        ])
        .split(inner);

    render_input_field(
        frame,
        " URL (MakerWorld / Printables / etc.) ",
        &app.modal.url,
        app.modal.active_field == ModalField::Url,
        chunks[0],
    );

    render_input_field(
        frame,
        " Metadados (Personagem, Escala, Acabamento) ",
        &app.modal.info,
        app.modal.active_field == ModalField::Info,
        chunks[1],
    );

    render_input_field(
        frame,
        " Caminho da Imagem (ex: /tmp/foto.png) ",
        &app.modal.image,
        app.modal.active_field == ModalField::Image,
        chunks[2],
    );

    let info_line = if let Some(ref err) = app.modal.error_msg {
        Paragraph::new(Span::styled(
            format!("Erro: {}", err),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ))
    } else {
        Paragraph::new(Span::styled(
            "Preencha ao menos um campo e pressione Enter.",
            Style::default().fg(Color::DarkGray),
        ))
    };

    frame.render_widget(info_line, chunks[3]);
}

fn render_input_field(frame: &mut Frame, label: &str, value: &str, is_active: bool, area: Rect) {
    let border_color = if is_active {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    let display_text = if is_active {
        format!("{}▌", value)
    } else {
        value.to_string()
    };

    let widget = Paragraph::new(display_text).block(
        Block::default()
            .title(label)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color)),
    );

    frame.render_widget(widget, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
