use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use sysinfo::System;

pub struct SystemSnapshot {
    pub cpu_usage: Vec<f32>,
    pub memory_used: u64,
    pub memory_total: u64,
    pub timestamp: Instant,
}

pub fn spawn_collector(tx: mpsc::Sender<SystemSnapshot>) {
    thread::spawn(move || {
        let mut sys = System::new_all();

        loop {
            sys.refresh_cpu_usage();
            sys.refresh_memory();

            let snapshot = SystemSnapshot {
                cpu_usage: sys.cpus().iter().map(|c| c.cpu_usage()).collect(),
                memory_used: sys.used_memory(),
                memory_total: sys.total_memory(),
                timestamp: Instant::now(),
            };

            if tx.send(snapshot).is_err() {
                break;
            }

            thread::sleep(Duration::from_secs(1));
        }
    });
}