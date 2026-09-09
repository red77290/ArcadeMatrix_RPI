<#
.SYNOPSIS
    ArcadeMatrix Gaming OS daemon installer - run this on your Windows PC, NOT on the
    console itself.

.DESCRIPTION
    Connects over SSH, allows selecting target OS (Recalbox, Batocera, RetroPie) or auto-detecting,
    uploads the right daemon/hook script (with your ArcadeMatrix device's IP and topic baked in),
    and starts/reboots the target so it starts sending game events over MQTT to system/playing/#.

.NOTES
    Requires ssh.exe and scp.exe to be on PATH. Check with: Get-Command ssh
#>

$ErrorActionPreference = "Stop"

Write-Host "=============================================="
Write-Host " ArcadeMatrix Gaming OS Daemon Installer"
Write-Host " (Recalbox / Batocera / RetroPie)"
Write-Host "=============================================="
Write-Host ""

if (-not (Get-Command ssh -ErrorAction SilentlyContinue)) {
    Write-Error "ssh.exe not found on PATH. Install the Windows OpenSSH Client: Settings > Apps > Optional Features > Add a feature > OpenSSH Client."
    exit 1
}

$TargetIp = Read-Host "IP address of your Console (Recalbox/Batocera/RetroPie)"
$Action = Read-Host "Action (1: Install Daemon, 2: Check Logs) [1]"
if ([string]::IsNullOrWhiteSpace($Action)) { $Action = "1" }

if ($Action -eq "1") {
    $BrokerIp = Read-Host "IP address of your ArcadeMatrix device (ESP32 or Raspberry Pi)"
    if ([string]::IsNullOrWhiteSpace($BrokerIp)) {
        Write-Error "Broker IP address is required for installation. Aborting."
        exit 1
    }
}

if ([string]::IsNullOrWhiteSpace($TargetIp)) {
    Write-Error "Target IP address is required. Aborting."
    exit 1
}

Write-Host ""
Write-Host "Select Target Console OS:"
Write-Host "  1) Auto-Detect"
Write-Host "  2) Recalbox"
Write-Host "  3) Batocera"
Write-Host "  4) RetroPie"
$OsChoice = Read-Host "Choice [1]"
if ([string]::IsNullOrWhiteSpace($OsChoice)) { $OsChoice = "1" }

$CustomUser = Read-Host "Custom SSH Username (leave blank for defaults)"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$SshOpts = @("-o", "StrictHostKeyChecking=no", "-o", "UserKnownHostsFile=NUL", "-o", "ConnectTimeout=5")

function Invoke-RemoteCommand {
    param([string]$User, [string]$Command)
    & ssh @SshOpts "${User}@${TargetIp}" $Command | Out-Null
    return $LASTEXITCODE
}

function Copy-ToRemote {
    param([string]$User, [string]$LocalPath, [string]$RemotePath)
    & scp @SshOpts $LocalPath "${User}@${TargetIp}:${RemotePath}"
    if ($LASTEXITCODE -ne 0) { throw "scp failed uploading $LocalPath" }
}

$system = "unknown"
$activeUser = "root"

if ($OsChoice -eq "2") {
    $system = "recalbox"
    $activeUser = if (-not [string]::IsNullOrWhiteSpace($CustomUser)) { $CustomUser } else { "root" }
} elseif ($OsChoice -eq "3") {
    $system = "batocera"
    $activeUser = if (-not [string]::IsNullOrWhiteSpace($CustomUser)) { $CustomUser } else { "root" }
} elseif ($OsChoice -eq "4") {
    $system = "retropie"
    $activeUser = if (-not [string]::IsNullOrWhiteSpace($CustomUser)) { $CustomUser } else { "pi" }
} else {
    Write-Host "Auto-detecting OS on $TargetIp..."
    $testUser = if (-not [string]::IsNullOrWhiteSpace($CustomUser)) { $CustomUser } else { "root" }
    if ((Invoke-RemoteCommand $testUser "test -d /recalbox/share") -eq 0) {
        $system = "recalbox"
        $activeUser = $testUser
    } elseif ((Invoke-RemoteCommand $testUser "test -d /userdata/system") -eq 0) {
        $system = "batocera"
        $activeUser = $testUser
    } else {
        $piUser = if (-not [string]::IsNullOrWhiteSpace($CustomUser)) { $CustomUser } else { "pi" }
        if ((Invoke-RemoteCommand $piUser "test -d /opt/retropie") -eq 0) {
            $system = "retropie"
            $activeUser = $piUser
        }
    }
}

if ($system -eq "unknown") {
    Write-Error "Could not detect Console OS on $TargetIp. Check IP, SSH settings, and password."
    exit 1
}

Write-Host "Detected: $system (User: $activeUser)"

if ($Action -eq "2") {
    $LogPath = switch ($system) {
        "recalbox" { "/recalbox/share/userscripts/daemon.log" }
        "batocera" { "/userdata/system/scripts/daemon.log" }
        "retropie" { "/opt/retropie/configs/all/daemon.log" }
    }
    Write-Host ""
    Write-Host "=============================================="
    Write-Host " Fetching logs from $LogPath..."
    Write-Host "=============================================="
    & ssh @SshOpts "${activeUser}@${TargetIp}" "tail -n 100 $LogPath 2>/dev/null || echo 'Log file not found or empty'"
    exit 0
}

$topic = "system/playing/$system"
$TmpDir = Join-Path $env:TEMP "arcadematrix_installer_$(Get-Random)"
New-Item -ItemType Directory -Path $TmpDir | Out-Null

try {
    $daemonSrc = Get-Content (Join-Path $ScriptDir "arcadematrix_daemon.py") -Raw
    $daemonSrc = $daemonSrc.Replace("{{BROKER}}", $BrokerIp).Replace("{{TOPIC}}", $topic)
    $daemonLocal = Join-Path $TmpDir "arcadematrix_daemon.py"
    [System.IO.File]::WriteAllText($daemonLocal, $daemonSrc.Replace("`r`n", "`n"))

    if ($system -eq "recalbox") {
        $TargetDir = "/recalbox/share/userscripts"
        Write-Host "Cleaning up previous install..."
        Invoke-RemoteCommand $activeUser "pkill -f arcadematrix_daemon.py || true; pkill -f arcadematrix_mqtt.sh || true; rm -f $TargetDir/arcadematrix_mqtt.sh" | Out-Null
        Invoke-RemoteCommand $activeUser "mkdir -p $TargetDir" | Out-Null

        Write-Host "Uploading daemon (topic: $topic)..."
        Copy-ToRemote $activeUser $daemonLocal "/recalbox/share/arcadematrix_daemon.py"

        $launcherSrc = Get-Content (Join-Path $ScriptDir "arcadematrix_launcher(permanent).sh") -Raw
        $launcherLocal = Join-Path $TmpDir "arcadematrix_launcher(permanent).sh"
        [System.IO.File]::WriteAllText($launcherLocal, $launcherSrc.Replace("`r`n", "`n"))
        Copy-ToRemote $activeUser $launcherLocal "'$TargetDir/arcadematrix_launcher(permanent).sh'"

        Invoke-RemoteCommand $activeUser "chmod +x '$TargetDir/arcadematrix_launcher(permanent).sh'" | Out-Null

        Write-Host "Rebooting $TargetIp to apply changes..."
        Invoke-RemoteCommand $activeUser "sleep 1 && reboot" | Out-Null

    } elseif ($system -eq "batocera") {
        $TargetDir = "/userdata/system/scripts"
        Write-Host "Cleaning up previous install..."
        Invoke-RemoteCommand $activeUser "pkill -f arcadematrix_daemon.py || true; pkill -f arcadematrix_mqtt.sh || true; rm -f $TargetDir/arcadematrix_mqtt.sh" | Out-Null
        Invoke-RemoteCommand $activeUser "mkdir -p $TargetDir /userdata/system/configs/emulationstation/scripts" | Out-Null

        Write-Host "Uploading daemon (topic: $topic)..."
        Copy-ToRemote $activeUser $daemonLocal "/userdata/system/arcadematrix_daemon.py"

        Write-Host "Installing Batocera event hooks..."
        $hookCmd = @'
cat > /userdata/system/scripts/arcadematrix_hook.sh << 'EOF'
#!/bin/sh
EVENT="$1"
[ -z "$EVENT" ] && EVENT="$(basename "$0")"
SYSTEM="$2"
ROMPATH="$3"
case "$(basename "$0")" in
    game-start|game_start|gameStart) EVENT="game-start"; SYSTEM="$1"; ROMPATH="$2" ;;
    game-end|game_end|gameStop)     EVENT="game-end"; SYSTEM="$1"; ROMPATH="$2" ;;
    game-selected)                   EVENT="game-selected"; SYSTEM="$1"; ROMPATH="$2" ;;
    system-selected)                 EVENT="system-selected"; SYSTEM="$1" ;;
esac
case "$EVENT" in
    game-selected) STATE="browsing" ;;
    game-start)    STATE="playing" ;;
    game-end)      STATE="stopped" ;;
    system-selected) STATE="browsing"; ROMPATH="" ;;
    *)             STATE="browsing" ;;
esac
cat > /tmp/es_state.inf << STATEEOF
SystemId=$SYSTEM
GamePath=$ROMPATH
State=$STATE
STATEEOF
EOF
chmod +x /userdata/system/scripts/arcadematrix_hook.sh
for evt in game-selected game-start game-end system-selected; do
    ln -sf /userdata/system/scripts/arcadematrix_hook.sh /userdata/system/scripts/$evt
    ln -sf /userdata/system/scripts/arcadematrix_hook.sh /userdata/system/configs/emulationstation/scripts/$evt
done
'@
        Invoke-RemoteCommand $activeUser $hookCmd | Out-Null

        $cmd = 'if [ ! -f /userdata/system/custom.sh ]; then echo "#!/bin/sh" > /userdata/system/custom.sh; echo ''[ "$1" = "start" ] && python3 /userdata/system/arcadematrix_daemon.py > /userdata/system/scripts/daemon.log 2>&1 &'' >> /userdata/system/custom.sh; chmod +x /userdata/system/custom.sh; else if ! grep -q "arcadematrix_daemon.py" /userdata/system/custom.sh; then echo ''[ "$1" = "start" ] && python3 /userdata/system/arcadematrix_daemon.py > /userdata/system/scripts/daemon.log 2>&1 &'' >> /userdata/system/custom.sh; fi; fi'
        Invoke-RemoteCommand $activeUser $cmd | Out-Null

        Write-Host "Rebooting $TargetIp to apply changes..."
        Invoke-RemoteCommand $activeUser "sleep 1 && reboot" | Out-Null

    } elseif ($system -eq "retropie") {
        Write-Host "Installing for RetroPie..."
        Invoke-RemoteCommand $activeUser "pkill -f arcadematrix_daemon.py || true; mkdir -p /opt/retropie/configs/all" | Out-Null

        Write-Host "Uploading daemon (topic: $topic)..."
        Copy-ToRemote $activeUser $daemonLocal "/opt/retropie/configs/all/arcadematrix_daemon.py"
        Invoke-RemoteCommand $activeUser "chmod +x /opt/retropie/configs/all/arcadematrix_daemon.py" | Out-Null

        $retroCmd = @'
touch /opt/retropie/configs/all/runcommand-onstart.sh
if ! grep -q 'ArcadeMatrix Runcommand Hook' /opt/retropie/configs/all/runcommand-onstart.sh; then
    cat >> /opt/retropie/configs/all/runcommand-onstart.sh << 'EOF'
# ArcadeMatrix Runcommand Hook
cat > /tmp/es_state.inf << STATEEOF
SystemId=$1
GamePath=$3
State=playing
STATEEOF
EOF
fi
chmod +x /opt/retropie/configs/all/runcommand-onstart.sh

touch /opt/retropie/configs/all/runcommand-onend.sh
if ! grep -q 'ArcadeMatrix Runcommand Hook' /opt/retropie/configs/all/runcommand-onend.sh; then
    cat >> /opt/retropie/configs/all/runcommand-onend.sh << 'EOF'
# ArcadeMatrix Runcommand Hook
cat > /tmp/es_state.inf << STATEEOF
SystemId=
GamePath=
State=stopped
STATEEOF
EOF
fi
chmod +x /opt/retropie/configs/all/runcommand-onend.sh

if [ -f /opt/retropie/configs/all/autostart.sh ] && ! grep -q 'arcadematrix_daemon.py' /opt/retropie/configs/all/autostart.sh; then
    sed -i '/emulationstation/i python3 /opt/retropie/configs/all/arcadematrix_daemon.py > /opt/retropie/configs/all/daemon.log 2>&1 &' /opt/retropie/configs/all/autostart.sh
fi
nohup python3 /opt/retropie/configs/all/arcadematrix_daemon.py > /opt/retropie/configs/all/daemon.log 2>&1 &
'@
        Invoke-RemoteCommand $activeUser $retroCmd | Out-Null
    }

    Write-Host ""
    Write-Host "=============================================="
    Write-Host " Done! $system is configured."
    Write-Host " It publishes game events to MQTT broker ${BrokerIp}:1883 on topic"
    Write-Host " $topic."
    Write-Host "=============================================="
} finally {
    Remove-Item -Recurse -Force $TmpDir -ErrorAction SilentlyContinue
}
