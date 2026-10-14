//! Turns raw samples into plain-language "episodes": periods where the PC was under strain,
//! each with the app most responsible. The UI words them ("PC slow for 7 min: CPU 87%, mostly ffmpeg").

use rusqlite::{params, Connection};
use serde::Serialize;

/// A condition that must hold for at least `min_secs` to count as an episode.
struct Rule {
    kind: &'static str,
    min_secs: i64,
    /// Column of process_samples used to find the responsible app (None = no app attribution).
    app_metric: Option<&'static str>,
    test: fn(&Row) -> Option<f64>,
}

struct Row {
    ts: i64,
    cpu: Option<f64>,
    mem: Option<f64>,
    disk_busy: Option<f64>,
    cpu_temp: Option<f64>,
    gpu_temp: Option<f64>,
}

const RULES: [Rule; 5] = [
    Rule { kind: "cpu_high", min_secs: 120, app_metric: Some("cpu"), test: |r| r.cpu.filter(|v| *v >= 80.0) },
    Rule { kind: "mem_high", min_secs: 600, app_metric: Some("mem"), test: |r| r.mem.filter(|v| *v >= 85.0) },
    Rule { kind: "disk_busy", min_secs: 120, app_metric: Some("disk"), test: |r| r.disk_busy.filter(|v| *v >= 80.0) },
    Rule { kind: "cpu_hot", min_secs: 60, app_metric: Some("cpu"), test: |r| r.cpu_temp.filter(|v| *v >= 90.0) },
    Rule { kind: "gpu_hot", min_secs: 60, app_metric: Some("gpu"), test: |r| r.gpu_temp.filter(|v| *v >= 85.0) },
];

/// Samples are 5s apart; a run survives this many failing samples in a row (brief dips).
const MAX_DIP_SAMPLES: u32 = 2;
/// A gap longer than this (sleep, app closed) always ends a run.
const MAX_GAP_SECS: i64 = 30;

#[derive(Serialize)]
pub struct Insight {
    pub kind: &'static str,
    pub from: i64,
    pub to: i64,
    pub peak: f64,
    pub avg: f64,
    pub app: Option<String>,
    pub app_value: Option<f64>,
}

pub fn detect(conn: &Connection, from: i64, to: i64) -> rusqlite::Result<Vec<Insight>> {
    let mut stmt = conn.prepare(
        "SELECT ts, cpu, mem_used * 100.0 / mem_total, disk_busy, cpu_temp, gpu_temp
         FROM system_samples WHERE ts >= ?1 AND ts <= ?2 ORDER BY ts",
    )?;
    let rows: Vec<Row> = stmt
        .query_map(params![from, to], |r| {
            Ok(Row {
                ts: r.get(0)?,
                cpu: r.get(1)?,
                mem: r.get(2)?,
                disk_busy: r.get(3)?,
                cpu_temp: r.get(4)?,
                gpu_temp: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    let mut out = Vec::new();
    for rule in &RULES {
        for (start, end, peak, avg) in runs(&rows, rule) {
            let (app, app_value) = match rule.app_metric {
                Some(metric) => top_app(conn, start, end, metric)?.unzip(),
                None => (None, None),
            };
            out.push(Insight { kind: rule.kind, from: start, to: end, peak, avg, app, app_value });
        }
    }
    out.sort_by_key(|i| std::cmp::Reverse(i.from));
    out.truncate(50);
    Ok(out)
}

/// (start, end, peak, avg) of every run of `rule` lasting at least `rule.min_secs`.
fn runs(rows: &[Row], rule: &Rule) -> Vec<(i64, i64, f64, f64)> {
    let mut found = Vec::new();
    let mut cur: Option<(i64, i64, f64, f64, u32)> = None; // start, last_ts, peak, sum, count
    let mut dips = 0u32;
    let mut prev_ts: Option<i64> = None;

    let close = |cur: &mut Option<(i64, i64, f64, f64, u32)>, found: &mut Vec<_>| {
        if let Some((start, last, peak, sum, n)) = cur.take() {
            if last - start >= rule.min_secs {
                found.push((start, last, peak, sum / n as f64));
            }
        }
    };

    for r in rows {
        if prev_ts.is_some_and(|p| r.ts - p > MAX_GAP_SECS) {
            close(&mut cur, &mut found);
        }
        prev_ts = Some(r.ts);
        match ((rule.test)(r), cur.as_mut()) {
            (Some(v), Some(c)) => {
                c.1 = r.ts;
                c.2 = c.2.max(v);
                c.3 += v;
                c.4 += 1;
                dips = 0;
            }
            (Some(v), None) => {
                cur = Some((r.ts, r.ts, v, v, 1));
                dips = 0;
            }
            (None, Some(_)) => {
                dips += 1;
                if dips > MAX_DIP_SAMPLES {
                    close(&mut cur, &mut found);
                }
            }
            (None, None) => {}
        }
    }
    close(&mut cur, &mut found);
    found
}

/// The app with the highest average `metric` over [from, to] (per-minute rows).
fn top_app(conn: &Connection, from: i64, to: i64, metric: &str) -> rusqlite::Result<Option<(String, f64)>> {
    let col = match metric {
        "mem" => "mem",
        "disk" => "disk",
        "gpu" => "gpu",
        _ => "cpu",
    };
    let from = from / 60 * 60;
    let sql = format!(
        "SELECT name, SUM({col}) * 1.0 / (SELECT MAX(COUNT(DISTINCT ts), 1) FROM process_samples WHERE ts >= ?1 AND ts <= ?2)
         FROM process_samples WHERE ts >= ?1 AND ts <= ?2
         GROUP BY name ORDER BY SUM({col}) DESC LIMIT 1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params![from, to])?;
    match rows.next()? {
        Some(r) => Ok(Some((r.get(0)?, r.get(1)?))),
        None => Ok(None),
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn cpu_rows(values: &[(i64, f64)]) -> Vec<Row> {
        values
            .iter()
            .map(|&(ts, cpu)| Row { ts, cpu: Some(cpu), mem: None, disk_busy: None, cpu_temp: None, gpu_temp: None })
            .collect()
    }

    fn every_5s(from: i64, secs: i64, cpu: f64) -> Vec<(i64, f64)> {
        (0..=secs / 5).map(|i| (from + i * 5, cpu)).collect()
    }

    #[test]
    fn long_high_cpu_is_one_episode() {
        let mut v = every_5s(0, 60, 20.0);
        v.extend(every_5s(65, 180, 90.0));
        v.extend(every_5s(250, 60, 20.0));
        let found = runs(&cpu_rows(&v), &RULES[0]);
        assert_eq!(found.len(), 1);
        let (start, end, peak, _) = found[0];
        assert_eq!((start, end, peak), (65, 245, 90.0));
    }

    #[test]
    fn short_spike_is_ignored() {
        let mut v = every_5s(0, 60, 20.0);
        v.extend(every_5s(65, 60, 95.0));
        assert!(runs(&cpu_rows(&v), &RULES[0]).is_empty());
    }

    #[test]
    fn brief_dip_does_not_split_episode() {
        let mut v = every_5s(0, 90, 90.0);
        v.push((95, 40.0));
        v.extend(every_5s(100, 90, 90.0));
        assert_eq!(runs(&cpu_rows(&v), &RULES[0]).len(), 1);
    }

    #[test]
    fn sleep_gap_splits_episodes() {
        let mut v = every_5s(0, 150, 90.0);
        v.extend(every_5s(1000, 150, 90.0));
        assert_eq!(runs(&cpu_rows(&v), &RULES[0]).len(), 2);
    }
}
