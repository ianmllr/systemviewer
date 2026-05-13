use std::sync::mpsc;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode};
use ratatui::{
    style::{Color, Style, Stylize},
    widgets::{Block, Borders, BarGroup, Bar, BarChart},
};
use crate::collector::SystemSnapshot;

pub fn run(rx: mpsc::Receiver<SystemSnapshot>) {
    let mut latest: Option<SystemSnapshot> = None;

    ratatui::run(|terminal| {
        loop {
            if let Ok(snapshot) = rx.try_recv() {
                latest = Some(snapshot);
            }

            terminal.draw(|frame| {
                render(frame, &latest);
            }).unwrap();

            if event::poll(Duration::from_millis(100)).unwrap(){
                if let Event::Key(key) = event::read().unwrap() {
                    if key.code == KeyCode::Char('q') {
                        break();
                    }
                }
            }
        }
    })
}

fn render(frame: &mut ratatui::Frame, latest: &Option<SystemSnapshot>) {
    let (cpu_pct, mem_pct, mem_label) = match latest {
        Some(s) => {

            let cpu = (s.cpu_usage.iter().sum::<f32>() / s.cpu_usage.len() as f32) as u16;
            let pct = (s.memory_used * 100 / s.memory_total.max(1)) as u16;
            let used_gb  = s.memory_used  as f64 / 1024.0 / 1024.0 / 1024.0;
            let total_gb = s.memory_total as f64 / 1024.0 / 1024.0 / 1024.0;
            (cpu, pct, format!("{:.1} / {:.1} GB", used_gb, total_gb))

        }
        None => (0, 0, String::from("Indlæser...")),
    };

    let cpu_color = match cpu_pct {
        0..=50  => Color::Green,
        51..=80 => Color::Yellow,
        _       => Color::Red,
    };
    let mem_color = match mem_pct {
        0..=60  => Color::Green,
        61..=85 => Color::Yellow,
        _       => Color::Red,
    };



    let barchart = BarChart::default()

        .block(Block::default().title("System Monitor  |  'q' for at afslutte").borders(Borders::ALL))
        .bar_width(25)
        .bar_gap(6)
        .max(100)
        .data(
            BarGroup::default().bars(&[
                Bar::default()
                    .value(cpu_pct as u64)
                    .label(format!("CPU ({}%)", cpu_pct))
                    .text_value("")
                    .style(Style::default().fg(cpu_color)),
                Bar::default()
                    .value(mem_pct as u64)
                    .label(format!("RAM: {} ({}%)", mem_label.as_str(), mem_pct))
                    .text_value("")
                    .style(Style::default().fg(mem_color)),
            ])
        );

    frame.render_widget(barchart, frame.area());
}