mod collector;
mod ui;

use std::sync::mpsc;

fn main() {
    let (tx, rx) = mpsc::channel();
    collector::spawn_collector(tx);
    ui::run(rx);
}