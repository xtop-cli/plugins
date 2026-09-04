# Samurai heuristic rules

Samurai's analyzer runs the ten rules below over the live process table and
keeps up to **50 alerts** per run (`MAX_ALERTS`, truncated at the end of the
pass). Each rule returns at most one alert per process per run; rules are
evaluated for every process in priority order, so a single process can produce
several alerts. Severities: **Critical**, **Warning**, **Info**.

Alerts persist between runs: a new analysis replaces the whole alert list, so
`process.alerts` / `alerts.status` reflect the **last completed run** (runs
happen every 5th tick — see [architecture.md](architecture.md)).

All pattern matching is plain **substring / regex containment** on the
searched field — there is no slash-wrapping and no whole-string anchoring.
The listed thresholds are the exact constants in `src/lib.rs`.

---

## Rule 1 — Suspicious executable path (`suspicious_exe_path`)

- **Trigger**: `exe_path` starts with one of
  `/tmp/`, `/dev/shm/`, `/var/tmp/`, `/proc/`, `/private/tmp/`,
  `/private/var/tmp/`.
- **Severity**: Critical.
- **Knobs**: allow-list of prefixes `SUSPICIOUS_PATH_PREFIXES`.

## Rule 2 — Orphan process (`orphan_process`)

- **Trigger**: `parent_pid == 1` and the lowercase process name does **not**
  contain any of the allowed daemon names: `systemd`, `init`, `launchd`,
  `sshd`, `login`, `getty`, `nginx`, `apache2`, `httpd`, `bash`, `sh`, `zsh`,
  `tmux`, `screen`.
- **Severity**: Critical when `run_time < 60` seconds, else Warning.
- **Knobs**: allow-list `ALLOWED_ORPHANS`; the 60 s severity cutoff is a
  literal in the rule.

## Rule 3 — Masquerading (`process_masquerading`)

Two independent checks:

1. The name is a known system process (`svchost`, `lsass`, `launchd`,
   `sshd`, `systemd`, `init`) but `exe_path` is **not** under `/usr/`,
   `/bin/`, `/sbin/`, or `/System/` → **Critical**.
2. Otherwise the name differs from the `exe_path` file stem
   (case-insensitive) and neither string contains the other — e.g.
   `name "kitt"` with `exe "/opt/vendor/pwn"` → **Warning**. Common aliases
   such as `python3` vs `/usr/bin/python3.11` are allowed (the exe path
   contains the name).

## Rule 4 — Privilege escalation (`privilege_escalation`)

- **Trigger**: `effective_user_id != user_id` (both present).
- **Skips**: known SUID binaries — `/usr/bin/sudo`, `/usr/bin/passwd`,
  `/bin/ping`, `/usr/bin/ping`, `/bin/su`, `/usr/bin/su`, `/usr/bin/newgrp`,
  `/usr/bin/gpasswd`, `/usr/bin/chsh`, `/usr/bin/chfn`, `/usr/bin/mount`,
  `/usr/bin/umount`.
- **Severity**: Critical when `euid == "0"` (escalation to root), else
  Warning.

## Rule 5 — Suspicious browser child (`suspicious_child_of_browser`)

- **Trigger**: the parent process (resolved through the snapshot's PID map)
  is a browser — name contains `chrome`, `firefox`, `safari`, `edge`,
  `brave`, `opera`, or `chromium` — and the child's name contains none of
  the expected helper/sandbox words: `helper`, `plugin_container`,
  `plugin_host`, `gpu_process`, `renderer`, `utility`, `crashpad`,
  `updater`.
- **Severity**: Warning.

## Rule 6 — Known threat name or command (`known_threat_pattern`)

- **Trigger**:
  - name (lowercased) contains a known miner/rootkit name: `minerd`,
    `cpu_miner`, `xmrig`, `kdevtmpfsi`, `kinsing`, `diagree`, `watchbog`,
    `sysguard`, `crond64`, `mkfile`, `sysupdate`, `xmrig-nvidia`,
    `xmrig-amd`, `moneroocean`; or
  - the joined command line matches one of the compiled command regexes:
    the miner flags `--donate-level`, `--max-cpu-usage`, `--threads`, and
    the pool hosts `pool.monero`, `pool.supportxmr`, `mine.monero`.
- **Severity**: Critical.
- **Knobs**: name allow-list `KNOWN_THREAT_NAMES`; command patterns
  `KNOWN_THREAT_CMDS` are compiled once per analysis pass.

## Rule 7 — Pipe / download command line (`suspicious_pipe_or_download`)

- **Trigger**: the joined command line matches any of the compiled patterns:
  - `curl … | sh` / `| bash` / `| zsh` (and the `wget` equivalent),
  - a curl command ending in a bare `bash`/`sh` word,
  - `python3 -c` with `import`/`urllib`/`requests`/`socket`,
  - `base64 -d |`,
  - `eval $(curl …)` / `eval $(wget …)` and `bash -c … $(curl|wget)`.
- **Severity**: Critical.
- **Knobs**: pattern list `PIPE_PATTERNS` (compiled once per pass).

## Rule 8 — High thread count (`high_thread_anomaly`)

- **Trigger**: `thread_count >= 500` and the name is not in the allow-list:
  `chrome`, `firefox`, `code`, `Code`, `idea`, `java`, `dotnet`, `python`,
  `node`, `mysqld`, `postgres`, `Xorg`, `dockerd`.
- **Severity**: Critical when `thread_count > 1000` or `cpu_usage > 200.0`,
  else Warning.

## Rule 9 — High file descriptor count (`suspicious_fd_anomaly`)

- **Trigger**: `open_files >= 1000` and the name is not in the allow-list:
  `mysql`, `postgres`, `nginx`, `httpd`, `apache`, `chrome`, `firefox`,
  `code`, `java`, `dotnet`, `dockerd`.
- **Severity**: Info.

## Rule 10 — Spawn storm (`recent_spawn_storm`)

- **Trigger**: more than **5 distinct fresh PIDs** of the same process name
  within a rolling **120 second** window. Fresh means `run_time <= 120`;
  the window is `now − start_time < 120` (epoch seconds). The same PID seen
  again on later runs is counted once (`spawn_history` keyed by name).
- **Severity**: Warning; the message reports how many new instances were
  seen in the window.
- **Knobs**: window (120 s), threshold (> 5) and the freshness check are
  literals in `rule_spawn_storm`; history lives per-plugin across runs.
