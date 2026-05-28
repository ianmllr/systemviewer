    use std::collections::VecDeque;
    use std::sync::mpsc;
    use std::time::Duration;

    use crossterm::event::{self, Event, KeyCode};
    use ratatui::{
        layout::{Constraint, Layout, Rect, Alignment},
        style::{Color, Style, Stylize},
        text::{Line, Span},
        widgets::{Bar, BarChart, Block, Borders, Paragraph},
        Frame,
    };

    use crate::collector::SystemSnapshot;

    // object of data ready to be shown
    struct ViewModel {
        cpu_pct: u16,
        cpu_freq_ghz: f64,
        mem_pct: u16,
        mem_label: String,
        gpu_total_line: String,
        gpu_lines: Vec<String>,
        proc_count: usize,
        thread_count: usize,
    }

    pub fn run(rx: mpsc::Receiver<SystemSnapshot>) {
        let mut cpu_history: VecDeque<u16> = VecDeque::new(); // vecdeque is smart for adding and removing from both ends
        let mut mem_history: VecDeque<u16> = VecDeque::new();

        let mut latest = rx.try_recv().unwrap_or_else(|_| SystemSnapshot::default_empty());

        push_cpu_sample(&mut cpu_history, &latest);
        push_mem_sample(&mut mem_history, &latest);

        ratatui::run(|terminal| {
            loop {
                // non blocking attempt to receive data
                if let Ok(snapshot) = rx.try_recv() {
                    push_cpu_sample(&mut cpu_history, &snapshot);
                    push_mem_sample(&mut mem_history, &snapshot);
                    latest = snapshot;
                }

                // draws screen
                terminal.draw(|frame| render(frame, &latest, &mut cpu_history, &mut mem_history)).unwrap();

                // listens for key presses for menu options/navigation
                if event::poll(Duration::from_millis(100)).unwrap() {
                    if let Event::Key(key) = event::read().unwrap() {
                        if key.code == KeyCode::Char('q') {
                            break;
                        }
                    }
                }
            }
        });
    }

    fn render(frame: &mut Frame, snapshot: &SystemSnapshot, cpu_history: &mut VecDeque<u16>, mem_history: &mut VecDeque<u16>) {
        let vm = build_view_model(snapshot);
        let area = frame.area();

        // dynamic colors based on "severity" or percent
        let cpu_color = match vm.cpu_pct {
            0..=50 => Color::Green,
            51..=80 => Color::Yellow,
            _ => Color::Red,
        };
        let mem_color = match vm.mem_pct {
            0..=60 => Color::Green,
            61..=85 => Color::Yellow,
            _ => Color::Red,
        };

        // screen layout with 4 squares
        let layout = build_layout(area);

        // makes percentage graph/bars only fill up to screen size
        let max_cpu_bars = layout.top_left.width.saturating_sub(2) as usize;
        while cpu_history.len() > max_cpu_bars {
            cpu_history.pop_front(); // removes first block if it's outside the max size
        }

        let max_mem_bars = layout.top_right.width.saturating_sub(2) as usize;
        while mem_history.len() > max_mem_bars {
            mem_history.pop_front();
        }

        // renders 4 widgets
        frame.render_widget(build_title(), layout.title);
        render_cpu_block(frame, layout.top_left, &vm, cpu_color, cpu_history);
        render_ram_block(frame, layout.top_right, &vm, mem_color, mem_history);
        frame.render_widget(build_gpu_block(&vm), layout.bottom_left);
        frame.render_widget(Block::default().borders(Borders::ALL), layout.bottom_right);
    }

    // defines parts of screen including title bar at top
    struct LayoutParts {
        title: Rect,
        top_left: Rect,
        top_right: Rect,
        bottom_left: Rect,
        bottom_right: Rect,
    }

    fn build_layout(area: Rect) -> LayoutParts {
        // splits title at top from squares
        let main_split = Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).split(area);

        // splits vertically
        let rows = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(main_split[1]);

        // splits horizontally
        let cols_top = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[0]);
        let cols_bottom = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[1]);

        LayoutParts {
            title: main_split[0],
            top_left: cols_top[0],
            top_right: cols_top[1],
            bottom_left: cols_bottom[0],
            bottom_right: cols_bottom[1],
        }
    }

    fn push_cpu_sample(cpu_history: &mut VecDeque<u16>, snapshot: &SystemSnapshot) {
        cpu_history.push_back(get_avg_cpu(snapshot));
    }

    fn push_mem_sample(mem_history: &mut VecDeque<u16>, snapshot: &SystemSnapshot) {
        mem_history.push_back(get_mem_pct(snapshot));
    }

    fn get_avg_cpu(snapshot: &SystemSnapshot) -> u16 {
        if snapshot.cpu_usage.is_empty() {
            0
        } else {
            (snapshot.cpu_usage.iter().sum::<f32>() / snapshot.cpu_usage.len() as f32) as u16
        }
    }

    fn get_mem_pct(snapshot: &SystemSnapshot) -> u16 {
        (snapshot.memory_used * 100 / snapshot.memory_total.max(1)) as u16
    }

    fn build_view_model(s: &SystemSnapshot) -> ViewModel {
        let cpu_pct = get_avg_cpu(s);

        let mem_pct = get_mem_pct(s);
        let used_gb = s.memory_used as f64 / 1024.0 / 1024.0 / 1024.0;
        let total_gb = s.memory_total as f64 / 1024.0 / 1024.0 / 1024.0;

        let gpu_used_gb = s.gpu_total_mem_used as f64 / 1024.0 / 1024.0 / 1024.0;
        let gpu_total_gb = s.gpu_total_mem as f64 / 1024.0 / 1024.0 / 1024.0;

        let gpu_total_line = format!(
            "GPU Total: util {}% | mem {:.1}/{:.1} GB | temp: {}C",
            s.gpu_total_util, gpu_used_gb, gpu_total_gb, s.gpu_max_temp
        );

        let mut gpu_lines = Vec::new();
        for g in &s.gpus {
            let g_used_gb = g.memory_used as f64 / 1024.0 / 1024.0 / 1024.0;
            let g_total_gb = g.memory_total as f64 / 1024.0 / 1024.0 / 1024.0;
            gpu_lines.push(format!(
                "GPU {}: {} | util {}% | mem {:.1}/{:.1} GB | temp {}C",
                g.index, g.name, g.utilization_percent, g_used_gb, g_total_gb, g.temperature
            ));
        }

        ViewModel {
            cpu_pct,
            cpu_freq_ghz: s.cpu_freq_ghz,
            mem_pct,
            mem_label: format!("{:.1} / {:.1} GB", used_gb, total_gb),
            gpu_total_line,
            gpu_lines,
            proc_count: s.process_count,
            thread_count: s.thread_count,
        }
    }

    fn build_title() -> Paragraph<'static> {
        Paragraph::new(vec![
            Line::from("System Monitor").light_cyan(),
            Line::from("'q' to end program").cyan(),
        ])
            .alignment(Alignment::Center)
    }

    fn render_cpu_block(frame: &mut Frame, area: Rect, vm: &ViewModel, cpu_color: Color, cpu_history: &VecDeque<u16>) {
        // border
        let block = Block::default().title("CPU").borders(Borders::ALL);
        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        // splits inner area between text and graph. text area is 1 line (constraint: 1)
        let layout = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(inner_area);

        let header_text = Line::from(vec![
            Span::raw("CPU: "),
            Span::styled(format!("{}%", vm.cpu_pct), Style::default().fg(cpu_color)),
            Span::raw(format!(" | {:.2} GHz | Processes: {} | Threads: {}",
                              vm.cpu_freq_ghz, vm.proc_count, vm.thread_count)),
        ]);

        let graph_layout = Layout::horizontal([Constraint::Length(6), Constraint::Min(0)]).split(layout[1]);
        let y_axis_area = graph_layout[0];
        let barchart_area = graph_layout[1];

        // converts CPU history into list of ("", u64) tuples for barchart.
        let bars: Vec<Bar> = cpu_history
            .iter()
            .map(|&v| Bar::default().value(v as u64).text_value(""))
            .collect();

        // graph settings
        let graph = BarChart::new(bars)
            .bar_width(1)
            .bar_gap(0)
            .max(100)
            .style(Style::default().fg(cpu_color));

        frame.render_widget(Paragraph::new(header_text), layout[0]);
        frame.render_widget(build_y_axis(y_axis_area.height), y_axis_area);
        frame.render_widget(graph, barchart_area);
    }

    fn render_ram_block(frame: &mut Frame, area: Rect, vm: &ViewModel, mem_color: Color, mem_history: &VecDeque<u16>) {
        let block = Block::default().title("Memory").borders(Borders::ALL);
        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        let layout = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(inner_area);

        let header_text = Line::from(vec![
            Span::raw("RAM: "),
            Span::styled(format!("{}%", vm.mem_pct), Style::default().fg(mem_color)),
            Span::raw(format!(" | {}", vm.mem_label)),
        ]);

        let graph_layout = Layout::horizontal([Constraint::Length(6), Constraint::Min(0)]).split(layout[1]);
        let y_axis_area = graph_layout[0];
        let barchart_area = graph_layout[1];

        let bars: Vec<Bar> = mem_history
            .iter()
            .map(|&v| Bar::default().value(v as u64).text_value(""))
            .collect();

        let graph = BarChart::new(bars)
            .bar_width(1)
            .bar_gap(0)
            .max(100)
            .style(Style::default().fg(mem_color));

        frame.render_widget(Paragraph::new(header_text), layout[0]);
        frame.render_widget(build_y_axis(y_axis_area.height), y_axis_area);
        frame.render_widget(graph, barchart_area);
    }

    fn build_gpu_block(vm: &ViewModel) -> Paragraph<'static> {
        let mut gpu_text = vec![Line::from(vm.gpu_total_line.clone())];
        gpu_text.extend(vm.gpu_lines.iter().cloned().map(Line::from));

        Paragraph::new(gpu_text)
            .block(Block::default().title("GPU").borders(Borders::ALL))
    }
    fn build_y_axis(height: u16) -> Paragraph<'static> {
        let mut lines = Vec::new();
        let mut last_20 = 120; // starts above 100 to make sure 100% always prints

        // makes sure graph starts at 0
        for i in 0..height {
            let pct = if height > 1 {
                100.0 - (i as f32 / (height - 1) as f32 * 100.0)
            } else {
                100.0
            };

            let nearest_20 = (pct / 20.0).round() as u16 * 20;

            if nearest_20 != last_20 {
                // prints number and underscore
                let text = format!("{:3}% _", nearest_20);
                lines.push(Line::from(text).dark_gray().alignment(Alignment::Right));
                last_20 = nearest_20;
            } else {
                // empty space if no 20% mark here
                lines.push(Line::from("  ").dark_gray().alignment(Alignment::Right));
            }
        }

        Paragraph::new(lines)
    }
