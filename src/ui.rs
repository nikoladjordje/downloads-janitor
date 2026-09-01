use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    widgets::{Block, Borders, List, ListItem, Paragraph},
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
    frame.render_widget(
        List::new(items).block(Block::default().borders(Borders::LEFT | Borders::RIGHT)),
        areas[1],
    );

    frame.render_widget(
        Paragraph::new("q Quit")
            .alignment(Alignment::Right)
            .block(Block::default().borders(Borders::ALL)),
        areas[2],
    );
}
