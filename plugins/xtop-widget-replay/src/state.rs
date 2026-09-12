//! Build the serializable [`State`](xtop_wasm_contract::State) a guest
//! receives from the kernel's plugin view ([`PluginContext`]).

use std::time::{SystemTime, UNIX_EPOCH};

use xtop_plugin_api::{PluginContext, PluginError};
use xtop_wasm_contract as c;

/// Convert the kernel's data model into the serializable contract snapshot.
///
/// Processes are sorted by CPU usage (descending) before being truncated to
/// `max_processes`, so a widget that only needs "the top N" gets the useful
/// ones regardless of provider order.
pub fn snapshot_to_contract(
    snapshot: &xtop_plugin_api::SystemSnapshot,
    max_processes: usize,
) -> c::Snapshot {
    let mut processes: Vec<&xtop_plugin_api::ProcessInfo> = snapshot.processes.iter().collect();
    processes.sort_by(|a, b| b.cpu_usage.total_cmp(&a.cpu_usage));
    processes.truncate(max_processes);

    c::Snapshot {
        cpus: snapshot
            .cpus
            .iter()
            .map(|cpu| c::Cpu {
                name: cpu.name.clone(),
                usage: cpu.usage,
                cpu_id: cpu.cpu_id,
                frequency: cpu.frequency,
                governor: cpu.governor.clone(),
                temp_c: cpu.temp_c,
            })
            .collect(),
        memory: c::Memory {
            total: snapshot.memory.total,
            used: snapshot.memory.used,
            available: snapshot.memory.available,
            free: snapshot.memory.free,
            percent: snapshot.memory.percent,
        },
        swap: c::Swap {
            total: snapshot.swap.total,
            used: snapshot.swap.used,
            free: snapshot.swap.free,
            percent: snapshot.swap.percent,
        },
        disks: snapshot
            .disks
            .iter()
            .map(|disk| c::Disk {
                mount_point: disk.mount_point.clone(),
                total_space: disk.total_space,
                available_space: disk.available_space,
                used_space: disk.used_space,
                percent: disk.percent,
                file_system: disk.file_system.clone(),
                mount_options: disk.mount_options.clone(),
            })
            .collect(),
        networks: snapshot
            .networks
            .iter()
            .map(|net| c::Network {
                name: net.name.clone(),
                received: net.received,
                transmitted: net.transmitted,
                rx_speed: net.rx_speed,
                tx_speed: net.tx_speed,
                ip: net.ip.clone(),
            })
            .collect(),
        processes: processes
            .iter()
            .map(|p| c::Process {
                pid: p.pid,
                name: p.name.clone(),
                cpu_usage: p.cpu_usage,
                memory: p.memory,
                user_id: p.user_id.clone(),
                state: p.state.clone(),
                cmd: p.cmd.clone(),
                exe_path: p.exe_path.clone(),
                parent_pid: p.parent_pid,
                cmd_full: p.cmd_full.clone(),
                start_time: p.start_time,
                run_time: p.run_time,
                effective_user_id: p.effective_user_id.clone(),
                group_id: p.group_id.clone(),
                cwd: p.cwd.clone(),
                thread_count: p.thread_count,
                open_files: p.open_files,
                open_files_limit: p.open_files_limit,
                disk_total_read_bytes: p.disk_total_read_bytes,
                disk_total_write_bytes: p.disk_total_write_bytes,
                environ: p.environ.clone(),
                session_id: p.session_id,
            })
            .collect(),
        load: c::Load {
            one: snapshot.load_avg.one,
            five: snapshot.load_avg.five,
            fifteen: snapshot.load_avg.fifteen,
        },
        uptime: snapshot.uptime,
        cpu_temp: snapshot.cpu_temp,
        disk_io: snapshot
            .disk_io
            .iter()
            .map(|io| c::DiskIo {
                name: io.name.clone(),
                read_bytes: io.read_bytes,
                write_bytes: io.write_bytes,
                read_speed: io.read_speed,
                write_speed: io.write_speed,
            })
            .collect(),
        batteries: snapshot
            .batteries
            .iter()
            .map(|b| c::Battery {
                name: b.name.clone(),
                percentage: b.percentage,
                state: b.state.clone(),
                time_to_full: b.time_to_full,
                time_to_empty: b.time_to_empty,
                health: b.health,
                cycle_count: b.cycle_count,
            })
            .collect(),
        gpus: snapshot
            .gpus
            .iter()
            .map(|gpu| c::Gpu {
                name: gpu.name.clone(),
                usage: gpu.usage,
                temperature: gpu.temperature,
                memory_total: gpu.memory_total,
                memory_used: gpu.memory_used,
            })
            .collect(),
        sys: c::SysInfo {
            hostname: snapshot.sys_info.hostname.clone(),
            os_version: snapshot.sys_info.os_version.clone(),
            kernel: snapshot.sys_info.kernel.clone(),
            desktop_env: snapshot.sys_info.desktop_env.clone(),
            shell: snapshot.sys_info.shell.clone(),
            cpu_model: snapshot.sys_info.cpu_model.clone(),
            package_power_w: snapshot.sys_info.package_power_w,
        },
    }
}

/// Assemble the full guest state for one tick.
///
/// `width`/`height` are the last known widget-area size (zero before the
/// first frame; the host updates them at render time).
pub fn state_from_context(
    ctx: &PluginContext,
    tick: u64,
    width: u16,
    height: u16,
    max_processes: usize,
) -> Result<c::State, PluginError> {
    let snapshot = ctx.snapshot()?;
    let config = ctx.config();
    let alerts = ctx.alerts();
    let unix_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    Ok(c::State {
        tick,
        unix_time,
        width,
        height,
        config: c::RuntimeConfig {
            theme: config.theme,
            layout: config.layout,
            interval_ms: config.interval_ms,
            hostname: config.hostname,
        },
        alerts: c::Alerts {
            cpu_high: alerts.cpu_high,
            mem_high: alerts.mem_high,
            disk_high: alerts.disk_high,
        },
        snapshot: snapshot_to_contract(&snapshot, max_processes),
    })
}
