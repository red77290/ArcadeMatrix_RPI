use ssh2::Session;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetOS {
    Recalbox,
    Batocera,
    RetroPie,
}

impl TargetOS {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "recalbox" => Some(TargetOS::Recalbox),
            "batocera" => Some(TargetOS::Batocera),
            "retropie" | "retro-pie" => Some(TargetOS::RetroPie),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            TargetOS::Recalbox => "Recalbox",
            TargetOS::Batocera => "Batocera",
            TargetOS::RetroPie => "RetroPie",
        }
    }

    pub fn topic(&self) -> &'static str {
        match self {
            TargetOS::Recalbox => "system/playing/recalbox",
            TargetOS::Batocera => "system/playing/batocera",
            TargetOS::RetroPie => "system/playing/retropie",
        }
    }

    pub fn log_path(&self) -> &'static str {
        match self {
            TargetOS::Recalbox => "/recalbox/share/userscripts/daemon.log",
            TargetOS::Batocera => "/userdata/system/scripts/daemon.log",
            TargetOS::RetroPie => "/opt/retropie/configs/all/daemon.log",
        }
    }
}

fn detect_os_remotely(sess: &Session) -> Option<TargetOS> {
    let mut channel = sess.channel_session().ok()?;
    let cmd = "if [ -d /opt/retropie ]; then echo RETROPIE; elif [ -d /userdata/system ]; then echo BATOCERA; elif [ -d /recalbox/share ]; then echo RECALBOX; else echo UNKNOWN; fi";
    channel.exec(cmd).ok()?;
    let mut out = String::new();
    channel.read_to_string(&mut out).ok();
    channel.wait_close().ok();
    let trimmed = out.trim();
    if trimmed.contains("RETROPIE") {
        Some(TargetOS::RetroPie)
    } else if trimmed.contains("BATOCERA") {
        Some(TargetOS::Batocera)
    } else if trimmed.contains("RECALBOX") {
        Some(TargetOS::Recalbox)
    } else {
        None
    }
}

fn generate_companion_sync_code(matrix_ip: &str, topic: &str) -> String {
    include_str!("../../tools/arcadematrix_companion_sync.py")
        .replace('\r', "")
        .replace("{{DMD_HOST}}", matrix_ip)
        .replace("{{BROKER}}", matrix_ip)
        .replace("{{TOPIC}}", topic)
}

fn generate_daemon_code(matrix_ip: &str, topic: &str) -> String {
    include_str!("../../tools/arcadematrix_daemon.py")
        .replace('\r', "")
        .replace("{{DMD_HOST}}", matrix_ip)
        .replace("{{BROKER}}", matrix_ip)
        .replace("{{TOPIC}}", topic)
}

fn generate_launcher_code() -> String {
    include_str!("../../tools/arcadematrix_launcher(permanent).sh").replace('\r', "")
}

fn generate_recalbox_wakeup_code() -> String {
    include_str!(
        "../../tools/arcadematrix_wakeup[start,systembrowsing,rungame,endgame,rundemo,enddemo,wakeup].sh"
    )
    .replace('\r', "")
}

fn generate_batocera_hook_code(matrix_ip: &str) -> String {
    include_str!("../../tools/arcadematrix_mqtt_batocera.sh")
        .replace('\r', "")
        .replace("{{DMD_HOST}}", matrix_ip)
        .replace("{{BROKER}}", matrix_ip)
}

fn ssh_exec(sess: &Session, cmd: &str, step_label: &str) -> Result<(i32, String), String> {
    let mut channel = sess
        .channel_session()
        .map_err(|e| format!("SSH channel error ({}): {}", step_label, e))?;
    channel
        .exec(cmd)
        .map_err(|e| format!("SSH exec failed ({}): {}", step_label, e))?;
    let mut out = String::new();
    let _ = channel.read_to_string(&mut out);
    let mut err_out = String::new();
    let _ = channel.stderr().read_to_string(&mut err_out);
    channel.wait_close().ok();
    let status = channel.exit_status().unwrap_or(0);
    if !err_out.trim().is_empty() {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(err_out.trim());
    }
    Ok((status, out))
}

fn normalize_host_input(raw: &str) -> String {
    let mut s = raw.trim().trim_matches(|c| c == '"' || c == '\'').trim();
    if let Some(rest) = s
        .strip_prefix("http://")
        .or_else(|| s.strip_prefix("HTTP://"))
        .or_else(|| s.strip_prefix("https://"))
        .or_else(|| s.strip_prefix("HTTPS://"))
    {
        s = rest;
    }
    if let Some(before_slash) = s.split('/').next() {
        s = before_slash.trim();
    }
    if let Some(rest) = s.strip_prefix('[') {
        if let Some((ipv6_inner, after_bracket)) = rest.split_once(']') {
            let after = after_bracket.trim();
            if after.is_empty()
                || after
                    .strip_prefix(':')
                    .is_some_and(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
            {
                return ipv6_inner.trim().to_string();
            }
        }
    }
    if s.matches(':').count() == 1 {
        if let Some((host, port)) = s.rsplit_once(':') {
            if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) {
                return host.trim().to_string();
            }
        }
    }
    s.to_string()
}

fn validate_safe_host(host: &str, label: &str) -> Result<(), String> {
    if host.is_empty() {
        return Err(format!("{} is required.", label));
    }
    let valid = host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':'));
    if !valid {
        return Err(format!(
            "{} contains invalid characters: '{}'. Provide a valid IP address or hostname.",
            label, host
        ));
    }
    Ok(())
}

fn build_auth_candidates(
    explicit_os: Option<TargetOS>,
    custom_user: Option<String>,
    custom_pass: Option<String>,
) -> Vec<(&'static str, String, String)> {
    let mut auth_candidates: Vec<(&'static str, String, String)> = Vec::new();
    let u_opt = custom_user
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let p_opt = custom_pass.filter(|s| !s.is_empty());

    match (u_opt, p_opt) {
        (Some(u), Some(p)) => {
            auth_candidates.push(("Custom", u, p));
        }
        (Some(u), None) => match explicit_os {
            Some(TargetOS::RetroPie) => {
                auth_candidates.push((
                    "Custom User (RetroPie)",
                    u.clone(),
                    "raspberry".to_string(),
                ));
                auth_candidates.push(("Custom User (Root)", u, "root".to_string()));
            }
            Some(TargetOS::Batocera) => {
                auth_candidates.push(("Custom User (Batocera)", u, "linux".to_string()));
            }
            Some(TargetOS::Recalbox) => {
                auth_candidates.push(("Custom User (Recalbox)", u, "recalboxroot".to_string()));
            }
            None => {
                auth_candidates.push((
                    "Custom User (Recalbox)",
                    u.clone(),
                    "recalboxroot".to_string(),
                ));
                auth_candidates.push(("Custom User (Batocera)", u.clone(), "linux".to_string()));
                auth_candidates.push((
                    "Custom User (RetroPie)",
                    u.clone(),
                    "raspberry".to_string(),
                ));
                auth_candidates.push(("Custom User (Root)", u, "root".to_string()));
            }
        },
        (None, Some(p)) => match explicit_os {
            Some(TargetOS::RetroPie) => {
                auth_candidates.push(("Custom Pass (pi)", "pi".to_string(), p.clone()));
                auth_candidates.push(("Custom Pass (root)", "root".to_string(), p));
            }
            Some(TargetOS::Recalbox) | Some(TargetOS::Batocera) => {
                auth_candidates.push(("Custom Pass (root)", "root".to_string(), p));
            }
            None => {
                auth_candidates.push(("Custom Pass (root)", "root".to_string(), p.clone()));
                auth_candidates.push(("Custom Pass (pi)", "pi".to_string(), p));
            }
        },
        (None, None) => {}
    }

    match explicit_os {
        Some(TargetOS::RetroPie) => {
            auth_candidates.push(("RetroPie", "pi".to_string(), "raspberry".to_string()));
            auth_candidates.push(("RetroPie Root", "root".to_string(), "root".to_string()));
        }
        Some(TargetOS::Recalbox) => {
            auth_candidates.push(("Recalbox", "root".to_string(), "recalboxroot".to_string()));
        }
        Some(TargetOS::Batocera) => {
            auth_candidates.push(("Batocera", "root".to_string(), "linux".to_string()));
        }
        None => {
            auth_candidates.push(("Recalbox", "root".to_string(), "recalboxroot".to_string()));
            auth_candidates.push(("Batocera", "root".to_string(), "linux".to_string()));
            auth_candidates.push(("RetroPie", "pi".to_string(), "raspberry".to_string()));
            auth_candidates.push(("RetroPie Root", "root".to_string(), "root".to_string()));
        }
    }

    auth_candidates
}

const RECALBOX_START_CMD: &str = r#"
chmod 755 /recalbox/share/arcadematrix_daemon.py \
          /recalbox/share/arcadematrix_companion_sync.py \
          '/recalbox/share/userscripts/arcadematrix_launcher(permanent).sh' \
          '/recalbox/share/userscripts/arcadematrix_wakeup[start,systembrowsing,rungame,endgame,rundemo,enddemo,wakeup].sh' 2>/dev/null || true
_ra_ovr='/recalbox/share/roms/.retroarch.cfg'
touch "$_ra_ovr" 2>/dev/null || true
if [ -w "$_ra_ovr" ]; then
    for _kv in \
        'log_verbosity = "true"' \
        'log_to_file = "true"' \
        'log_to_file_timestamp = "false"' \
        'log_dir = "/tmp"' \
        'frontend_log_level = "1"' \
        'libretro_log_level = "2"'; do
        _k=${_kv%% *}
        if ! grep -Eq "^[[:space:]]*${_k}[[:space:]]*=" "$_ra_ovr" 2>/dev/null; then
            printf '%s\n' "$_kv" >> "$_ra_ovr"
        fi
    done
fi
if [ ! -f /recalbox/share/system/custom.sh ]; then
    printf '#!/bin/sh\ncase "$1" in\n  ""|start) sh "/recalbox/share/userscripts/arcadematrix_launcher(permanent).sh" -action start >/dev/null 2>&1 & ;;\nesac\n' > /recalbox/share/system/custom.sh
elif ! grep -q 'arcadematrix_launcher' /recalbox/share/system/custom.sh; then
    printf '\n# ArcadeMatrix Daemon\ncase "$1" in ""|start) sh "/recalbox/share/userscripts/arcadematrix_launcher(permanent).sh" -action start >/dev/null 2>&1 & ;; esac\n' >> /recalbox/share/system/custom.sh
fi
chmod +x /recalbox/share/system/custom.sh 2>/dev/null || true
rm -f /tmp/arcadematrix_daemon.stop
: > /recalbox/share/userscripts/daemon.log
sh '/recalbox/share/userscripts/arcadematrix_launcher(permanent).sh' -action start
sync
sleep 0.6
_dpid=$(cat /tmp/arcadematrix_daemon.pid 2>/dev/null || true)
[ -n "$_dpid" ] && kill -0 "$_dpid" 2>/dev/null
"#;

fn ssh_write_file(sess: &Session, remote_path: &str, content: &str) -> Result<(), String> {
    let escaped = remote_path.replace('\'', "'\\''");
    let mut channel = sess
        .channel_session()
        .map_err(|e| format!("SSH channel error writing {}: {}", remote_path, e))?;
    let write_cmd = format!(
        "cat > '{escaped}.amtmp.$$' && mv -f '{escaped}.amtmp.$$' '{escaped}'",
        escaped = escaped
    );
    channel
        .exec(&write_cmd)
        .map_err(|e| format!("SSH exec atomic write to {} failed: {}", remote_path, e))?;
    channel
        .write_all(content.as_bytes())
        .map_err(|e| format!("SSH write_all to {} failed: {}", remote_path, e))?;
    channel
        .send_eof()
        .map_err(|e| format!("SSH send_eof to {} failed: {}", remote_path, e))?;
    let mut out = String::new();
    let _ = channel.read_to_string(&mut out);
    let mut err_out = String::new();
    let _ = channel.stderr().read_to_string(&mut err_out);
    channel.wait_close().ok();
    let rc = channel.exit_status().unwrap_or(0);
    if rc != 0 {
        return Err(format!(
            "Failed writing remote file {} (rc={}): {}",
            remote_path,
            rc,
            err_out.trim()
        ));
    }
    Ok(())
}

pub fn install_sync_script(
    target_ip: &str,
    matrix_ip: &str,
    custom_user: Option<String>,
    custom_pass: Option<String>,
    target_os: Option<String>,
) -> Result<String, String> {
    let clean_target = normalize_host_input(target_ip);
    let clean_matrix = normalize_host_input(matrix_ip);
    validate_safe_host(&clean_target, "Target console IP address")?;
    validate_safe_host(&clean_matrix, "ArcadeMatrix IP address")?;

    let addr = (clean_target.as_str(), 22)
        .to_socket_addrs()
        .map_err(|e| format!("Invalid target IP or hostname '{}': {}", clean_target, e))?
        .next()
        .ok_or_else(|| format!("Could not resolve target host '{}'", clean_target))?;
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(8))
        .map_err(|e| format!("Failed to connect to {}: {}", clean_target, e))?;

    let mut sess = Session::new().map_err(|e| format!("SSH session error: {}", e))?;
    sess.set_tcp_stream(tcp);
    sess.handshake()
        .map_err(|e| format!("SSH handshake failed: {}", e))?;

    let explicit_os = target_os.as_deref().and_then(TargetOS::from_str);
    let auth_candidates = build_auth_candidates(explicit_os, custom_user, custom_pass);

    let mut authenticated = false;
    for (label, user, pwd) in &auth_candidates {
        tracing::info!(
            "Trying to connect to {} as {} ({})",
            clean_target,
            user,
            label
        );
        if sess.userauth_password(user, pwd).is_ok() {
            authenticated = true;
            break;
        }
    }

    if !authenticated {
        return Err(
            "Failed to authenticate via SSH. Check your credentials or target OS selection."
                .to_string(),
        );
    }

    let os = if let Some(resolved) = explicit_os {
        resolved
    } else {
        match detect_os_remotely(&sess) {
            Some(detected) => detected,
            None => {
                return Err("Failed to auto-detect gaming OS (checked /recalbox/share, /userdata/system, /opt/retropie). Please select OS explicitly.".to_string());
            }
        }
    };

    tracing::info!("Target OS identified as: {}", os.name());
    let daemon_code = generate_daemon_code(&clean_matrix, os.topic());
    let companion_code = generate_companion_sync_code(&clean_matrix, os.topic());

    match os {
        TargetOS::Recalbox => {
            tracing::info!("Installing for Recalbox (live start, no reboot required)...");
            let target_dir = "/recalbox/share/userscripts";
            let wakeup_hook =
                "arcadematrix_wakeup[start,systembrowsing,rungame,endgame,rundemo,enddemo,wakeup].sh";
            let cleanup_cmd = r#"
_pfx="/tmp/arcadematrix_"
touch "${_pfx}daemon.stop"
for _pf in "${_pfx}supervisor.pid" "${_pfx}daemon.pid"; do
    if [ -f "$_pf" ]; then
        _kp=$(cat "$_pf" 2>/dev/null || true)
        if [ -n "$_kp" ] && [ "$_kp" != "$$" ] && [ "$_kp" != "$PPID" ]; then
            kill -9 "$_kp" 2>/dev/null || true
        fi
    fi
done
for _pd in /proc/[0-9]*; do
    _p=${_pd#/proc/}
    [ "$_p" = "1" ] || [ "$_p" = "$$" ] || [ "$_p" = "$PPID" ] && continue
    [ -r "$_pd/cmdline" ] || continue
    _a0=$(tr "\0" "\n" < "$_pd/cmdline" 2>/dev/null | sed -n "1p")
    _cl=$(tr "\0" " " < "$_pd/cmdline" 2>/dev/null || true)
    case "${_a0##*/}" in
        python*)
            case "$_cl" in
                *arcadematrix_[d]aemon.py*|*arcadematrix_[c]ompanion_sync.py*)
                    kill -9 "$_p" 2>/dev/null || true
                    ;;
            esac
            ;;
        sh|ash|dash|bash)
            case "$_cl" in
                *arcadematrix_[l]auncher*--supervisor-loop*|*arcadematrix_[s]upervisor.pid*|*arcadematrix_[m]qtt.sh*)
                    kill -9 "$_p" 2>/dev/null || true
                    ;;
            esac
            ;;
    esac
done
rm -f "${_pfx}daemon.pid" "${_pfx}supervisor.pid" "${_pfx}daemon.lock" "/recalbox/share/userscripts/arcadematrix_"mqtt.sh || true
mkdir -p /recalbox/share/userscripts /recalbox/share/system /recalbox/share/roms
"#;
            ssh_exec(&sess, cleanup_cmd, "Recalbox cleanup")?;

            ssh_write_file(
                &sess,
                "/recalbox/share/arcadematrix_daemon.py",
                &daemon_code,
            )?;
            ssh_write_file(
                &sess,
                "/recalbox/share/arcadematrix_companion_sync.py",
                &companion_code,
            )?;
            ssh_write_file(
                &sess,
                &format!("{}/arcadematrix_launcher(permanent).sh", target_dir),
                &generate_launcher_code(),
            )?;
            ssh_write_file(
                &sess,
                &format!("{}/{}", target_dir, wakeup_hook),
                &generate_recalbox_wakeup_code(),
            )?;

            let (rc, out) = ssh_exec(&sess, RECALBOX_START_CMD, "Recalbox start")?;
            if rc != 0 {
                return Err(format!(
                    "Recalbox daemon failed to start (rc={}): {}",
                    rc,
                    out.trim()
                ));
            }
            Ok("Successfully installed! Recalbox daemon & watchdog are active immediately (no reboot required).".to_string())
        }

        TargetOS::Batocera => {
            tracing::info!("Installing for Batocera (live hooks, no reboot required)...");
            let hook_path = "/userdata/system/scripts/arcadematrix_mqtt.sh";
            let companion_path = "/userdata/system/scripts/arcadematrix_companion_sync.py";

            if let Ok((_, version_str)) = ssh_exec(
                &sess,
                "cat /usr/share/batocera/batocera.version 2>/dev/null || cat /etc/batocera-version 2>/dev/null || true",
                "Batocera version",
            ) {
                let trimmed = version_str.trim();
                if !trimmed.is_empty() {
                    tracing::info!("Detected Batocera version: {}", trimmed);
                    if let Some(first_tok) = trimmed.split_whitespace().next() {
                        if let Ok(ver_num) = first_tok.parse::<u32>() {
                            if ver_num < 33 {
                                tracing::warn!(
                                    "Batocera version {} is < 33. EmulationStation dynamic marquee browsing (game-selected) requires Batocera v33+. Game launch/stop events will still work.",
                                    ver_num
                                );
                            }
                        }
                    }
                }
            }

            ssh_exec(
                &sess,
                "pkill -f '[a]rcadematrix_daemon.py' 2>/dev/null; pkill -f '[a]rcadematrix_companion_sync.py' 2>/dev/null; pkill -f '[a]rcadematrix_mqtt.sh' 2>/dev/null; true",
                "Batocera cleanup kill",
            )?;
            ssh_exec(
                &sess,
                "rm -f /userdata/system/arcadematrix_daemon.py /userdata/system/scripts/arcadematrix_hook.sh /userdata/system/scripts/arcadematrix_mqtt.sh /userdata/system/scripts/arcadematrix_companion_sync.py",
                "Batocera cleanup files",
            )?;
            ssh_exec(
                &sess,
                "rm -rf /userdata/system/scripts/game-selected /userdata/system/scripts/game-start /userdata/system/scripts/game-end /userdata/system/scripts/system-selected",
                "Batocera cleanup script dirs",
            )?;
            ssh_exec(
                &sess,
                "rm -rf /userdata/system/configs/emulationstation/scripts/game-selected /userdata/system/configs/emulationstation/scripts/game-start /userdata/system/configs/emulationstation/scripts/game-end /userdata/system/configs/emulationstation/scripts/system-selected",
                "Batocera cleanup ES dirs",
            )?;
            ssh_exec(
                &sess,
                "if [ -f /userdata/system/custom.sh ]; then sed -i '/arcadematrix_daemon.py/d' /userdata/system/custom.sh; fi; mkdir -p /userdata/system/scripts",
                "Batocera mkdir scripts",
            )?;

            let hook_code = generate_batocera_hook_code(&clean_matrix);
            ssh_write_file(&sess, hook_path, &hook_code)?;
            ssh_write_file(&sess, companion_path, &companion_code)?;
            ssh_exec(
                &sess,
                &format!("chmod 755 {} {}", hook_path, companion_path),
                "Batocera chmod",
            )?;

            let (_, verify_out) = ssh_exec(
                &sess,
                &format!(
                    "test -f {} && wc -c < {} && head -1 {}",
                    hook_path, hook_path, hook_path
                ),
                "Batocera verify",
            )?;
            if verify_out.trim().is_empty() {
                return Err(format!(
                    "CRITICAL: Hook script {} was NOT written to the Batocera filesystem.",
                    hook_path
                ));
            }
            if !verify_out.contains("#!/bin/sh") {
                tracing::warn!(
                    "Batocera hook verify: file exists but header looks unexpected: {}",
                    verify_out.trim()
                );
            }

            // Install all 4 EmulationStation UI wrappers in individual Dropbear-safe SSH channels
            // (`game-start` and `game-end` are required for Batocera <= v32 and standalone ES ports).
            for evt in &["game-selected", "system-selected", "game-start", "game-end"] {
                let cmd = format!(
                    "mkdir -p /userdata/system/configs/emulationstation/scripts/{evt} && \
                     printf '#!/bin/sh\\n/userdata/system/scripts/arcadematrix_mqtt.sh {evt} \"$@\"\\n' \
                     > /userdata/system/configs/emulationstation/scripts/{evt}/arcadematrix_mqtt.sh && \
                     chmod 755 /userdata/system/configs/emulationstation/scripts/{evt}/arcadematrix_mqtt.sh",
                    evt = evt
                );
                ssh_exec(&sess, &cmd, &format!("Batocera ES wrapper {}", evt))?;
            }

            ssh_exec(
                &sess,
                "chmod -R 755 /userdata/system/configs/emulationstation/scripts",
                "Batocera chmod ES",
            )?;
            ssh_exec(&sess, "sync", "Batocera sync")?;

            Ok("Successfully installed! Batocera event hooks are active immediately (no reboot required).".to_string())
        }

        TargetOS::RetroPie => {
            tracing::info!(
                "Installing for RetroPie (ES browsing hooks + runcommand + watchdog)..."
            );
            let cleanup_cmd = r#"
_pfx="/tmp/arcadematrix_"
touch "${_pfx}daemon.stop"
for _pf in "${_pfx}supervisor.pid" "${_pfx}daemon.pid"; do
    if [ -f "$_pf" ]; then
        _kp=$(cat "$_pf" 2>/dev/null || true)
        if [ -n "$_kp" ] && [ "$_kp" != "$$" ] && [ "$_kp" != "$PPID" ]; then
            kill -9 "$_kp" 2>/dev/null || true
        fi
    fi
done
for _pd in /proc/[0-9]*; do
    _p=${_pd#/proc/}
    [ "$_p" = "1" ] || [ "$_p" = "$$" ] || [ "$_p" = "$PPID" ] && continue
    [ -r "$_pd/cmdline" ] || continue
    _a0=$(tr "\0" "\n" < "$_pd/cmdline" 2>/dev/null | sed -n "1p")
    _cl=$(tr "\0" " " < "$_pd/cmdline" 2>/dev/null || true)
    case "${_a0##*/}" in
        python*)
            case "$_cl" in
                *arcadematrix_[d]aemon.py*|*arcadematrix_[c]ompanion_sync.py*)
                    kill -9 "$_p" 2>/dev/null || true
                    ;;
            esac
            ;;
        sh|ash|dash|bash)
            case "$_cl" in
                *arcadematrix_[l]auncher*--supervisor-loop*|*arcadematrix_[s]upervisor.pid*)
                    kill -9 "$_p" 2>/dev/null || true
                    ;;
            esac
            ;;
    esac
done
rm -f "${_pfx}daemon.pid" "${_pfx}supervisor.pid" "${_pfx}daemon.lock" || true
mkdir -p /opt/retropie/configs/all/emulationstation/scripts/system-select \
         /opt/retropie/configs/all/emulationstation/scripts/game-select
"#;
            ssh_exec(&sess, cleanup_cmd, "RetroPie cleanup")?;

            ssh_write_file(
                &sess,
                "/opt/retropie/configs/all/arcadematrix_daemon.py",
                &daemon_code,
            )?;
            ssh_write_file(
                &sess,
                "/opt/retropie/configs/all/arcadematrix_companion_sync.py",
                &companion_code,
            )?;
            ssh_write_file(
                &sess,
                "/opt/retropie/configs/all/arcadematrix_launcher.sh",
                &generate_launcher_code(),
            )?;

            let retro_setup_cmd = r#"
chmod +x /opt/retropie/configs/all/arcadematrix_daemon.py \
         /opt/retropie/configs/all/arcadematrix_companion_sync.py \
         /opt/retropie/configs/all/arcadematrix_launcher.sh 2>/dev/null || true

cat > /opt/retropie/configs/all/emulationstation/scripts/system-select/arcadematrix_es.sh << 'ESEOF'
#!/bin/sh
[ -z "$1" ] || [ "$1" = "NULL" ] && exit 0
_tmp="/tmp/es_state.inf.tmp.$$"
printf "State=browsing\nAction=systembrowsing\nSystemId=%s\nGamePath=\nGame=\n" "$1" > "$_tmp" && mv -f "$_tmp" /tmp/es_state.inf
ESEOF
chmod 755 /opt/retropie/configs/all/emulationstation/scripts/system-select/arcadematrix_es.sh

cat > /opt/retropie/configs/all/emulationstation/scripts/game-select/arcadematrix_es.sh << 'ESEOF'
#!/bin/sh
[ -z "$1" ] || [ "$1" = "NULL" ] && exit 0
_tmp="/tmp/es_state.inf.tmp.$$"
if [ -z "$2" ] || [ "$2" = "NULL" ]; then
    printf "State=browsing\nAction=systembrowsing\nSystemId=%s\nGamePath=\nGame=\n" "$1" > "$_tmp" && mv -f "$_tmp" /tmp/es_state.inf
else
    printf "State=browsing\nAction=gamelistbrowsing\nSystemId=%s\nGamePath=%s\nGame=%s\n" "$1" "$2" "$3" > "$_tmp" && mv -f "$_tmp" /tmp/es_state.inf
fi
ESEOF
chmod 755 /opt/retropie/configs/all/emulationstation/scripts/game-select/arcadematrix_es.sh

if [ -d "$HOME/.emulationstation" ] && [ ! -L "$HOME/.emulationstation" ]; then
    mkdir -p "$HOME/.emulationstation/scripts/system-select" "$HOME/.emulationstation/scripts/game-select"
    cp -f /opt/retropie/configs/all/emulationstation/scripts/system-select/arcadematrix_es.sh "$HOME/.emulationstation/scripts/system-select/arcadematrix_es.sh"
    cp -f /opt/retropie/configs/all/emulationstation/scripts/game-select/arcadematrix_es.sh "$HOME/.emulationstation/scripts/game-select/arcadematrix_es.sh"
    chmod 755 "$HOME/.emulationstation/scripts/system-select/arcadematrix_es.sh" "$HOME/.emulationstation/scripts/game-select/arcadematrix_es.sh"
fi

touch /opt/retropie/configs/all/runcommand-onstart.sh
sed -i '/# ArcadeMatrix Runcommand Hook/,/EOF/d' /opt/retropie/configs/all/runcommand-onstart.sh 2>/dev/null || true
sed -i '/# ArcadeMatrix Runcommand Hook/,/# End ArcadeMatrix Hook/d' /opt/retropie/configs/all/runcommand-onstart.sh 2>/dev/null || true
cat >> /opt/retropie/configs/all/runcommand-onstart.sh << 'RCEOF'
# ArcadeMatrix Runcommand Hook
_am_tmp="/tmp/es_state.inf.tmp.$$"
printf "State=playing\nAction=rungame\nSystemId=%s\nGamePath=%s\n" "$1" "$3" > "$_am_tmp" && mv -f "$_am_tmp" /tmp/es_state.inf
# End ArcadeMatrix Hook
RCEOF
chmod +x /opt/retropie/configs/all/runcommand-onstart.sh

touch /opt/retropie/configs/all/runcommand-onend.sh
sed -i '/# ArcadeMatrix Runcommand Hook/,/EOF/d' /opt/retropie/configs/all/runcommand-onend.sh 2>/dev/null || true
sed -i '/# ArcadeMatrix Runcommand Hook/,/# End ArcadeMatrix Hook/d' /opt/retropie/configs/all/runcommand-onend.sh 2>/dev/null || true
cat >> /opt/retropie/configs/all/runcommand-onend.sh << 'RCEOF'
# ArcadeMatrix Runcommand Hook
_am_tmp="/tmp/es_state.inf.tmp.$$"
printf "State=stopped\nAction=endgame\nSystemId=\nGamePath=\n" > "$_am_tmp" && mv -f "$_am_tmp" /tmp/es_state.inf
# End ArcadeMatrix Hook
RCEOF
chmod +x /opt/retropie/configs/all/runcommand-onend.sh

if [ -f /opt/retropie/configs/all/autostart.sh ]; then
    sed -i '/arcadematrix_daemon\.py/d;/arcadematrix_launcher\.sh/d' /opt/retropie/configs/all/autostart.sh 2>/dev/null || true
    if grep -q 'emulationstation' /opt/retropie/configs/all/autostart.sh; then
        sed -i '/emulationstation/i sh /opt/retropie/configs/all/arcadematrix_launcher.sh -action start >/dev/null 2>&1 &' /opt/retropie/configs/all/autostart.sh
    else
        printf '\nsh /opt/retropie/configs/all/arcadematrix_launcher.sh -action start >/dev/null 2>&1 &\n' >> /opt/retropie/configs/all/autostart.sh
    fi
fi

rm -f /tmp/arcadematrix_daemon.stop
: > /opt/retropie/configs/all/daemon.log
sh /opt/retropie/configs/all/arcadematrix_launcher.sh -action start
sync
sleep 0.6
_dpid=$(cat /tmp/arcadematrix_daemon.pid 2>/dev/null || true)
[ -n "$_dpid" ] && kill -0 "$_dpid" 2>/dev/null
"#;
            let (rc, out) = ssh_exec(&sess, retro_setup_cmd, "RetroPie setup")?;
            if rc != 0 {
                return Err(format!(
                    "RetroPie daemon failed to start (rc={}): {}",
                    rc,
                    out.trim()
                ));
            }
            Ok(
                "Successfully installed! RetroPie daemon, ES browsing hooks & watchdog are active."
                    .to_string(),
            )
        }
    }
}

pub fn fetch_sync_logs(
    target_ip: &str,
    custom_user: Option<String>,
    custom_pass: Option<String>,
    target_os: Option<String>,
) -> Result<String, String> {
    let clean_target = normalize_host_input(target_ip);
    validate_safe_host(&clean_target, "Target console IP address")?;

    let addr = (clean_target.as_str(), 22)
        .to_socket_addrs()
        .map_err(|e| format!("Invalid target address '{}': {}", clean_target, e))?
        .next()
        .ok_or_else(|| format!("Could not resolve target address '{}'", clean_target))?;
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(8))
        .map_err(|e| format!("Failed to connect to {}: {}", clean_target, e))?;

    let mut sess = Session::new().map_err(|e| format!("SSH session error: {}", e))?;
    sess.set_tcp_stream(tcp);
    sess.handshake()
        .map_err(|e| format!("SSH handshake failed: {}", e))?;

    let explicit_os = target_os.as_deref().and_then(TargetOS::from_str);
    let auth_candidates = build_auth_candidates(explicit_os, custom_user, custom_pass);

    let mut authenticated = false;
    for (label, user, pwd) in &auth_candidates {
        tracing::info!(
            "Trying to connect to {} as {} ({})",
            clean_target,
            user,
            label
        );
        if sess.userauth_password(user, pwd).is_ok() {
            authenticated = true;
            break;
        }
    }

    if !authenticated {
        return Err(
            "Failed to authenticate via SSH. Check credentials or OS selection.".to_string(),
        );
    }

    let os = if let Some(resolved) = explicit_os {
        resolved
    } else {
        detect_os_remotely(&sess).unwrap_or(TargetOS::Recalbox)
    };

    let log_path = os.log_path();
    let (_, logs) = ssh_exec(
        &sess,
        &format!(
            "tail -n 100 {} 2>/dev/null || echo 'Log file not found or empty'",
            log_path
        ),
        "fetch logs",
    )?;

    if logs.trim().is_empty() {
        Ok("Log file is empty.".to_string())
    } else {
        Ok(logs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_and_validate_host_input() {
        assert_eq!(normalize_host_input("  192.168.1.130  "), "192.168.1.130");
        assert_eq!(
            normalize_host_input("http://192.168.1.26:8080/api"),
            "192.168.1.26"
        );
        assert_eq!(
            normalize_host_input("\"https://recalbox.local:22/\""),
            "recalbox.local"
        );
        assert_eq!(normalize_host_input("[fe80::1]:22"), "fe80::1");
        assert_eq!(normalize_host_input("fe80::1"), "fe80::1");

        assert!(validate_safe_host("192.168.1.130", "Target").is_ok());
        assert!(validate_safe_host("recalbox-01.local", "Target").is_ok());
        assert!(validate_safe_host("fe80::1", "Target").is_ok());
        assert!(validate_safe_host("", "Target").is_err());
        assert!(validate_safe_host("192.168.1.1;rm -rf /", "Target").is_err());
    }

    #[test]
    fn test_build_auth_candidates_supports_partial_custom_credentials() {
        let pass_only = build_auth_candidates(
            Some(TargetOS::Recalbox),
            Some("   ".to_string()),
            Some("mysecret".to_string()),
        );
        assert_eq!(pass_only[0].1, "root");
        assert_eq!(pass_only[0].2, "mysecret");

        let user_only = build_auth_candidates(
            Some(TargetOS::Batocera),
            Some("arcade".to_string()),
            Some(String::new()),
        );
        assert_eq!(user_only[0].1, "arcade");
        assert_eq!(user_only[0].2, "linux");

        let both =
            build_auth_candidates(None, Some("admin".to_string()), Some("pw123".to_string()));
        assert_eq!(both[0].1, "admin");
        assert_eq!(both[0].2, "pw123");
    }

    #[test]
    fn test_recalbox_start_cmd_configures_ram_retroarch_rcheevos_logging() {
        assert!(RECALBOX_START_CMD.contains("/recalbox/share/roms/.retroarch.cfg"));
        assert!(RECALBOX_START_CMD.contains("log_dir = \"/tmp\""));
        assert!(RECALBOX_START_CMD.contains("log_to_file = \"true\""));
        assert!(RECALBOX_START_CMD.contains("log_to_file_timestamp = \"false\""));
    }

    #[test]
    fn test_generated_recalbox_daemon_uses_dynamic_broker_target_and_persistent_qos1() {
        let code = generate_daemon_code("192.168.1.42", "system/playing/recalbox");
        assert!(code.contains("DMD_HOST = \"192.168.1.42\""));
        assert!(code.contains("TOPIC = \"system/playing/recalbox\""));
        assert!(!code.contains("{{DMD_HOST}}"));
        assert!(!code.contains("{{TOPIC}}"));
        assert!(code.contains("/api/mqtt/broker-target"));
        assert!(code.contains("class PersistentMqttClient"));
        assert!(code.contains("def spawn_companion_sync"));
        assert!(!code.contains('\r'));
    }

    #[test]
    fn test_generated_batocera_hook_preserves_socket_hygiene_and_4_es_hooks() {
        let hook = generate_batocera_hook_code("192.168.1.42");
        assert!(hook.starts_with("#!/bin/sh\n"));
        assert!(hook.contains("DMD_HOST=\"192.168.1.42\""));
        assert!(hook.contains("/api/mqtt/broker-target"));
        assert!(hook.contains("resolve_broker_target()"));
        assert!(hook.contains("spawn_companion_sync()"));
        assert!(hook.contains("should_emit_lifecycle()"));
        assert!(hook.contains("game-start)"));
        assert!(hook.contains("game-end)"));
        assert!(!hook.contains("-q 1"));
        assert!(hook.contains("build_game_payload"));
        assert!(!hook.contains('\r'));
    }

    #[test]
    fn test_generated_launcher_and_wakeup_scripts_support_recalbox_and_retropie() {
        let launcher = generate_launcher_code();
        assert!(launcher.starts_with("#!/bin/sh\n"));
        assert!(launcher.contains("/opt/retropie/configs/all"));
        assert!(launcher.contains("/recalbox/share"));
        assert!(launcher.contains("--supervisor-loop"));

        let wakeup = generate_recalbox_wakeup_code();
        assert!(wakeup.starts_with("#!/bin/sh\n"));
        assert!(wakeup.contains("arcadematrix_launcher(permanent).sh"));
    }

    #[test]
    fn test_generated_companion_sync_code_substitutes_dmd_host_and_topic() {
        let comp = generate_companion_sync_code("192.168.1.42", "system/playing/batocera");
        assert!(comp.contains("DMD_HOST_DEFAULT = \"192.168.1.42\""));
        assert!(comp.contains("TOPIC_DEFAULT = \"system/playing/batocera\""));
        assert!(!comp.contains('\r'));
        assert!(comp.contains("def extract_hiscores"));
        assert!(comp.contains("def parse_hi_binary"));
        assert!(comp.contains("def _decode_namco_tile_hi"));
        assert!(comp.contains("def _decode_dkong_hi"));
        assert!(comp.contains("def parse_rcheevos_unlock_line"));
        assert!(comp.contains("API_GetUserRecentlyPlayedGames.php"));
    }
}
