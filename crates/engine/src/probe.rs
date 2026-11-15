//! Probe the machine and choose how many render workers to run before a render starts.

use std::process::Command;

/// What the machine has to spare right now.
pub struct Machine {
    /// Cores that can carry a render: performance cores on Apple silicon, logical cores elsewhere.
    pub cores: u32,
    /// Memory the OS can hand out without swapping, if it can be read.
    pub available_mb: Option<u64>,
}

pub struct Plan {
    pub workers: u32,
    pub reason: String,
}

/// One worker (browser, GPU process, renderer) peaked at ~540 MB on the bench scene.
const WORKER_MB: u64 = 600;
// shortcut: calibrated on one 8 GB M1, where one worker already saturates the 4 performance cores; re-measure on a bigger machine.
const CORES_PER_WORKER: u32 = 4;
/// Each worker starts its own Chromium (~1 s); fewer frames than this per worker don't repay it.
const MIN_FRAMES_PER_WORKER: u32 = 60;

pub fn probe() -> Machine {
    let logical = std::thread::available_parallelism().map_or(1, |n| n.get() as u32);
    if cfg!(target_os = "macos") {
        let cores = sysctl("hw.perflevel0.physicalcpu").or_else(|| sysctl("hw.physicalcpu"));
        let vm_stat = Command::new("vm_stat").output().ok().map(|o| String::from_utf8_lossy(&o.stdout).into_owned());
        Machine {
            cores: cores.map_or(logical, |c| c as u32),
            available_mb: vm_stat.as_deref().and_then(vm_stat_available_mb),
        }
    } else {
        let meminfo = std::fs::read_to_string("/proc/meminfo").ok();
        Machine { cores: logical, available_mb: meminfo.as_deref().and_then(meminfo_available_mb) }
    }
}

/// The smallest of what cores, memory and frame count allow, never below 1.
pub fn plan(machine: &Machine, frames: u32) -> Plan {
    let by_cores = machine.cores / CORES_PER_WORKER;
    let by_memory = machine.available_mb.map_or(1, |mb| (mb / WORKER_MB) as u32);
    let by_frames = frames / MIN_FRAMES_PER_WORKER;
    let workers = by_cores.min(by_memory).min(by_frames).max(1);
    let memory = match machine.available_mb {
        Some(mb) => format!("{mb} MB free fits {by_memory}"),
        None => "free memory unknown, assume 1".into(),
    };
    Plan {
        workers,
        reason: format!(
            "{} cores fit {by_cores} ({CORES_PER_WORKER} each), {memory} ({WORKER_MB} MB each), {frames} frames fit {by_frames} ({MIN_FRAMES_PER_WORKER} each)",
            machine.cores
        ),
    }
}

fn sysctl(name: &str) -> Option<u64> {
    let out = Command::new("sysctl").args(["-n", name]).output().ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// Free + inactive + speculative + purgeable pages: what macOS reclaims before it swaps.
fn vm_stat_available_mb(text: &str) -> Option<u64> {
    let page: u64 = text.split("page size of ").nth(1)?.split_whitespace().next()?.parse().ok()?;
    let pages = |label: &str| -> Option<u64> {
        let line = text.lines().find(|l| l.starts_with(label))?;
        line.rsplit(':').next()?.trim().trim_end_matches('.').parse().ok()
    };
    let total = ["Pages free:", "Pages inactive:", "Pages speculative:", "Pages purgeable:"]
        .iter()
        .map(|l| pages(l))
        .sum::<Option<u64>>()?;
    Some(total * page / 1_048_576)
}

fn meminfo_available_mb(text: &str) -> Option<u64> {
    let line = text.lines().find(|l| l.starts_with("MemAvailable:"))?;
    Some(line.split_whitespace().nth(1)?.parse::<u64>().ok()? / 1024)
}

#[test]
fn plans_workers() {
    let m1 = Machine { cores: 4, available_mb: Some(1400) };
    assert_eq!(plan(&m1, 300).workers, 1);
    let big = Machine { cores: 16, available_mb: Some(20_000) };
    assert_eq!(plan(&big, 300).workers, 4);
    assert_eq!(plan(&big, 90).workers, 1);
    let tight = Machine { cores: 16, available_mb: Some(1300) };
    assert_eq!(plan(&tight, 300).workers, 2);
    assert_eq!(plan(&Machine { cores: 16, available_mb: None }, 300).workers, 1);
}

#[test]
fn reads_free_memory() {
    let vm = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free:  4497.\nPages active: 85156.\nPages inactive: 81270.\nPages speculative: 2895.\nPages purgeable: 2.\n";
    assert_eq!(vm_stat_available_mb(vm), Some((4497 + 81270 + 2895 + 2) * 16384 / 1_048_576));
    assert_eq!(meminfo_available_mb("MemTotal: 100 kB\nMemAvailable:   2097152 kB\n"), Some(2048));
}
