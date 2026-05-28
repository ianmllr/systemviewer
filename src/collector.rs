use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use gpuinfo::device::nvidia::NvidiaManager;
use gpuinfo::device::GpuManager;
use sysinfo::{System, ProcessesToUpdate};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Thread32First, Thread32Next,
    THREADENTRY32, TH32CS_SNAPTHREAD,
};

#[derive(Clone)]
pub struct GpuSnapshot {
    pub index: u32,
    pub name: String,
    pub utilization_percent: u64,
    pub memory_used: u64,
    pub memory_total: u64,
    pub temperature: u64,
}

pub struct SystemSnapshot {
    pub cpu_usage: Vec<f32>,
    pub cpu_freq_ghz: f64,
    pub process_count: usize,
    pub thread_count: usize,
    pub memory_used: u64,
    pub memory_total: u64,
    pub gpu_total_util: u64,
    pub gpu_total_mem_used: u64,
    pub gpu_total_mem: u64,
    pub gpu_max_temp: u64,
    pub gpus: Vec<GpuSnapshot>,
}

// default for if no collector running
impl SystemSnapshot {
    pub fn default_empty() -> Self {
        SystemSnapshot {
            cpu_usage: vec![0.0; 4],
            cpu_freq_ghz: 0.0,
            process_count: 0,
            thread_count: 0,
            memory_used: 0,
            memory_total: 1_000_000_000,
            gpu_total_util: 0,
            gpu_total_mem_used: 0,
            gpu_total_mem: 0,
            gpu_max_temp: 0,
            gpus: vec![],
        }
    }
}

pub fn spawn_collector(tx: mpsc::Sender<SystemSnapshot>) {
    thread::spawn(move || {
        let mut sys = System::new_all();
        let mut gpu_manager = NvidiaManager::init().ok();
        let mut cached_gpu = CachedGpuMetrics::default();

        // GPU check interval to keep program relatively light
        let gpu_sample_interval = Duration::from_secs(3);
        let mut last_gpu_sample = Instant::now() - gpu_sample_interval;

        loop {
            refresh_system(&mut sys);
            ensure_gpu_manager(&mut gpu_manager);
            sample_gpu_if_due(
                &gpu_manager,
                &mut cached_gpu,
                &mut last_gpu_sample,
                gpu_sample_interval,
            );

            let process_count = sys.processes().len();
            let thread_count = count_threads_windows();

            let snapshot = build_snapshot(&sys, &cached_gpu, process_count, thread_count);

            if tx.send(snapshot).is_err() {
                break;
            }

            thread::sleep(Duration::from_secs(2));
        }
    });
}

#[derive(Default)] // don't need to make initial 0 constructor
struct CachedGpuMetrics {
    total_util: u64,
    total_mem_used: u64,
    total_mem: u64,
    max_temp: u64,
    gpus: Vec<GpuSnapshot>,
}

fn refresh_system(sys: &mut System) {
    sys.refresh_cpu_usage();
    sys.refresh_cpu_frequency();
    sys.refresh_memory();
    sys.refresh_processes(ProcessesToUpdate::All, true);
}

fn ensure_gpu_manager(gpu_manager: &mut Option<NvidiaManager>) {
    if gpu_manager.is_none() {
        *gpu_manager = NvidiaManager::init().ok();
    }
}

fn sample_gpu_if_due(
    gpu_manager: &Option<NvidiaManager>,
    cached: &mut CachedGpuMetrics,
    last_sample: &mut Instant,
    interval: Duration,
) {
    if last_sample.elapsed() < interval {
        return;
    }

    if let Some(manager) = gpu_manager.as_ref() {
        if let Ok(metrics) = manager.collect_all_metrics() {
            *cached = collect_gpu_metrics(metrics);
        }
    }

    *last_sample = Instant::now();
}

fn collect_gpu_metrics(metrics: Vec<gpuinfo::metrics::GpuMetrics>) -> CachedGpuMetrics {
    let mut total_mem_used = 0u64;
    let mut total_mem = 0u64;
    let mut total_util = 0u64;
    let mut max_temp = 0u64;
    let mut per_gpu = Vec::with_capacity(metrics.len());

    for m in metrics {
        let index = m.index.unwrap_or(0);
        let name = m.name.unwrap_or_else(|| "Unknown".to_string());
        let util = m.gpu_utilization.unwrap_or(99) as u64;
        let mem_used = m.memory_used.unwrap_or(0);
        let mem_total = m.memory_total.unwrap_or(0);
        let temp = m.temperature.unwrap_or(0) as u64;

        total_util += util;
        total_mem_used += mem_used;
        total_mem += mem_total;
        if temp > max_temp {
            max_temp = temp;
        }

        per_gpu.push(GpuSnapshot {
            index,
            name,
            utilization_percent: util,
            memory_used: mem_used,
            memory_total: mem_total,
            temperature: temp,
        });
    }

    CachedGpuMetrics {
        total_util,
        total_mem_used,
        total_mem,
        max_temp,
        gpus: per_gpu,
    }
}

fn build_snapshot(sys: &System, cached_gpu: &CachedGpuMetrics, process_count: usize, thread_count: usize) -> SystemSnapshot {
    let cpu_freq_ghz = average_cpu_freq_ghz(sys);

    SystemSnapshot {
        cpu_usage: sys.cpus().iter().map(|c| c.cpu_usage()).collect(),
        cpu_freq_ghz,
        process_count,
        thread_count,
        memory_used: sys.used_memory(),
        memory_total: sys.total_memory(),
        gpu_total_util: cached_gpu.total_util,
        gpu_total_mem_used: cached_gpu.total_mem_used,
        gpu_total_mem: cached_gpu.total_mem,
        gpu_max_temp: cached_gpu.max_temp,
        gpus: cached_gpu.gpus.clone(),
    }
}

fn average_cpu_freq_ghz(sys: &System) -> f64 {
    if sys.cpus().is_empty() {
        0.0
    } else {
        let total_mhz: u64 = sys.cpus().iter().map(|c| c.frequency()).sum();
        (total_mhz as f64 / sys.cpus().len() as f64) / 1000.0
    }
}


fn count_threads_windows() -> usize {
    unsafe { // Windows api has no safety guarantees so rust requires unsafe block
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) else {
            return 0;
        };

        let mut entry = THREADENTRY32::default(); // empty structure to hold thread info
        entry.dwSize = size_of::<THREADENTRY32>() as u32;

        let mut count = 0usize;
        if Thread32First(snapshot, &mut entry).is_ok() {
            count += 1;
            while Thread32Next(snapshot, &mut entry).is_ok() {
                count += 1;
            }
        }
        count
    }
}