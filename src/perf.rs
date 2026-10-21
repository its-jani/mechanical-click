//! Process telemetry for the header HUD: CPU (percent of one core) and working
//! set. Sampled every ~700 ms from the Win32 process APIs, never in a hot path.

use std::time::{Duration, Instant};

use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

pub struct Perf {
    cpu: f32,
    ram_mb: f32,
    last: Instant,
    last_cpu: u64,
}

impl Default for Perf {
    fn default() -> Self {
        Self::new()
    }
}

impl Perf {
    pub fn new() -> Self {
        let mut perf = Self {
            cpu: 0.0,
            ram_mb: 0.0,
            last: Instant::now(),
            last_cpu: cpu_time(),
        };
        perf.ram_mb = working_set_mb();
        perf
    }

    /// Cheap enough to call every frame; only reads the OS twice a second.
    pub fn tick(&mut self) {
        if self.last.elapsed() < Duration::from_millis(700) {
            return;
        }
        let now = Instant::now();
        let wall = now.duration_since(self.last).as_secs_f64();
        let cpu = cpu_time();
        if wall > 0.05 {
            let delta = cpu.saturating_sub(self.last_cpu) as f64;
            self.cpu = (delta / (wall * 1e7) * 100.0) as f32;
        }
        self.last = now;
        self.last_cpu = cpu;
        self.ram_mb = working_set_mb();
    }

    pub fn cpu(&self) -> f32 {
        self.cpu
    }

    pub fn ram_mb(&self) -> f32 {
        self.ram_mb
    }
}

fn filetime_ticks(ft: FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64
}

fn cpu_time() -> u64 {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let ok = unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    };
    if ok.is_err() {
        return 0;
    }
    filetime_ticks(kernel) + filetime_ticks(user)
}

fn working_set_mb() -> f32 {
    let mut counters = PROCESS_MEMORY_COUNTERS::default();
    let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    let ok = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, size) };
    if ok.is_err() {
        return 0.0;
    }
    counters.WorkingSetSize as f32 / (1024.0 * 1024.0)
}
