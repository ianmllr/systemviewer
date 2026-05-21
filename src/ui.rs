use std::sync::mpsc;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},

};
use ratatui::layout::Alignment;
use crate::collector::SystemSnapshot;

pub fn run(rx: mpsc::Receiver<SystemSnapshot>) {
    let mut latest: Option<SystemSnapshot> = None;

    ratatui::run(|terminal| {
        loop {
            if let Ok(snapshot) = rx.try_recv() {
                latest = Some(snapshot);
            }

            terminal
                .draw(|frame| {
                    render(frame, &latest);
                })
                .unwrap();

            if event::poll(Duration::from_millis(100)).unwrap() {
                if let Event::Key(key) = event::read().unwrap() {
                    match key.code {
                        KeyCode::Char('q') => break(),
                        _ => {}
                    }
                }
            }
        }
    })
}

fn render(frame: &mut ratatui::Frame, latest: &Option<SystemSnapshot>) {
    let (cpu_pct, cpu_freq_ghz, mem_pct, mem_label, gpu_total_line, gpu_lines, proc_count, thread_count) = match latest {
        Some(s) => {
            let cpu = (s.cpu_usage.iter().sum::<f32>() / s.cpu_usage.len() as f32) as u16;
            let proc_count = s.process_count;
            let thread_count = s.thread_count;

            let pct = (s.memory_used * 100 / s.memory_total.max(1)) as u16;
            let used_gb = s.memory_used as f64 / 1024.0 / 1024.0 / 1024.0;
            let total_gb = s.memory_total as f64 / 1024.0 / 1024.0 / 1024.0;

            let gpu_used_gb = s.gpu_total_mem_used as f64 / 1024.0 / 1024.0 / 1024.0;
            let gpu_total_gb = s.gpu_total_mem as f64 / 1024.0 / 1024.0 / 1024.0;

            let gpu_total_line = format!(
                "GPU Total: util {}% | mem {:.1}/{:.1} GB | temp: {}C",
                s.gpu_total_util,
                gpu_used_gb,
                gpu_total_gb,
                s.gpu_max_temp
            );

            let mut lines = Vec::new();
            for g in &s.gpus {
                let used_gb = g.memory_used as f64 / 1024.0 / 1024.0 / 1024.0;
                let total_gb = g.memory_total as f64 / 1024.0 / 1024.0 / 1024.0;
                lines.push(format!(
                    "GPU {}: {} | util {}% | mem {:.1}/{:.1} GB | temp {}C",
                    g.index, g.name, g.utilization_percent, used_gb, total_gb, g.temperature
                ));
            }

            (
                cpu,
                s.cpu_freq_ghz,
                pct,
                format!("{:.1} / {:.1} GB", used_gb, total_gb),
                gpu_total_line,
                lines,
                proc_count,
                thread_count
            )
        }
        None => (
            0,
            0.0,
            0,
            String::from("Loading..."),
            String::from("GPU Total: -"),
            vec![String::from("GPU: -")],
            0,
            0,
        ),
    };

    // colors

    let cpu_color = match cpu_pct {
        0..=50 => Color::Green,
        51..=80 => Color::Yellow,
        _ => Color::Red,
    };
    let mem_color = match mem_pct {
        0..=60 => Color::Green,
        61..=85 => Color::Yellow,
        _ => Color::Red,
    };

    // title
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(frame.area());

    let title = Paragraph::new(vec![
        Line::raw("System Monitor").style(Style::default().fg(Color::LightCyan)),
        Line::raw("'q' to end program  |  'x' for dwdqw").style(Style::default().fg(Color::Cyan))
        ])
        .alignment(Alignment::Center);


    frame.render_widget(title, outer[0]);

    // 4 square grid
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(outer[1]);

    let cols_top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[0]);

    let cols_bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[1]);


    // text and values in grids
    let sys_text = vec![
        Line::from(vec![
            Span::raw("CPU: "),
            Span::styled(cpu_pct.to_string(), Style::default().fg(cpu_color)),
            Span::raw("% | "),
            Span::styled(format!("{:.2}", cpu_freq_ghz), Style::default().fg(cpu_color)),
            Span::raw(" GHz | "),
            Span::raw(format!("Processes: {}  |  Threads: {}", proc_count, thread_count)),
        ]),
    ];

    let mut gpu_text: Vec<Line> = Vec::new();
    gpu_text.push(Line::raw(gpu_total_line));
    gpu_text.extend(gpu_lines.into_iter().map(Line::raw));

    let ram_text = vec![
        Line::raw(format!("RAM: {} ({}%)", mem_label, mem_pct)).style(Style::default().fg(mem_color)),
    ];

    // different squares and their names/headlines
    let sys_block = Paragraph::new(sys_text)
        .block(Block::default().title("CPU").borders(Borders::ALL));

    let gpu_block = Paragraph::new(gpu_text)
        .block(Block::default().title("GPU").borders(Borders::ALL));

    let ram_block = Paragraph::new(ram_text)
        .block(Block::default().title("Memory").borders(Borders::ALL));


    let placeholder_block = Block::default().borders(Borders::ALL);

    // render the blocks
    frame.render_widget(sys_block, cols_top[0]);
    frame.render_widget(ram_block, cols_top[1]);
    frame.render_widget(gpu_block, cols_bottom[0]);

    frame.render_widget(placeholder_block, cols_bottom[1]);
}