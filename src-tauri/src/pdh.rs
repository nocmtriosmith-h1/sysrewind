//! Minimal bindings to Windows PDH (Performance Data Helper).
//! Used for counters sysinfo does not expose: GPU engines (per process) and physical disk.

use std::collections::HashMap;
use std::ptr::{null, null_mut};

#[repr(C)]
#[derive(Clone, Copy)]
struct FmtValue {
    status: u32,
    _pad: u32,
    value: f64,
}

#[repr(C)]
struct FmtItem {
    name: *mut u16,
    value: FmtValue,
}

const PDH_FMT_DOUBLE: u32 = 0x0000_0200;
const PDH_FMT_NOCAP100: u32 = 0x0000_8000;
const PDH_MORE_DATA: u32 = 0x8000_07D2;
/// PDH_CSTATUS_VALID_DATA = 0, PDH_CSTATUS_NEW_DATA = 1.
const MAX_OK_STATUS: u32 = 1;

#[link(name = "pdh")]
extern "system" {
    fn PdhOpenQueryW(source: *const u16, user: usize, query: *mut isize) -> u32;
    fn PdhAddEnglishCounterW(query: isize, path: *const u16, user: usize, counter: *mut isize) -> u32;
    fn PdhCollectQueryData(query: isize) -> u32;
    fn PdhGetFormattedCounterArrayW(
        counter: isize,
        format: u32,
        size: *mut u32,
        count: *mut u32,
        items: *mut FmtItem,
    ) -> u32;
    fn PdhCloseQuery(query: isize) -> u32;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn from_wide(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut len = 0;
    while *p.add(len) != 0 {
        len += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
}

/// A PDH query holding several wildcard counters. Counter index = position in `paths`.
pub struct Query {
    handle: isize,
    counters: Vec<isize>,
}

impl Query {
    pub fn new(paths: &[&str]) -> Option<Self> {
        let mut handle = 0isize;
        if unsafe { PdhOpenQueryW(null(), 0, &mut handle) } != 0 {
            return None;
        }
        let counters = paths
            .iter()
            .map(|p| {
                let w = wide(p);
                let mut c = 0isize;
                // A missing counter (e.g. no GPU driver support) leaves 0 and reads as empty.
                if unsafe { PdhAddEnglishCounterW(handle, w.as_ptr(), 0, &mut c) } == 0 {
                    c
                } else {
                    0
                }
            })
            .collect();
        // Rate counters need a first sample before they return values.
        unsafe { PdhCollectQueryData(handle) };
        Some(Self { handle, counters })
    }

    pub fn collect(&self) {
        unsafe { PdhCollectQueryData(self.handle) };
    }

    /// All instances of counter `idx` as (instance name, value).
    pub fn read(&self, idx: usize) -> Vec<(String, f64)> {
        let counter = match self.counters.get(idx) {
            Some(&c) if c != 0 => c,
            _ => return Vec::new(),
        };
        let fmt = PDH_FMT_DOUBLE | PDH_FMT_NOCAP100;
        // Instances can appear between the size probe and the read, so retry a few times.
        for _ in 0..3 {
            let mut size = 0u32;
            let mut count = 0u32;
            let rc = unsafe { PdhGetFormattedCounterArrayW(counter, fmt, &mut size, &mut count, null_mut()) };
            if rc != PDH_MORE_DATA || size == 0 {
                return Vec::new();
            }
            let mut buf: Vec<u64> = vec![0; (size as usize).div_ceil(8)];
            let items = buf.as_mut_ptr() as *mut FmtItem;
            let rc = unsafe { PdhGetFormattedCounterArrayW(counter, fmt, &mut size, &mut count, items) };
            if rc == PDH_MORE_DATA {
                continue;
            }
            if rc != 0 {
                return Vec::new();
            }
            let mut out = Vec::with_capacity(count as usize);
            for i in 0..count as usize {
                let item = unsafe { &*items.add(i) };
                if item.value.status <= MAX_OK_STATUS {
                    out.push((unsafe { from_wide(item.name) }, item.value.value));
                }
            }
            return out;
        }
        Vec::new()
    }

    /// Sum of all instances of counter `idx`, or None if the counter is unavailable.
    pub fn sum(&self, idx: usize) -> Option<f64> {
        let items = self.read(idx);
        if items.is_empty() {
            None
        } else {
            Some(items.iter().map(|(_, v)| v).sum())
        }
    }
}

impl Drop for Query {
    fn drop(&mut self) {
        unsafe { PdhCloseQuery(self.handle) };
    }
}

/// Turns `\GPU Engine(*)\Utilization Percentage` instances into total GPU % and GPU % per PID.
/// Instance names look like `pid_1234_luid_0x0_0xC6D5_phys_0_eng_3_engtype_3D`.
/// Like Task Manager: sum per engine type, then take the busiest engine type.
pub fn gpu_usage(items: &[(String, f64)]) -> (f64, HashMap<u32, f64>) {
    let mut by_type: HashMap<String, f64> = HashMap::new();
    let mut by_pid_type: HashMap<(u32, String), f64> = HashMap::new();
    for (name, value) in items {
        let Some(engtype) = name.split("engtype_").nth(1) else { continue };
        *by_type.entry(engtype.to_string()).or_default() += value;
        let pid = name
            .strip_prefix("pid_")
            .and_then(|s| s.split('_').next())
            .and_then(|s| s.parse::<u32>().ok());
        if let Some(pid) = pid {
            *by_pid_type.entry((pid, engtype.to_string())).or_default() += value;
        }
    }
    let total = by_type.values().cloned().fold(0.0, f64::max).min(100.0);
    let mut per_pid: HashMap<u32, f64> = HashMap::new();
    for ((pid, _), v) in by_pid_type {
        let e = per_pid.entry(pid).or_default();
        *e = e.max(v.min(100.0));
    }
    (total, per_pid)
}
