use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use crate::app::App;

pub fn render(frame: &mut Frame<'_>, app: &App) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Fill(1),
            Constraint::Length(3),
        ])
        .split(frame.area());

    frame.render_widget(
        Paragraph::new(format!("~/Downloads    {} entries", app.entries().len())).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Downloads Janitor"),
        ),
        areas[0],
    );

    let items = app
        .entries()
        .iter()
        .map(|entry| ListItem::new(entry.display_name()))
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(Block::default().borders(Borders::LEFT | Borders::RIGHT))
        .highlight_symbol("> ")
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    let mut list_state = ListState::default();
    list_state.select(app.selected());
    frame.render_stateful_widget(list, areas[1], &mut list_state);

    frame.render_widget(
        Paragraph::new("j/k or ↑/↓ Navigate    q Quit")
            .alignment(Alignment::Right)
            .block(Block::default().borders(Borders::ALL)),
        areas[2],
    );
}
