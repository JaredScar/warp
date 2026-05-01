use std::collections::HashMap;

use sysinfo::ProcessesToUpdate;
use warpui::{Entity, ModelContext, SingletonEntity};

/// How often to refresh process stats (seconds).
const REFRESH_INTERVAL_S: u64 = 3;
const REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(REFRESH_INTERVAL_S);

/// Per-process resource stats.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessStats {
    /// CPU usage percentage (0–100 × number of logical cores on some platforms; normalised to single-core here).
    pub cpu_pct: f32,
    /// Resident memory in bytes.
    pub memory_bytes: u64,
}

pub enum TabResourceMonitorEvent {
    /// Emitted after each successful refresh; consumers should re-read stats.
    Refreshed,
}

/// A singleton model that periodically samples CPU and memory usage for a
/// set of registered shell PIDs (one per terminal tab).
pub struct TabResourceMonitor {
    system: sysinfo::System,
    /// Map from shell PID → most-recent stats.
    stats: HashMap<u32, ProcessStats>,
    /// Number of logical CPUs (used to normalise per-core CPU usage).
    num_cpus: usize,
}

impl Entity for TabResourceMonitor {
    type Event = TabResourceMonitorEvent;
}

impl SingletonEntity for TabResourceMonitor {}

impl TabResourceMonitor {
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        let num_cpus = sysinfo::System::physical_core_count().unwrap_or(1).max(1);
        let me = Self {
            system: sysinfo::System::new(),
            stats: HashMap::new(),
            num_cpus,
        };
        Self::schedule_refresh(ctx);
        me
    }

    /// Returns the most-recently measured stats for the given PID, if available.
    pub fn stats_for_pid(&self, pid: u32) -> Option<ProcessStats> {
        self.stats.get(&pid).copied()
    }

    fn refresh(&mut self, ctx: &mut ModelContext<Self>) {
        if self.stats.is_empty() {
            return;
        }
        let pids: Vec<sysinfo::Pid> = self
            .stats
            .keys()
            .copied()
            .map(|p| sysinfo::Pid::from(p as usize))
            .collect();

        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&pids),
            false,
            sysinfo::ProcessRefreshKind::nothing().with_memory().with_cpu(),
        );

        for (&pid, stat) in self.stats.iter_mut() {
            let sysinfo_pid = sysinfo::Pid::from(pid as usize);
            if let Some(process) = self.system.process(sysinfo_pid) {
                stat.cpu_pct = process.cpu_usage() / self.num_cpus as f32;
                stat.memory_bytes = process.memory();
            }
        }

        ctx.emit(TabResourceMonitorEvent::Refreshed);
    }

    /// Register a shell PID so it is included in future refresh cycles.
    pub fn register_pid(&mut self, pid: u32) {
        self.stats.entry(pid).or_default();
    }

    /// Deregister a shell PID (e.g. when the terminal tab is closed).
    pub fn deregister_pid(&mut self, pid: u32) {
        self.stats.remove(&pid);
    }

    fn schedule_refresh(ctx: &mut ModelContext<Self>) {
        ctx.spawn(
            async {
                warpui::r#async::Timer::after(REFRESH_INTERVAL).await;
            },
            |me, _, ctx| {
                me.refresh(ctx);
                Self::schedule_refresh(ctx);
            },
        );
    }
}
