//! CPU, memory, storage and temperature as one shared reading.
//!
//! These four are polled rather than event-driven — the kernel has no "usage changed" signal — so they are
//! deliberately a *single* service with one timer and one publish per tick, not four. A bar with a CPU chip and
//! a dashboard with five cards then cost one wakeup a second between them, which is the difference between a
//! shell that idles and one that keeps a core warm.
//!
//! Everything here reads `/proc` and sysfs directly: no `lm-sensors`, no `nvidia-smi`, nothing to install. A
//! machine that doesn't expose a reading (no hwmon, no swap) reports `None` for it and the UI omits it.

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use platform_layershell::EventSender;

use crate::shared::services::broadcast::{Broadcast, Service};

/// How often the reading is refreshed. A second is the granularity a person can actually read off a bar chip;
/// anything faster costs wakeups for numbers nobody can follow.
const POLL: Duration = Duration::from_secs(1);

/// How many readings a sparkline keeps — a minute of history at [`POLL`].
pub const HISTORY: usize = 60;

/// A fixed-length window of recent readings, oldest first. Kept by the service rather than by each card, so
/// several cards charting the same series show the same history instead of each starting blank on open.
#[derive(Clone, Debug, Default)]
pub struct History(VecDeque<f32>);

impl History {
    /// Records a reading, dropping the oldest once the window is full.
    pub fn push(&mut self, value: f32) {
        if self.0.len() == HISTORY {
            self.0.pop_front();
        }
        self.0.push_back(value);
    }

    /// The readings, oldest first.
    pub fn values(&self) -> Vec<f32> {
        self.0.iter().copied().collect()
    }

    pub fn latest(&self) -> f32 {
        self.0.back().copied().unwrap_or(0.0)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The largest reading in the window, for scaling a chart whose series has no natural ceiling (byte rates).
    pub fn peak(&self) -> f32 {
        self.0.iter().copied().fold(0.0, f32::max)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Memory {
    /// Bytes.
    pub total: u64,
    pub used: u64,
    pub swap_total: u64,
    pub swap_used: u64,
}

impl Memory {
    pub fn used_percent(&self) -> f32 {
        percent(self.used, self.total)
    }

    pub fn swap_percent(&self) -> f32 {
        percent(self.swap_used, self.swap_total)
    }
}

#[derive(Clone, Debug)]
pub struct Disk {
    pub mount: PathBuf,
    /// Bytes.
    pub total: u64,
    pub used: u64,
}

impl Disk {
    pub fn used_percent(&self) -> f32 {
        percent(self.used, self.total)
    }
}

/// One tick's view of the machine.
#[derive(Clone, Debug, Default)]
pub struct Resources {
    /// Aggregate CPU busy time, 0–100.
    pub cpu: f32,
    pub cpu_history: History,
    /// Per-core busy time, 0–100, in `/proc/stat` order.
    pub cores: Vec<f32>,
    pub memory: Memory,
    pub memory_history: History,
    /// The hottest sensor the machine exposes, in °C; `None` when there is no hwmon to read.
    pub temperature: Option<f32>,
    /// The root filesystem, plus `/home` when it is a separate mount.
    pub disks: Vec<Disk>,
}

fn percent(part: u64, whole: u64) -> f32 {
    if whole == 0 {
        return 0.0;
    }
    (part as f64 / whole as f64 * 100.0) as f32
}

/// Cumulative jiffies from one `/proc/stat` cpu line: everything, and the idle share of it.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct CpuTimes {
    total: u64,
    idle: u64,
}

/// Parses a `/proc/stat` `cpu…` line. Fields are `user nice system idle iowait irq softirq steal …`; idle time
/// is `idle + iowait`, since a core waiting on disk is not doing work.
fn parse_cpu_line(line: &str) -> Option<CpuTimes> {
    let mut fields = line.split_whitespace();
    let label = fields.next()?;
    if !label.starts_with("cpu") {
        return None;
    }
    let values: Vec<u64> = fields.filter_map(|f| f.parse().ok()).collect();
    if values.len() < 5 {
        return None;
    }
    Some(CpuTimes {
        total: values.iter().sum(),
        idle: values[3] + values[4],
    })
}

/// Busy percentage between two cumulative samples. Counters only ever grow, so a smaller `now` means they were
/// reset (a suspend/resume) and the sample is reported as idle rather than as a nonsense spike.
fn busy_between(previous: CpuTimes, now: CpuTimes) -> f32 {
    let total = now.total.saturating_sub(previous.total);
    let idle = now.idle.saturating_sub(previous.idle);
    if total == 0 {
        return 0.0;
    }
    (100.0 * (total.saturating_sub(idle)) as f64 / total as f64) as f32
}

/// Every `cpu…` line of `/proc/stat`: the aggregate first, then one per core.
fn read_cpu_times(text: &str) -> Vec<CpuTimes> {
    text.lines().map_while(parse_cpu_line).collect()
}

/// Parses `/proc/meminfo`. "Used" follows what `free` reports: total minus available, which counts reclaimable
/// cache as free — the number a user recognises as their memory pressure.
fn parse_meminfo(text: &str) -> Memory {
    let field = |name: &str| -> u64 {
        text.lines()
            .find(|l| l.starts_with(name))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())
            .map(|kb| kb * 1024)
            .unwrap_or(0)
    };
    let total = field("MemTotal:");
    let available = field("MemAvailable:");
    let swap_total = field("SwapTotal:");
    let swap_free = field("SwapFree:");
    Memory {
        total,
        used: total.saturating_sub(available),
        swap_total,
        swap_used: swap_total.saturating_sub(swap_free),
    }
}

const HWMON_DIR: &str = "/sys/class/hwmon";

/// The hottest `tempN_input` across every hwmon device, in °C (the files are millidegrees). Taking the maximum
/// rather than naming a sensor keeps this working across AMD, Intel and laptops without a per-machine config.
fn read_temperature(hwmon: &Path) -> Option<f32> {
    let mut hottest: Option<f32> = None;
    for device in fs::read_dir(hwmon).ok()?.flatten() {
        let Ok(inputs) = fs::read_dir(device.path()) else {
            continue;
        };
        for input in inputs.flatten() {
            let name = input.file_name();
            let Some(name) = name.to_str() else { continue };
            if !(name.starts_with("temp") && name.ends_with("_input")) {
                continue;
            }
            let Some(millidegrees) = fs::read_to_string(input.path())
                .ok()
                .and_then(|t| t.trim().parse::<f32>().ok())
            else {
                continue;
            };
            let celsius = millidegrees / 1000.0;
            // Some sensors report obvious nonsense when unplugged or unpowered; a machine is not at 200 °C.
            if (1.0..=150.0).contains(&celsius) {
                hottest = Some(hottest.map_or(celsius, |h: f32| h.max(celsius)));
            }
        }
    }
    hottest
}

/// A mount's capacity, straight from the `statvfs` syscall. "Used" is total minus what an unprivileged user can
/// claim, which is what `df` reports: it excludes the root-reserved blocks, so a full disk reads as full.
fn read_disk(mount: &Path) -> Option<Disk> {
    let stats = rustix::fs::statvfs(mount).ok()?;
    // `f_frsize` is the fragment size the block counts are in; kernels that leave it 0 mean `f_bsize`.
    let block = if stats.f_frsize > 0 {
        stats.f_frsize
    } else {
        stats.f_bsize
    };
    let total = stats.f_blocks.checked_mul(block)?;
    let available = stats.f_bavail.saturating_mul(block);
    (total > 0).then(|| Disk {
        mount: mount.to_path_buf(),
        total,
        used: total.saturating_sub(available),
    })
}

/// The mounts worth charting: the root filesystem, and `/home` when the user put it on its own device.
fn interesting_mounts() -> Vec<PathBuf> {
    let root = PathBuf::from("/");
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut mounts = vec![root];
    if let Some(home) = home
        && let (Ok(home_meta), Ok(root_meta)) = (fs::metadata(&home), fs::metadata("/"))
    {
        use std::os::unix::fs::MetadataExt;
        if home_meta.dev() != root_meta.dev() {
            mounts.push(home);
        }
    }
    mounts
}

/// Ticks between disk re-reads. Free space moves in minutes, not seconds, so `statvfs`-ing every mount once a
/// second would be pure wakeup cost for a number that hasn't changed.
const DISK_EVERY: u32 = 30;

static RESOURCES: Service<Resources> = Service::new("hyprshell-resources", run);

fn run(out: &Arc<Broadcast<Resources>>) {
    let mut previous = read_cpu_times(&fs::read_to_string("/proc/stat").unwrap_or_default());
    let mut cpu_history = History::default();
    let mut memory_history = History::default();
    let mounts = interesting_mounts();
    let mut disks: Vec<Disk> = mounts.iter().filter_map(|m| read_disk(m)).collect();
    let mut tick: u32 = 0;
    loop {
        std::thread::sleep(POLL);
        let now = read_cpu_times(&fs::read_to_string("/proc/stat").unwrap_or_default());
        let busy: Vec<f32> = now
            .iter()
            .zip(previous.iter())
            .map(|(now, was)| busy_between(*was, *now))
            .collect();
        previous = now;

        let memory = parse_meminfo(&fs::read_to_string("/proc/meminfo").unwrap_or_default());
        let cpu = busy.first().copied().unwrap_or(0.0);
        cpu_history.push(cpu);
        memory_history.push(memory.used_percent());

        tick = tick.wrapping_add(1);
        if tick % DISK_EVERY == 0 {
            disks = mounts.iter().filter_map(|m| read_disk(m)).collect();
        }

        out.publish(Resources {
            cpu,
            cpu_history: cpu_history.clone(),
            cores: busy.into_iter().skip(1).collect(),
            memory_history: memory_history.clone(),
            memory,
            temperature: read_temperature(Path::new(HWMON_DIR)),
            disks: disks.clone(),
        });
    }
}

/// Registers `tx` for live system readings, starting the single shared poller on first use.
pub fn subscribe(tx: EventSender<Resources>) {
    RESOURCES.subscribe(tx);
}

/// The last reading, without waiting for the next tick.
pub fn current() -> Option<Resources> {
    RESOURCES.current()
}

/// Renders a byte count the way a person reads it: three significant figures and a binary unit.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_busy_is_the_non_idle_share_between_two_samples() {
        let was = CpuTimes {
            total: 1000,
            idle: 800,
        };
        let now = CpuTimes {
            total: 1100,
            idle: 850,
        };
        assert_eq!(busy_between(was, now), 50.0);
    }

    #[test]
    fn cpu_busy_survives_a_counter_reset_and_a_still_frame() {
        let high = CpuTimes {
            total: 10_000,
            idle: 5_000,
        };
        let reset = CpuTimes {
            total: 10,
            idle: 5,
        };
        assert_eq!(
            busy_between(high, reset),
            0.0,
            "a reset counter reads as idle, not as a spike"
        );
        assert_eq!(
            busy_between(high, high),
            0.0,
            "no elapsed time cannot divide by zero"
        );
    }

    #[test]
    fn proc_stat_yields_the_aggregate_then_one_entry_per_core() {
        let text = "\
cpu  100 0 50 800 50 0 0 0 0 0
cpu0 50 0 25 400 25 0 0 0 0 0
cpu1 50 0 25 400 25 0 0 0 0 0
intr 12345
";
        let times = read_cpu_times(text);
        assert_eq!(times.len(), 3, "aggregate + two cores, stopping at `intr`");
        assert_eq!(times[0].total, 1000);
        assert_eq!(times[0].idle, 850, "idle + iowait");
    }

    #[test]
    fn meminfo_reports_used_as_total_minus_available() {
        let text = "\
MemTotal:       16000000 kB
MemFree:         1000000 kB
MemAvailable:    8000000 kB
SwapTotal:       4000000 kB
SwapFree:        3000000 kB
";
        let memory = parse_meminfo(text);
        assert_eq!(memory.total, 16_000_000 * 1024);
        assert_eq!(
            memory.used,
            8_000_000 * 1024,
            "cache counts as free, matching `free`"
        );
        assert_eq!(memory.swap_used, 1_000_000 * 1024);
        assert_eq!(memory.used_percent(), 50.0);
    }

    #[test]
    fn meminfo_missing_fields_do_not_divide_by_zero() {
        let memory = parse_meminfo("");
        assert_eq!(memory.total, 0);
        assert_eq!(memory.used_percent(), 0.0);
        assert_eq!(memory.swap_percent(), 0.0);
    }

    #[test]
    fn temperature_takes_the_hottest_plausible_sensor() {
        let dir = std::env::temp_dir().join(format!("hyprshell-hwmon-{}", std::process::id()));
        let device = dir.join("hwmon0");
        fs::create_dir_all(&device).unwrap();
        fs::write(device.join("temp1_input"), "45000").unwrap();
        fs::write(device.join("temp2_input"), "61500").unwrap();
        // An unplugged sensor reporting an impossible value must not become "the temperature".
        fs::write(device.join("temp3_input"), "200000").unwrap();
        fs::write(device.join("name"), "coretemp").unwrap();

        assert_eq!(read_temperature(&dir), Some(61.5));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn no_hwmon_at_all_reports_no_temperature() {
        assert_eq!(read_temperature(Path::new("/nonexistent-hwmon")), None);
    }

    #[test]
    fn history_is_a_bounded_window_that_keeps_the_newest() {
        let mut history = History::default();
        for i in 0..(HISTORY + 10) {
            history.push(i as f32);
        }
        let values = history.values();
        assert_eq!(values.len(), HISTORY, "old readings fall off the front");
        assert_eq!(history.latest(), (HISTORY + 9) as f32);
        assert_eq!(values[0], 10.0, "the oldest kept reading is the 11th pushed");
        assert_eq!(history.peak(), (HISTORY + 9) as f32);
    }

    #[test]
    fn bytes_render_with_a_binary_unit_and_three_significant_figures() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1536), "1.5 KiB");
        assert_eq!(format_bytes(16 * 1024 * 1024 * 1024), "16.0 GiB");
        assert_eq!(format_bytes(900 * 1024 * 1024), "900 MiB");
    }
}
