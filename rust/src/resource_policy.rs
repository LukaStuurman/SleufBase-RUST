//! Native port of active resource_policy.py. Pure resolution is independently
//! fixture tested; the process-wide hardware/env snapshot is read once.
use std::{collections::BTreeMap, sync::OnceLock};
const MIB: u64 = 1024 * 1024;
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct HardwareSnapshot {
    pub logical_cpus: usize,
    pub total_memory_bytes: u64,
    pub available_memory_bytes: u64,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResourcePolicy {
    pub mode: String,
    pub logical_cpus: usize,
    pub total_memory_bytes: u64,
    pub available_memory_bytes: u64,
    pub network_workers: usize,
    pub local_geometry_workers: usize,
    pub light_map_workers: usize,
    pub template_asset_workers: usize,
    pub heavy_raster_workers: usize,
    pub virtual_tiff_pixel_budget: u64,
    pub tile_memory_cache_entries: usize,
    pub wms_cache_bytes: u64,
}
fn positive(env: &BTreeMap<String, String>, key: &str) -> u64 {
    env.get(key)
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(0)
}
pub fn build(snapshot: HardwareSnapshot, env: &BTreeMap<String, String>) -> ResourcePolicy {
    let cpu = snapshot.logical_cpus.max(1);
    let gib = if snapshot.total_memory_bytes == 0 {
        8.
    } else {
        snapshot.total_memory_bytes as f64 / (1024u64.pow(3)) as f64
    };
    let tier = if gib < 6. {
        0
    } else if gib < 12. {
        1
    } else if gib < 24. {
        2
    } else if gib < 48. {
        3
    } else if gib < 96. {
        4
    } else {
        5
    };
    let cap = [2usize, 4, 6, 8, 10, 12][tier];
    let mut workers = [
        cpu.saturating_mul(2).clamp(2, 24).min((cap * 2).max(2)),
        cpu.div_ceil(2).clamp(1, 12).min(cap),
        cpu.div_ceil(2).clamp(1, 12).min(cap),
        cpu.div_ceil(2).clamp(1, 10).min(cap),
        if gib < 16. || cpu < 8 {
            1
        } else if gib < 32. || cpu < 16 {
            2
        } else if gib < 64. || cpu < 24 {
            3
        } else {
            4
        },
    ];
    let mut pixels = [
        12_000_000, 16_000_000, 20_000_000, 32_000_000, 48_000_000, 64_000_000,
    ][tier];
    let mut tiles = [96, 160, 256, 512, 768, 1024][tier];
    let mut bytes = [24, 48, 64, 128, 192, 256][tier] * MIB;
    let raw = env
        .get("SLEUFBASE_RESOURCE_MODE")
        .map_or("auto", String::as_str)
        .trim()
        .to_lowercase();
    let mode = if ["auto", "eco", "balanced", "performance"].contains(&raw.as_str()) {
        raw
    } else {
        "auto".into()
    };
    match mode.as_str() {
        "eco" => {
            workers = workers.map(|n| n.div_ceil(2).max(1));
            workers[4] = 1;
            pixels = pixels.min(16_000_000);
            tiles = tiles.min(160);
            bytes = bytes.min(48 * MIB);
        }
        "balanced" => {
            workers = workers.map(|n| n.clamp(1, 8));
            workers[4] = workers[4].min(2);
            pixels = pixels.min(32_000_000);
            tiles = tiles.min(512);
            bytes = bytes.min(128 * MIB);
        }
        "performance" => {
            workers[0] = workers[0].clamp(8, 32);
            workers[1] = workers[1].clamp(4, 16);
            workers[2] = workers[2].clamp(4, 16);
            workers[3] = workers[3].clamp(4, 12);
        }
        _ => {}
    }
    let global = positive(env, "SLEUFBASE_MAX_WORKERS");
    for (i, key) in ["NETWORK", "GEOMETRY", "MAP", "TEMPLATE", "RASTER"]
        .iter()
        .enumerate()
    {
        let explicit = positive(env, &format!("SLEUFBASE_{key}_WORKERS"));
        if explicit > 0 {
            workers[i] = explicit.min(usize::MAX as u64) as usize;
        }
        if global > 0 {
            workers[i] = workers[i].min(global.min(usize::MAX as u64) as usize);
        }
        workers[i] = workers[i].max(1);
    }
    let value = positive(env, "SLEUFBASE_TIFF_PIXEL_BUDGET");
    if value > 0 {
        pixels = value.max(4_000_000);
    }
    let value = positive(env, "SLEUFBASE_TILE_CACHE_ENTRIES");
    if value > 0 {
        tiles = (value as usize).max(32);
    }
    let value = positive(env, "SLEUFBASE_WMS_CACHE_MB");
    if value > 0 {
        bytes = value.saturating_mul(MIB).max(8 * MIB);
    }
    ResourcePolicy {
        mode,
        logical_cpus: cpu,
        total_memory_bytes: snapshot.total_memory_bytes,
        available_memory_bytes: snapshot.available_memory_bytes,
        network_workers: workers[0],
        local_geometry_workers: workers[1],
        light_map_workers: workers[2],
        template_asset_workers: workers[3],
        heavy_raster_workers: workers[4],
        virtual_tiff_pixel_budget: pixels,
        tile_memory_cache_entries: tiles,
        wms_cache_bytes: bytes,
    }
}
#[cfg(windows)]
fn memory_status() -> (u64, u64) {
    unsafe {
        use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        let mut status: MEMORYSTATUSEX = std::mem::zeroed();
        status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut status) != 0 {
            (status.ullTotalPhys, status.ullAvailPhys)
        } else {
            (0, 0)
        }
    }
}
#[cfg(not(windows))]
fn memory_status() -> (u64, u64) {
    let text = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let value = |key: &str| {
        text.lines()
            .find_map(|line| {
                line.strip_prefix(key)
                    .and_then(|tail| tail.split_whitespace().next())
                    .and_then(|n| n.parse::<u64>().ok())
            })
            .unwrap_or(0)
            .saturating_mul(1024)
    };
    (value("MemTotal:"), value("MemAvailable:"))
}
pub fn detect_hardware() -> HardwareSnapshot {
    let (total, available) = memory_status();
    HardwareSnapshot {
        logical_cpus: std::thread::available_parallelism().map_or(1, usize::from),
        total_memory_bytes: total,
        available_memory_bytes: available,
    }
}

pub fn get() -> &'static ResourcePolicy {
    static POLICY: OnceLock<ResourcePolicy> = OnceLock::new();
    POLICY.get_or_init(|| build(detect_hardware(), &std::env::vars().collect()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_resource_policy_oracle() {
        let rows: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/resource-policy/reference.json"
        ))
        .unwrap();
        for row in rows.as_array().unwrap() {
            let snapshot = serde_json::from_value(row["snapshot"].clone()).unwrap();
            let env = serde_json::from_value(row["env"].clone()).unwrap();
            let expected: ResourcePolicy = serde_json::from_value(row["expected"].clone()).unwrap();
            assert_eq!(build(snapshot, &env), expected, "{row}");
        }
    }
}
