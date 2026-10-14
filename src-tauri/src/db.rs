//! SQLite storage: one row per 5s system sample, per-minute averages for the top apps.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;

pub const DEFAULT_RETENTION_DAYS: i64 = 90;
/// Charts never get more points than this; longer ranges are averaged into buckets.
const MAX_POINTS: i64 = 600;

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         CREATE TABLE IF NOT EXISTS system_samples(
             ts INTEGER PRIMARY KEY,
             cpu REAL, mem_used INTEGER, mem_total INTEGER, swap_used INTEGER,
             disk_read REAL, disk_write REAL, disk_busy REAL,
             net_rx REAL, net_tx REAL, gpu REAL, gpu_mem INTEGER);
         CREATE TABLE IF NOT EXISTS process_samples(
             ts INTEGER NOT NULL, name TEXT NOT NULL,
             cpu REAL, mem INTEGER, mem_max INTEGER, disk REAL, gpu REAL);
         CREATE INDEX IF NOT EXISTS idx_process_ts ON process_samples(ts);
         CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS app_info(name TEXT PRIMARY KEY, exe TEXT NOT NULL);",
    )?;
    migrate(&conn)?;
    Ok(conn)
}

/// Columns added after v0.1. SQLite has no ADD COLUMN IF NOT EXISTS, so check first.
fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let existing: Vec<String> = conn
        .prepare("SELECT name FROM pragma_table_info('system_samples')")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for col in ["cpu_temp", "gpu_temp", "disk_temp"] {
        if !existing.iter().any(|c| c == col) {
            conn.execute(&format!("ALTER TABLE system_samples ADD COLUMN {col} REAL"), [])?;
        }
    }
    Ok(())
}

pub fn get_setting(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
        .optional()
        .ok()
        .flatten()
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO settings(key, value) VALUES(?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )?;
    Ok(())
}

pub fn retention_days(conn: &Connection) -> i64 {
    get_setting(conn, "retention_days")
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_RETENTION_DAYS)
}

pub fn prune(conn: &Connection, now: i64) -> rusqlite::Result<()> {
    let cutoff = now - retention_days(conn) * 86_400;
    conn.execute("DELETE FROM system_samples WHERE ts < ?1", [cutoff])?;
    conn.execute("DELETE FROM process_samples WHERE ts < ?1", [cutoff])?;
    Ok(())
}

/// Column-oriented series for uPlot. `None` marks a gap (laptop asleep / app closed).
#[derive(Serialize, Default)]
pub struct Series {
    pub bucket: i64,
    pub t: Vec<i64>,
    pub cpu: Vec<Option<f64>>,
    pub mem: Vec<Option<f64>>,
    pub mem_used: Vec<Option<f64>>,
    pub disk_read: Vec<Option<f64>>,
    pub disk_write: Vec<Option<f64>>,
    pub gpu: Vec<Option<f64>>,
    pub net_rx: Vec<Option<f64>>,
    pub net_tx: Vec<Option<f64>>,
    pub cpu_temp: Vec<Option<f64>>,
    pub gpu_temp: Vec<Option<f64>>,
    pub disk_temp: Vec<Option<f64>>,
}

pub fn series(conn: &Connection, from: i64, to: i64) -> rusqlite::Result<Series> {
    let bucket = ((to - from) / MAX_POINTS).max(5);
    let mut stmt = conn.prepare(
        "SELECT (ts / ?1) * ?1 AS b, AVG(cpu), AVG(mem_used * 100.0 / mem_total), AVG(mem_used),
                AVG(disk_read), AVG(disk_write), AVG(gpu), AVG(net_rx), AVG(net_tx),
                AVG(cpu_temp), AVG(gpu_temp), AVG(disk_temp)
         FROM system_samples WHERE ts >= ?2 AND ts <= ?3
         GROUP BY b ORDER BY b",
    )?;
    let mut s = Series { bucket, ..Default::default() };
    let mut last: Option<i64> = None;
    let mut rows = stmt.query(params![bucket, from, to])?;
    while let Some(r) = rows.next()? {
        let t: i64 = r.get(0)?;
        // Insert an empty point across gaps so the chart does not draw a line through them.
        if let Some(prev) = last {
            if t - prev > bucket * 3 {
                s.t.push(prev + bucket);
                for v in [&mut s.cpu, &mut s.mem, &mut s.mem_used, &mut s.disk_read, &mut s.disk_write,
                          &mut s.gpu, &mut s.net_rx, &mut s.net_tx, &mut s.cpu_temp, &mut s.gpu_temp,
                          &mut s.disk_temp] {
                    v.push(None);
                }
            }
        }
        last = Some(t);
        s.t.push(t);
        s.cpu.push(r.get(1)?);
        s.mem.push(r.get(2)?);
        s.mem_used.push(r.get(3)?);
        s.disk_read.push(r.get(4)?);
        s.disk_write.push(r.get(5)?);
        s.gpu.push(r.get(6)?);
        s.net_rx.push(r.get(7)?);
        s.net_tx.push(r.get(8)?);
        s.cpu_temp.push(r.get(9)?);
        s.gpu_temp.push(r.get(10)?);
        s.disk_temp.push(r.get(11)?);
    }
    Ok(s)
}

#[derive(Serialize)]
pub struct TopApp {
    pub name: String,
    pub cpu: f64,
    pub mem: f64,
    pub mem_max: i64,
    pub disk: f64,
    pub gpu: f64,
    /// Minutes in the range where this app was among the top apps.
    pub minutes: i64,
}

/// Apps ranked by `metric` over [from, to]. Averages are over the whole range,
/// so an app that ran only 5 of 60 minutes is not overstated.
pub fn top_apps(conn: &Connection, from: i64, to: i64, metric: &str) -> rusqlite::Result<Vec<TopApp>> {
    let order = match metric {
        "mem" => "mem",
        "disk" => "disk",
        "gpu" => "gpu",
        _ => "cpu",
    };
    // Snap to minute boundaries: process rows are stored per minute.
    let from = from / 60 * 60;
    let total: f64 = conn.query_row(
        "SELECT COUNT(DISTINCT ts) FROM process_samples WHERE ts >= ?1 AND ts <= ?2",
        params![from, to],
        |r| r.get::<_, i64>(0),
    )? as f64;
    if total == 0.0 {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT name, SUM(cpu) / ?3 AS cpu, SUM(mem) / ?3 AS mem, MAX(mem_max), SUM(disk) / ?3 AS disk,
                SUM(gpu) / ?3 AS gpu, COUNT(*)
         FROM process_samples WHERE ts >= ?1 AND ts <= ?2
         GROUP BY name ORDER BY {order} DESC LIMIT 15"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![from, to, total], |r| {
        Ok(TopApp {
            name: r.get(0)?,
            cpu: r.get(1)?,
            mem: r.get(2)?,
            mem_max: r.get(3)?,
            disk: r.get(4)?,
            gpu: r.get(5)?,
            minutes: r.get(6)?,
        })
    })?;
    rows.collect()
}

/// One app's per-minute rows. Minutes where it was not among the top apps are absent.
#[derive(Serialize, Default)]
pub struct AppHistory {
    pub t: Vec<i64>,
    pub cpu: Vec<f64>,
    pub mem: Vec<f64>,
    pub disk: Vec<f64>,
    pub gpu: Vec<f64>,
}

pub fn app_history(conn: &Connection, name: &str, from: i64, to: i64) -> rusqlite::Result<AppHistory> {
    let mut stmt = conn.prepare(
        "SELECT ts, cpu, mem, disk, gpu FROM process_samples
         WHERE name = ?1 AND ts >= ?2 AND ts <= ?3 ORDER BY ts",
    )?;
    let mut h = AppHistory::default();
    let mut rows = stmt.query(params![name, from / 60 * 60, to])?;
    while let Some(r) = rows.next()? {
        h.t.push(r.get(0)?);
        h.cpu.push(r.get(1)?);
        h.mem.push(r.get::<_, i64>(2)? as f64);
        h.disk.push(r.get(3)?);
        h.gpu.push(r.get(4)?);
    }
    Ok(h)
}

/// Remembers where an app's exe lives, so its icon can be shown even after it has closed.
pub fn save_app_exe(conn: &Connection, name: &str, exe: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO app_info(name, exe) VALUES(?1, ?2) ON CONFLICT(name) DO UPDATE SET exe = excluded.exe",
        [name, exe],
    )?;
    Ok(())
}

pub fn app_exe(conn: &Connection, name: &str) -> Option<String> {
    conn.query_row("SELECT exe FROM app_info WHERE name = ?1", [name], |r| r.get(0)).optional().ok().flatten()
}

/// Earliest recorded sample, so the UI can say how far back history goes.
pub fn first_ts(conn: &Connection) -> Option<i64> {
    conn.query_row("SELECT MIN(ts) FROM system_samples", [], |r| r.get(0)).ok().flatten()
}
