<#
.SYNOPSIS
    ArcadeMatrix Gaming OS daemon installer - run this on your Windows PC, NOT on the
    console itself.

.DESCRIPTION
    Connects over SSH, allows selecting target OS (Recalbox, Batocera, RetroPie) or auto-detecting,
    uploads the matching daemon/hook scripts (with your ArcadeMatrix DMD IP and topic baked in),
    and starts/configures the target live so it publishes game events over MQTT to system/playing/#.

.NOTES
    Requires Windows OpenSSH Client (ssh.exe) on PATH. Check with: Get-Command ssh
#>

param(
    [string]$TargetIp = "",
    [string]$Action = "",
    [string]$DmdIp = "",
    [string]$OsChoice = "",
    [string]$CustomUser = ""
)

$ErrorActionPreference = "Stop"

function Write-Fatal {
    param([string]$Message)
    Write-Host ""
    Write-Host "ERROR: $Message" -ForegroundColor Red
    exit 1
}

function Normalize-HostInput {
    param([string]$RawValue)
    if ($null -eq $RawValue) { return "" }
    $val = $RawValue.Trim().Trim('"').Trim("'")
    $val = $val -replace '^https?://', ''
    $slashIdx = $val.IndexOf('/')
    if ($slashIdx -ge 0) {
        $val = $val.Substring(0, $slashIdx)
    }
    # Strip :port for IPv4/hostnames (preserve raw IPv6 if colons > 1)
    if (($val.Split(':').Count -eq 2) -and ($val -match '^([^:]+):\d+$')) {
        $val = $Matches[1]
    }
    return $val.Trim()
}

function Assert-SafeHost {
    param([string]$HostValue, [string]$Label)
    if ([string]::IsNullOrWhiteSpace($HostValue)) {
        Write-Fatal "$Label is required. Aborting."
    }
    if ($HostValue -notmatch '^[A-Za-z0-9\[\]][A-Za-z0-9.\-_:\[\]]*$') {
        Write-Fatal "$Label contains invalid characters: '$HostValue'. Provide a valid IP address or hostname."
    }
}

function Get-TemplateBase64 {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [hashtable]$Replacements = @{}
    )
    if (-not [System.IO.File]::Exists($FilePath)) {
        Write-Fatal "Required template file not found: $FilePath"
    }
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    $text = [System.IO.File]::ReadAllText($FilePath, $utf8NoBom)
    if ($text.Length -gt 0 -and [int][char]$text[0] -eq 0xFEFF) {
        $text = $text.Substring(1)
    }
    foreach ($key in $Replacements.Keys) {
        $text = $text.Replace([string]$key, [string]$Replacements[$key])
    }
    # Strictly normalize CRLF / CR to LF before encoding so Linux shebangs and scripts never contain \r
    $text = $text.Replace("`r`n", "`n").Replace("`r", "`n")
    $bytes = $utf8NoBom.GetBytes($text)
    $b64 = [Convert]::ToBase64String($bytes, [Base64FormattingOptions]::InsertLineBreaks)
    return $b64.Replace("`r`n", "`n").Replace("`r", "`n")
}

Write-Host "=============================================="
Write-Host " ArcadeMatrix Gaming OS Daemon Installer"
Write-Host " (Recalbox / Batocera / RetroPie)"
Write-Host "=============================================="
Write-Host ""

if (-not (Get-Command ssh -ErrorAction SilentlyContinue)) {
    Write-Fatal "ssh.exe not found on PATH. Install the Windows OpenSSH Client: Settings > Apps > Optional Features > Add a feature > OpenSSH Client."
}

if ([string]::IsNullOrWhiteSpace($TargetIp)) {
    $TargetIp = Read-Host "IP address of your Console (Recalbox/Batocera/RetroPie)"
}
$TargetIp = Normalize-HostInput $TargetIp
Assert-SafeHost -HostValue $TargetIp -Label "Target Console IP address"

if ([string]::IsNullOrWhiteSpace($Action)) {
    $Action = Read-Host "Action (1: Install Daemon, 2: Check Logs) [1]"
}
if ([string]::IsNullOrWhiteSpace($Action)) { $Action = "1" }
$Action = $Action.Trim()
if ($Action -ne "1" -and $Action -ne "2") {
    Write-Fatal "Invalid action '$Action'. Expected 1 (Install Daemon) or 2 (Check Logs)."
}

if ($Action -eq "1") {
    if ([string]::IsNullOrWhiteSpace($DmdIp)) {
        $DmdIp = Read-Host "IP address of your ArcadeMatrix DMD (ESP32 or Raspberry Pi)"
    }
    $DmdIp = Normalize-HostInput $DmdIp
    Assert-SafeHost -HostValue $DmdIp -Label "ArcadeMatrix DMD IP address"
}

if ([string]::IsNullOrWhiteSpace($OsChoice)) {
    Write-Host ""
    Write-Host "Select Target Console OS:"
    Write-Host "  1) Auto-Detect"
    Write-Host "  2) Recalbox"
    Write-Host "  3) Batocera"
    Write-Host "  4) RetroPie"
    $OsChoice = Read-Host "Choice [1]"
}
if ([string]::IsNullOrWhiteSpace($OsChoice)) { $OsChoice = "1" }
$OsChoice = $OsChoice.Trim()
if (@("1", "2", "3", "4") -notcontains $OsChoice) {
    Write-Fatal "Invalid OS choice '$OsChoice'. Expected 1, 2, 3, or 4."
}

if ((-not $PSBoundParameters.ContainsKey("CustomUser")) -and (-not $PSBoundParameters.ContainsKey("TargetIp"))) {
    $CustomUser = Read-Host "Custom SSH Username (leave blank for defaults)"
}
$CustomUser = $CustomUser.Trim()
if ((-not [string]::IsNullOrWhiteSpace($CustomUser)) -and ($CustomUser -notmatch '^[A-Za-z0-9_][A-Za-z0-9._\-]*$')) {
    Write-Fatal "Custom SSH Username contains invalid characters: '$CustomUser'."
}

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition

# Runs a POSIX shell script on the remote host via stdin ("tr -d '\r' | sh -s").
# Uses an isolated temporary ~/.ssh/am_kh_* file instead of OpenSSH 'none' hostfiles (which force
# readonly=RDONLY in sshconnect.c and trigger "Host key verification failed" exit 255 on new hosts),
# and avoids space-in-path Win32-OpenSSH bugs by leveraging native ~/ expansion inside ssh.exe.
function Invoke-RemoteShell {
    param(
        [Parameter(Mandatory = $true)][string]$User,
        [Parameter(Mandatory = $true)][string]$ScriptContent
    )
    $prevEap = $ErrorActionPreference
    $prevOutEnc = $OutputEncoding
    $khToken = "am_kh_${PID}_$([Guid]::NewGuid().ToString('N').Substring(0, 8))"
    $cleanupPaths = New-Object System.Collections.Generic.List[string]
    foreach ($baseProfile in @($env:USERPROFILE, $HOME)) {
        if (-not [string]::IsNullOrWhiteSpace($baseProfile)) {
            $sshDir = Join-Path $baseProfile ".ssh"
            try {
                if (-not [System.IO.Directory]::Exists($sshDir)) {
                    [System.IO.Directory]::CreateDirectory($sshDir) | Out-Null
                }
                $uKh = Join-Path $sshDir $khToken
                $gKh = Join-Path $sshDir "${khToken}_g"
                [System.IO.File]::WriteAllBytes($uKh, [byte[]]@())
                [System.IO.File]::WriteAllBytes($gKh, [byte[]]@())
                [void]$cleanupPaths.Add($uKh)
                [void]$cleanupPaths.Add("${uKh}.old")
                [void]$cleanupPaths.Add($gKh)
                [void]$cleanupPaths.Add("${gKh}.old")
            } catch {}
        }
    }
    $sshOpts = @(
        "-T",
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=~/.ssh/${khToken}",
        "-o", "GlobalKnownHostsFile=~/.ssh/${khToken}_g",
        "-o", "CheckHostIP=no",
        "-o", "LogLevel=ERROR",
        "-o", "ConnectTimeout=8"
    )
    try {
        $ErrorActionPreference = "Continue"
        $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
        $OutputEncoding = $utf8NoBom
        $payload = $ScriptContent.Replace("`r`n", "`n").Replace("`r", "`n")
        if (-not $payload.EndsWith("`n")) {
            $payload += "`n"
        }
        $payload | & ssh @sshOpts -- "${User}@${TargetIp}" "tr -d '\r' | sh -s" | Out-Host
        $rc = $LASTEXITCODE
        if ($null -eq $rc) { $rc = 1 }
        return [int]$rc
    } finally {
        $OutputEncoding = $prevOutEnc
        $ErrorActionPreference = $prevEap
        foreach ($f in $cleanupPaths) {
            try {
                if ([System.IO.File]::Exists($f)) {
                    Remove-Item -LiteralPath $f -Force -ErrorAction SilentlyContinue
                }
            } catch {}
        }
    }
}

$requestedSystem = switch ($OsChoice) {
    "2" { "recalbox" }
    "3" { "batocera" }
    "4" { "retropie" }
    default { "auto" }
}

# ==============================================================================
# ACTION 2: CHECK LOGS (single SSH connection)
# ==============================================================================
if ($Action -eq "2") {
    $logScript = @'
REQ_SYS="__REQ_SYS__"
if [ "$REQ_SYS" = "auto" ]; then
    if [ -d /recalbox/share ]; then
        SYS="recalbox"
    elif [ -d /userdata/system ]; then
        SYS="batocera"
    elif [ -d /opt/retropie ]; then
        SYS="retropie"
    else
        exit 42
    fi
else
    SYS="$REQ_SYS"
fi
case "$SYS" in
    recalbox) LOG_PATH="/recalbox/share/userscripts/daemon.log" ;;
    batocera) LOG_PATH="/userdata/system/scripts/daemon.log" ;;
    retropie) LOG_PATH="/opt/retropie/configs/all/daemon.log" ;;
    *) exit 42 ;;
esac
echo ""
echo "=============================================="
echo " Detected: $SYS - Fetching logs from $LOG_PATH..."
echo "=============================================="
tail -n 100 "$LOG_PATH" 2>/dev/null || echo "Log file not found or empty"
'@
    $logScript = $logScript.Replace("__REQ_SYS__", $requestedSystem)

    if (-not [string]::IsNullOrWhiteSpace($CustomUser)) {
        $rc = Invoke-RemoteShell -User $CustomUser -ScriptContent $logScript
    } elseif ($requestedSystem -eq "retropie") {
        Write-Host "Connecting to ${TargetIp} as 'pi' (default password: 'raspberry')..."
        $rc = Invoke-RemoteShell -User "pi" -ScriptContent $logScript
    } elseif ($requestedSystem -eq "auto") {
        Write-Host "Connecting to ${TargetIp} as 'root' (Recalbox: 'recalboxroot', Batocera: 'linux')..."
        $rc = Invoke-RemoteShell -User "root" -ScriptContent $logScript
        if ($rc -ne 0) {
            Write-Host "Root check did not match Recalbox/Batocera; trying 'pi' for RetroPie (password: 'raspberry')..."
            $rc = Invoke-RemoteShell -User "pi" -ScriptContent $logScript
        }
    } else {
        $hint = if ($requestedSystem -eq "recalbox") { "recalboxroot" } else { "linux" }
        Write-Host "Connecting to ${TargetIp} as 'root' (default password: '$hint')..."
        $rc = Invoke-RemoteShell -User "root" -ScriptContent $logScript
    }

    if ($rc -ne 0) {
        Write-Fatal "Could not fetch logs from $TargetIp (SSH exit code $rc). Check IP, SSH settings, and credentials."
    }
    exit 0
}

# ==============================================================================
# ACTION 1: INSTALL DAEMON / HOOKS (single SSH session per target user)
# ==============================================================================
$WakeupHookName = "arcadematrix_wakeup[start,systembrowsing,rungame,endgame,rundemo,enddemo,wakeup].sh"

function Build-InstallPayload {
    param([string]$Mode)
    # $Mode: "recalbox", "batocera", "retropie", or "auto"

    $needRecalbox = ($Mode -eq "recalbox" -or $Mode -eq "auto")
    $needBatocera = ($Mode -eq "batocera" -or $Mode -eq "auto")
    $needRetropie = ($Mode -eq "retropie" -or $Mode -eq "auto")

    $sb = New-Object System.Text.StringBuilder

    [void]$sb.AppendLine('set -u')
    [void]$sb.AppendLine("DMD_IP='$DmdIp'")
    [void]$sb.AppendLine("REQ_SYS='$Mode'")
    [void]$sb.AppendLine(@'
if [ "$REQ_SYS" = "auto" ]; then
    if [ -d /recalbox/share ]; then
        SYSTEM="recalbox"
    elif [ -d /userdata/system ]; then
        SYSTEM="batocera"
    elif [ -d /opt/retropie ]; then
        SYSTEM="retropie"
    else
        exit 42
    fi
else
    SYSTEM="$REQ_SYS"
fi

TOPIC="system/playing/$SYSTEM"
echo "Console identified as: $SYSTEM"
echo "Checking DMD target ($DMD_IP) from console..."
_bt=""
for _hp in 80 8080; do
    _bt=$(curl -fsS -m 2 "http://$DMD_IP:$_hp/api/mqtt/broker-target" 2>/dev/null || wget -qO- -T 2 "http://$DMD_IP:$_hp/api/mqtt/broker-target" 2>/dev/null || true)
    [ -n "$_bt" ] && break
done
if [ -n "$_bt" ]; then
    echo "  DMD /api/mqtt/broker-target -> $_bt"
else
    echo "  WARNING: Could not reach http://$DMD_IP[:8080]/api/mqtt/broker-target from console!"
fi

am_write_b64() {
    _dst="$1"
    _tmp="${_dst}.amtmp.$$"
    if command -v base64 >/dev/null 2>&1; then
        base64 -d > "$_tmp" || { rm -f "$_tmp" 2>/dev/null || true; return 1; }
    else
        python3 -c 'import base64,sys; sys.stdout.buffer.write(base64.b64decode(sys.stdin.read()))' > "$_tmp" || { rm -f "$_tmp" 2>/dev/null || true; return 1; }
    fi
    mv -f "$_tmp" "$_dst"
}
'@)

    if ($needRecalbox) {
        $b64DaemonRb = Get-TemplateBase64 -FilePath (Join-Path $ScriptDir "arcadematrix_daemon.py") -Replacements @{
            "{{DMD_HOST}}" = $DmdIp
            "{{BROKER}}"   = $DmdIp
            "{{TOPIC}}"    = "system/playing/recalbox"
        }
        $b64CompRb = Get-TemplateBase64 -FilePath (Join-Path $ScriptDir "arcadematrix_companion_sync.py") -Replacements @{
            "{{DMD_HOST}}" = $DmdIp
            "{{BROKER}}"   = $DmdIp
            "{{TOPIC}}"    = "system/playing/recalbox"
        }
        $b64Launcher = Get-TemplateBase64 -FilePath (Join-Path $ScriptDir "arcadematrix_launcher(permanent).sh")
        $b64Wakeup = Get-TemplateBase64 -FilePath (Join-Path $ScriptDir $WakeupHookName)

        [void]$sb.AppendLine(@'
if [ "$SYSTEM" = "recalbox" ]; then
    TARGET_DIR="/recalbox/share/userscripts"
    WAKEUP_HOOK="arcadematrix_wakeup[start,systembrowsing,rungame,endgame,rundemo,enddemo,wakeup].sh"
    echo "Cleaning up any previous Recalbox install..."
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
        _a0=$(tr '\0' '\n' < "$_pd/cmdline" 2>/dev/null | sed -n '1p')
        _cl=$(tr '\0' ' ' < "$_pd/cmdline" 2>/dev/null || true)
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
    mkdir -p /recalbox/share/userscripts /recalbox/share/system /recalbox/share/roms || { echo "ERROR: Failed to create Recalbox directories!" >&2; exit 1; }

    echo "Uploading daemon, companion sync, watchdog & ES self-healer (topic: $TOPIC)..."
    am_write_b64 "/recalbox/share/arcadematrix_daemon.py" << 'AM_B64_RB_DAEMON' || { echo "ERROR: Failed writing arcadematrix_daemon.py!" >&2; exit 1; }
'@)
        [void]$sb.AppendLine($b64DaemonRb)
        [void]$sb.AppendLine('AM_B64_RB_DAEMON')

        [void]$sb.AppendLine(@'
    am_write_b64 "/recalbox/share/arcadematrix_companion_sync.py" << 'AM_B64_RB_COMP' || { echo "ERROR: Failed writing arcadematrix_companion_sync.py!" >&2; exit 1; }
'@)
        [void]$sb.AppendLine($b64CompRb)
        [void]$sb.AppendLine('AM_B64_RB_COMP')

        [void]$sb.AppendLine(@'
    am_write_b64 "$TARGET_DIR/arcadematrix_launcher(permanent).sh" << 'AM_B64_RB_LAUNCH' || { echo "ERROR: Failed writing arcadematrix_launcher(permanent).sh!" >&2; exit 1; }
'@)
        [void]$sb.AppendLine($b64Launcher)
        [void]$sb.AppendLine('AM_B64_RB_LAUNCH')

        [void]$sb.AppendLine(@'
    am_write_b64 "$TARGET_DIR/$WAKEUP_HOOK" << 'AM_B64_RB_WAKE' || { echo "ERROR: Failed writing $WAKEUP_HOOK!" >&2; exit 1; }
'@)
        [void]$sb.AppendLine($b64Wakeup)
        [void]$sb.AppendLine('AM_B64_RB_WAKE')

        [void]$sb.AppendLine(@'
    echo "Configuring Recalbox boot persistence, RAM RetroArch RCHEEVOS logging & starting daemon live..."
    chmod 755 /recalbox/share/arcadematrix_daemon.py /recalbox/share/arcadematrix_companion_sync.py "$TARGET_DIR/arcadematrix_launcher(permanent).sh" "$TARGET_DIR/$WAKEUP_HOOK" 2>/dev/null || true
    _ra_ovr="/recalbox/share/roms/.retroarch.cfg"
    touch "$_ra_ovr" 2>/dev/null || true
    if [ -w "$_ra_ovr" ]; then
        for _kv in \
            'log_verbosity = "true"' \
            'log_to_file = "true"' \
            'log_to_file_timestamp = "false"' \
            'log_dir = "/tmp"' \
            'frontend_log_level = "1"' \
            'libretro_log_level = "2"'; do
            _k="${_kv%% *}"
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
    : > "$TARGET_DIR/daemon.log"
    sh "$TARGET_DIR/arcadematrix_launcher(permanent).sh" -action start
    sync
    sleep 1
    echo "Active Recalbox processes:"
    _dpid=$(cat /tmp/arcadematrix_daemon.pid 2>/dev/null || true)
    _spid=$(cat /tmp/arcadematrix_supervisor.pid 2>/dev/null || true)
    if [ -n "$_dpid" ] && kill -0 "$_dpid" 2>/dev/null; then
        echo "  Daemon PID $_dpid: $(tr '\0' ' ' < "/proc/$_dpid/cmdline" 2>/dev/null)"
    else
        echo "  ERROR: Daemon PID not running on Recalbox!" >&2
        cat "$TARGET_DIR/daemon.log" 2>/dev/null || true
        exit 1
    fi
    if [ -n "$_spid" ] && kill -0 "$_spid" 2>/dev/null; then
        echo "  Supervisor PID $_spid: $(tr '\0' ' ' < "/proc/$_spid/cmdline" 2>/dev/null)"
    fi
    echo "Daemon log:"
    cat "$TARGET_DIR/daemon.log" 2>/dev/null || true
    echo ""
    echo "=============================================="
    echo " Done! recalbox is configured (active immediately)."
    echo " DMD target: $DMD_IP (/api/mqtt/broker-target)"
    echo " MQTT topic: $TOPIC"
    echo "=============================================="
    exit 0
fi
'@)
    }

    if ($needBatocera) {
        $b64BatoHook = Get-TemplateBase64 -FilePath (Join-Path $ScriptDir "arcadematrix_mqtt_batocera.sh") -Replacements @{
            "{{DMD_HOST}}" = $DmdIp
            "{{BROKER}}"   = $DmdIp
        }
        $b64CompBato = Get-TemplateBase64 -FilePath (Join-Path $ScriptDir "arcadematrix_companion_sync.py") -Replacements @{
            "{{DMD_HOST}}" = $DmdIp
            "{{BROKER}}"   = $DmdIp
            "{{TOPIC}}"    = "system/playing/batocera"
        }

        [void]$sb.AppendLine(@'
if [ "$SYSTEM" = "batocera" ]; then
    TARGET_DIR="/userdata/system/scripts"
    echo "Cleaning up legacy Batocera daemons & scripts..."
    for _pd in /proc/[0-9]*; do
        _p=${_pd#/proc/}
        [ "$_p" = "1" ] || [ "$_p" = "$$" ] || [ "$_p" = "$PPID" ] && continue
        [ -r "$_pd/cmdline" ] || continue
        _a0=$(tr '\0' '\n' < "$_pd/cmdline" 2>/dev/null | sed -n '1p')
        _a1=$(tr '\0' '\n' < "$_pd/cmdline" 2>/dev/null | sed -n '2p')
        case "${_a0##*/}" in
            python*)
                case "$_a1" in
                    *arcadematrix_daemon.py|*arcadematrix_companion_sync.py)
                        kill -9 "$_p" 2>/dev/null || true
                        ;;
                esac
                ;;
            sh|ash|dash|bash)
                case "$_a1" in
                    *arcadematrix_mqtt.sh)
                        kill -9 "$_p" 2>/dev/null || true
                        ;;
                esac
                ;;
        esac
    done
    rm -f /userdata/system/arcadematrix_daemon.py "$TARGET_DIR/arcadematrix_hook.sh" "$TARGET_DIR/arcadematrix_mqtt.sh" "$TARGET_DIR/arcadematrix_companion_sync.py"
    rm -rf "$TARGET_DIR/game-selected" "$TARGET_DIR/game-start" "$TARGET_DIR/game-end" "$TARGET_DIR/system-selected"
    rm -rf /userdata/system/configs/emulationstation/scripts/game-selected \
           /userdata/system/configs/emulationstation/scripts/game-start \
           /userdata/system/configs/emulationstation/scripts/game-end \
           /userdata/system/configs/emulationstation/scripts/system-selected
    if [ -f /userdata/system/custom.sh ]; then
        sed -i '/arcadematrix_daemon.py/d' /userdata/system/custom.sh 2>/dev/null || true
    fi
    mkdir -p "$TARGET_DIR" || { echo "ERROR: Failed to create $TARGET_DIR!" >&2; exit 1; }

    echo "Uploading Batocera event hook & companion sync to $TARGET_DIR..."
    am_write_b64 "$TARGET_DIR/arcadematrix_mqtt.sh" << 'AM_B64_BATO_HOOK' || { echo "ERROR: Failed writing arcadematrix_mqtt.sh!" >&2; exit 1; }
'@)
        [void]$sb.AppendLine($b64BatoHook)
        [void]$sb.AppendLine('AM_B64_BATO_HOOK')

        [void]$sb.AppendLine(@'
    am_write_b64 "$TARGET_DIR/arcadematrix_companion_sync.py" << 'AM_B64_BATO_COMP' || { echo "ERROR: Failed writing arcadematrix_companion_sync.py!" >&2; exit 1; }
'@)
        [void]$sb.AppendLine($b64CompBato)
        [void]$sb.AppendLine('AM_B64_BATO_COMP')

        [void]$sb.AppendLine(@'
    chmod 755 "$TARGET_DIR/arcadematrix_mqtt.sh" "$TARGET_DIR/arcadematrix_companion_sync.py" || exit 1

    echo "Verifying hook deployment..."
    if [ ! -s "$TARGET_DIR/arcadematrix_mqtt.sh" ]; then
        echo "ERROR: Hook script was NOT written to the Batocera filesystem!" >&2
        exit 1
    fi
    _sz=$(wc -c < "$TARGET_DIR/arcadematrix_mqtt.sh" 2>/dev/null || echo 0)
    _hd=$(head -n 1 "$TARGET_DIR/arcadematrix_mqtt.sh" 2>/dev/null || true)
    echo "  Hook verified: ${_sz} bytes (${_hd})"

    echo "Configuring EmulationStation UI hooks (game-selected, system-selected, game-start, game-end)..."
    for evt in game-selected system-selected game-start game-end; do
        _hdir="/userdata/system/configs/emulationstation/scripts/$evt"
        mkdir -p "$_hdir" || exit 1
        printf '#!/bin/sh\n/userdata/system/scripts/arcadematrix_mqtt.sh %s "$@"\n' "$evt" > "$_hdir/arcadematrix_mqtt.sh" || exit 1
        chmod 755 "$_hdir/arcadematrix_mqtt.sh" || exit 1
    done
    chmod -R 755 /userdata/system/configs/emulationstation/scripts 2>/dev/null || true
    sync
    echo ""
    echo "=============================================="
    echo " Done! batocera one-shot hooks installed (active immediately)."
    echo " DMD target: $DMD_IP (/api/mqtt/broker-target)"
    echo " MQTT topic: $TOPIC"
    echo "=============================================="
    exit 0
fi
'@)
    }

    if ($needRetropie) {
        $b64DaemonRp = Get-TemplateBase64 -FilePath (Join-Path $ScriptDir "arcadematrix_daemon.py") -Replacements @{
            "{{DMD_HOST}}" = $DmdIp
            "{{BROKER}}"   = $DmdIp
            "{{TOPIC}}"    = "system/playing/retropie"
        }
        $b64CompRp = Get-TemplateBase64 -FilePath (Join-Path $ScriptDir "arcadematrix_companion_sync.py") -Replacements @{
            "{{DMD_HOST}}" = $DmdIp
            "{{BROKER}}"   = $DmdIp
            "{{TOPIC}}"    = "system/playing/retropie"
        }
        $b64LauncherRp = Get-TemplateBase64 -FilePath (Join-Path $ScriptDir "arcadematrix_launcher(permanent).sh")

        [void]$sb.AppendLine(@'
if [ "$SYSTEM" = "retropie" ]; then
    echo "Cleaning up any previous RetroPie install..."
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
        _a0=$(tr '\0' '\n' < "$_pd/cmdline" 2>/dev/null | sed -n '1p')
        _cl=$(tr '\0' ' ' < "$_pd/cmdline" 2>/dev/null || true)
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
             /opt/retropie/configs/all/emulationstation/scripts/game-select || { echo "ERROR: Failed to create RetroPie directories!" >&2; exit 1; }

    echo "Uploading daemon, watchdog & companion sync (topic: $TOPIC)..."
    am_write_b64 "/opt/retropie/configs/all/arcadematrix_daemon.py" << 'AM_B64_RP_DAEMON' || { echo "ERROR: Failed writing arcadematrix_daemon.py!" >&2; exit 1; }
'@)
        [void]$sb.AppendLine($b64DaemonRp)
        [void]$sb.AppendLine('AM_B64_RP_DAEMON')

        [void]$sb.AppendLine(@'
    am_write_b64 "/opt/retropie/configs/all/arcadematrix_companion_sync.py" << 'AM_B64_RP_COMP' || { echo "ERROR: Failed writing arcadematrix_companion_sync.py!" >&2; exit 1; }
'@)
        [void]$sb.AppendLine($b64CompRp)
        [void]$sb.AppendLine('AM_B64_RP_COMP')

        [void]$sb.AppendLine(@'
    am_write_b64 "/opt/retropie/configs/all/arcadematrix_launcher.sh" << 'AM_B64_RP_LAUNCH' || { echo "ERROR: Failed writing arcadematrix_launcher.sh!" >&2; exit 1; }
'@)
        [void]$sb.AppendLine($b64LauncherRp)
        [void]$sb.AppendLine('AM_B64_RP_LAUNCH')

        [void]$sb.AppendLine(@'
    echo "Configuring RetroPie EmulationStation browsing hooks, runcommand hooks & watchdog..."
    chmod 755 /opt/retropie/configs/all/arcadematrix_daemon.py \
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
    sed -i '/# ArcadeMatrix Runcommand Hook/,/STATEEOF/d' /opt/retropie/configs/all/runcommand-onstart.sh 2>/dev/null || true
    sed -i '/# ArcadeMatrix Runcommand Hook/,/# End ArcadeMatrix Hook/d' /opt/retropie/configs/all/runcommand-onstart.sh 2>/dev/null || true
    cat >> /opt/retropie/configs/all/runcommand-onstart.sh << 'RCEOF'
# ArcadeMatrix Runcommand Hook
_am_tmp="/tmp/es_state.inf.tmp.$$"
printf "State=playing\nAction=rungame\nSystemId=%s\nGamePath=%s\n" "$1" "$3" > "$_am_tmp" && mv -f "$_am_tmp" /tmp/es_state.inf
# End ArcadeMatrix Hook
RCEOF
    chmod +x /opt/retropie/configs/all/runcommand-onstart.sh

    touch /opt/retropie/configs/all/runcommand-onend.sh
    sed -i '/# ArcadeMatrix Runcommand Hook/,/STATEEOF/d' /opt/retropie/configs/all/runcommand-onend.sh 2>/dev/null || true
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
    sleep 1
    _dpid=$(cat /tmp/arcadematrix_daemon.pid 2>/dev/null || true)
    if [ -n "$_dpid" ] && kill -0 "$_dpid" 2>/dev/null; then
        echo "  RetroPie Daemon PID $_dpid active."
    else
        echo "  ERROR: RetroPie Daemon PID not running!" >&2
        cat /opt/retropie/configs/all/daemon.log 2>/dev/null || true
        exit 1
    fi
    echo ""
    echo "=============================================="
    echo " Done! retropie is configured (active immediately)."
    echo " DMD target: $DMD_IP (/api/mqtt/broker-target)"
    echo " MQTT topic: $TOPIC"
    echo "=============================================="
    exit 0
fi
'@)
    }

    [void]$sb.AppendLine('exit 42')
    return $sb.ToString()
}

Write-Host ""
if (-not [string]::IsNullOrWhiteSpace($CustomUser)) {
    Write-Host "Connecting to ${TargetIp} as '${CustomUser}' (single SSH session)..."
    $payload = Build-InstallPayload -Mode $requestedSystem
    $rc = Invoke-RemoteShell -User $CustomUser -ScriptContent $payload
} elseif ($requestedSystem -eq "retropie") {
    Write-Host "Connecting to ${TargetIp} as 'pi' (default password: 'raspberry')..."
    $payload = Build-InstallPayload -Mode "retropie"
    $rc = Invoke-RemoteShell -User "pi" -ScriptContent $payload
} elseif ($requestedSystem -eq "auto") {
    Write-Host "Auto-detecting & installing on ${TargetIp} as 'root' (Recalbox: 'recalboxroot', Batocera: 'linux')..."
    $payload = Build-InstallPayload -Mode "auto"
    $rc = Invoke-RemoteShell -User "root" -ScriptContent $payload
    if ($rc -ne 0) {
        Write-Host ""
        Write-Host "Root session did not complete (exit $rc). Trying 'pi' for RetroPie (default password: 'raspberry')..." -ForegroundColor Yellow
        $rpPayload = Build-InstallPayload -Mode "retropie"
        $rc = Invoke-RemoteShell -User "pi" -ScriptContent $rpPayload
    }
} else {
    $hint = if ($requestedSystem -eq "recalbox") { "recalboxroot" } else { "linux" }
    Write-Host "Connecting to ${TargetIp} as 'root' (default password: '$hint')..."
    $payload = Build-InstallPayload -Mode $requestedSystem
    $rc = Invoke-RemoteShell -User "root" -ScriptContent $payload
}

if ($rc -eq 42) {
    Write-Fatal "Connected to $TargetIp, but none of /recalbox/share, /userdata/system, or /opt/retropie were found."
} elseif ($rc -ne 0) {
    Write-Fatal "Installation failed on $TargetIp (exit code $rc). Verify IP address, SSH access, password, and console logs."
}
