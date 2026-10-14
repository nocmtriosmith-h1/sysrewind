//! System counters sysinfo does not provide: GPU engine usage (per process), physical-disk
//! throughput and busy %, GPU memory and the ACPI thermal zone.
//! Windows reads them from PDH; other platforms return None and the collector falls back
//! (disk throughput = sum of per-process I/O, no GPU usage).

use std::collections::HashMap;

pub struct DiskIo {
    pub read: f64,
    pub write: f64,
    pub busy: f64,
}

#[cfg(windows)]
pub struct Counters {
    query: Option<crate::pdh::Query>,
}

#[cfg(windows)]
const GPU_ENGINE: usize = 0;
#[cfg(windows)]
const GPU_MEM: usize = 1;
#[cfg(windows)]
const DISK_READ: usize = 2;
#[cfg(windows)]
const DISK_WRITE: usize = 3;
#[cfg(windows)]
const DISK_IDLE: usize = 4;
#[cfg(windows)]
const THERMAL_ZONE: usize = 5;
#[cfg(windows)]
const PDH_COUNTERS: [&str; 6] = [
    r"\GPU Engine(*)\Utilization Percentage",
    r"\GPU Adapter Memory(*)\Dedicated Usage",
    r"\PhysicalDisk(_Total)\Disk Read Bytes/sec",
    r"\PhysicalDisk(_Total)\Disk Write Bytes/sec",
    r"\PhysicalDisk(_Total)\% Idle Time",
    r"\Thermal Zone Information(*)\Temperature",
];

#[cfg(windows)]
impl Counters {
    pub fn new() -> Self {
        Self { query: crate::pdh::Query::new(&PDH_COUNTERS) }
    }

    pub fn collect(&self) {
        if let Some(q) = &self.query {
            q.collect();
        }
    }

    /// Total GPU % and GPU % per PID.
    pub fn gpu(&self) -> (Option<f64>, HashMap<u32, f64>) {
        match &self.query {
            Some(q) => {
                let items = q.read(GPU_ENGINE);
                if items.is_empty() {
                    (None, HashMap::new())
                } else {
                    let (total, per_pid) = crate::pdh::gpu_usage(&items);
                    (Some(total), per_pid)
                }
            }
            None => (None, HashMap::new()),
        }
    }

    pub fn gpu_mem(&self) -> Option<u64> {
        self.query.as_ref()?.sum(GPU_MEM).map(|v| v as u64)
    }

    pub fn disk(&self) -> Option<DiskIo> {
        let q = self.query.as_ref()?;
        let (read, write) = (q.sum(DISK_READ)?, q.sum(DISK_WRITE)?);
        let busy = q.sum(DISK_IDLE).map(|i| (100.0 - i).clamp(0.0, 100.0)).unwrap_or(0.0);
        Some(DiskIo { read, write, busy })
    }

    /// Hottest ACPI thermal zone in °C (PDH reports Kelvin).
    pub fn acpi_celsius(&self) -> Option<f64> {
        self.query.as_ref()?.read(THERMAL_ZONE).into_iter().map(|(_, k)| k - 273.15).reduce(f64::max)
    }
}

#[cfg(not(windows))]
pub struct Counters;

#[cfg(not(windows))]
impl Counters {
    pub fn new() -> Self {
        Self
    }
    pub fn collect(&self) {}
    pub fn gpu(&self) -> (Option<f64>, HashMap<u32, f64>) {
        (None, HashMap::new())
    }
    pub fn gpu_mem(&self) -> Option<u64> {
        None
    }
    pub fn disk(&self) -> Option<DiskIo> {
        None
    }
    pub fn acpi_celsius(&self) -> Option<f64> {
        None
    }
}
