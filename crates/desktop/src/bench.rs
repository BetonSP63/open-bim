//! In-app benchmark: frame intervals with a moving camera after warm-up,
//! ray picking latency and peak resident memory, written as JSON.
use serde::Serialize;
use std::time::{Duration, Instant};

#[derive(Debug, Serialize)]
pub struct Results {
    pub objects: usize,
    pub window_points: [f32; 2],
    pub pixels_per_point: f32,
    pub gpu: String,
    pub load_ms: f64,
    pub warmup_frames: u32,
    pub frames: usize,
    pub frame_ms_p50: f64,
    pub frame_ms_p95: f64,
    pub frame_ms_max: f64,
    pub picks: usize,
    pub pick_ms_p95: f64,
    pub pick_ms_max: f64,
    pub peak_rss_mb: f64,
}

pub struct Bench {
    pub path: std::path::PathBuf,
    pub warmup: u32,
    pub frames: usize,
    frame: u32,
    last: Option<Instant>,
    pub intervals: Vec<Duration>,
}

impl Bench {
    pub fn new(path: std::path::PathBuf, frames: usize) -> Self {
        Self {
            path,
            warmup: 120,
            frames,
            frame: 0,
            last: None,
            intervals: Vec::new(),
        }
    }
    /// Call once per frame; returns true when measurement is complete.
    pub fn tick(&mut self) -> bool {
        let now = Instant::now();
        self.frame += 1;
        if self.frame > self.warmup
            && let Some(last) = self.last
        {
            self.intervals.push(now - last);
        }
        self.last = Some(now);
        self.intervals.len() >= self.frames
    }
}

pub fn percentile(values: &mut [f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.;
    }
    values.sort_by(f64::total_cmp);
    let index = ((values.len() as f64 - 1.) * p).round() as usize;
    values[index.min(values.len() - 1)]
}

/// Peak resident set size of this process in MiB (macOS `ps`, kilobytes).
pub fn peak_rss_mb() -> f64 {
    std::process::Command::new("/bin/ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<f64>().ok())
        .map_or(0., |kb| kb / 1024.)
}
