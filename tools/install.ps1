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
    $DmdIp = Read-Host "IP address of your ArcadeMatrix DMD (ESP32 or Raspberry Pi)"
    if ([string]::IsNullOrWhiteSpace($DmdIp)) {
        Write-Error "ArcadeMatrix DMD IP address is required for installation. Aborting."
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
    $CleanRemotePath = $RemotePath.Trim("'")
    # Using SSH pipe instead of SCP avoids all OpenSSH SFTP legacy incompatibilities (e.g. on Batocera).
    # Strip CR (\r) remotely so Windows PowerShell string piping never writes CRLF into Linux scripts.
    $prevEncoding = $OutputEncoding
    try {
        $OutputEncoding = [System.Text.UTF8Encoding]::new($false)
        Get-Content -LiteralPath $LocalPath -Raw -Encoding UTF8 | & ssh @SshOpts "${User}@${TargetIp}" "tr -d '\r' > '$CleanRemotePath'"
        if ($LASTEXITCODE -ne 0) { throw "upload failed for $LocalPath" }
    } finally {
        $OutputEncoding = $prevEncoding
    }
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
    $daemonSrc = Get-Content -LiteralPath (Join-Path $ScriptDir "arcadematrix_daemon.py") -Raw -Encoding UTF8
    $daemonSrc = $daemonSrc.Replace("{{DMD_HOST}}", $DmdIp).Replace("{{BROKER}}", $DmdIp).Replace("{{TOPIC}}", $topic)
    $daemonLocal = Join-Path $TmpDir "arcadematrix_daemon.py"
    [System.IO.File]::WriteAllText($daemonLocal, $daemonSrc.Replace("`r`n", "`n"))

    $companionSrc = Get-Content -LiteralPath (Join-Path $ScriptDir "arcadematrix_companion_sync.py") -Raw -Encoding UTF8
    $companionSrc = $companionSrc.Replace("{{DMD_HOST}}", $DmdIp).Replace("{{BROKER}}", $DmdIp).Replace("{{TOPIC}}", $topic)
    $companionLocal = Join-Path $TmpDir "arcadematrix_companion_sync.py"
    [System.IO.File]::WriteAllText($companionLocal, $companionSrc.Replace("`r`n", "`n"))

    if ($system -eq "recalbox") {
        $TargetDir = "/recalbox/share/userscripts"
        $WakeupHook = "arcadematrix_wakeup[start,systembrowsing,rungame,endgame,rundemo,enddemo,wakeup].sh"
        Write-Host "Cleaning up previous install..."
        Invoke-RemoteCommand $activeUser "touch /tmp/arcadematrix_daemon.stop; if [ -f /tmp/arcadematrix_supervisor.pid ]; then kill -9 `$(cat /tmp/arcadematrix_supervisor.pid 2>/dev/null) 2>/dev/null || true; fi; if [ -f /tmp/arcadematrix_daemon.pid ]; then kill `$(cat /tmp/arcadematrix_daemon.pid 2>/dev/null) 2>/dev/null || true; fi; pkill -f '[a]rcadematrix_daemon.py' 2>/dev/null || true; sleep 0.2; pkill -9 -f '[a]rcadematrix_daemon.py' 2>/dev/null || true; pkill -f '[a]rcadematrix_companion_sync.py' 2>/dev/null || true; pkill -f '[a]rcadematrix_mqtt.sh' 2>/dev/null || true; rm -f /tmp/arcadematrix_daemon.pid /tmp/arcadematrix_supervisor.pid '$TargetDir/arcadematrix_mqtt.sh'; mkdir -p '$TargetDir' /recalbox/share/system" | Out-Null

        Write-Host "Uploading daemon, watchdog & ES self-healer (topic: $topic)..."
        Copy-ToRemote $activeUser $daemonLocal "/recalbox/share/arcadematrix_daemon.py"
        Copy-ToRemote $activeUser $companionLocal "/recalbox/share/arcadematrix_companion_sync.py"

        $launcherSrc = Get-Content -LiteralPath (Join-Path $ScriptDir "arcadematrix_launcher(permanent).sh") -Raw -Encoding UTF8
        $launcherLocal = Join-Path $TmpDir "arcadematrix_launcher(permanent).sh"
        [System.IO.File]::WriteAllText($launcherLocal, $launcherSrc.Replace("`r`n", "`n"))
        Copy-ToRemote $activeUser $launcherLocal "$TargetDir/arcadematrix_launcher(permanent).sh"

        $wakeupSrc = Get-Content -LiteralPath (Join-Path $ScriptDir $WakeupHook) -Raw -Encoding UTF8
        $wakeupLocal = Join-Path $TmpDir $WakeupHook
        [System.IO.File]::WriteAllText($wakeupLocal, $wakeupSrc.Replace("`r`n", "`n"))
        Copy-ToRemote $activeUser $wakeupLocal "$TargetDir/$WakeupHook"

        Write-Host "Configuring Recalbox boot persistence & starting daemon live (no reboot needed)..."
        $recalboxStartCmd = @'
chmod +x /recalbox/share/arcadematrix_companion_sync.py '/recalbox/share/userscripts/arcadematrix_launcher(permanent).sh' '/recalbox/share/userscripts/arcadematrix_wakeup[start,systembrowsing,rungame,endgame,rundemo,enddemo,wakeup].sh' 2>/dev/null || true
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
sleep 1
'@
        Invoke-RemoteCommand $activeUser ($recalboxStartCmd.Replace("`r`n", "`n")) | Out-Null

    } elseif ($system -eq "batocera") {
        $TargetDir = "/userdata/system/scripts"
        Write-Host "Cleaning up legacy daemons..."
        Invoke-RemoteCommand $activeUser "pkill -f '[a]rcadematrix_daemon.py' 2>/dev/null || true; pkill -f '[a]rcadematrix_companion_sync.py' 2>/dev/null || true; pkill -f '[a]rcadematrix_mqtt.sh' 2>/dev/null || true" | Out-Null
        Write-Host "Removing legacy script files..."
        Invoke-RemoteCommand $activeUser "rm -f /userdata/system/arcadematrix_daemon.py $TargetDir/arcadematrix_hook.sh $TargetDir/arcadematrix_mqtt.sh $TargetDir/arcadematrix_companion_sync.py" | Out-Null
        Write-Host "Removing legacy ES event directories..."
        Invoke-RemoteCommand $activeUser "rm -rf $TargetDir/game-selected $TargetDir/game-start $TargetDir/game-end $TargetDir/system-selected" | Out-Null
        Invoke-RemoteCommand $activeUser "rm -rf /userdata/system/configs/emulationstation/scripts/game-selected /userdata/system/configs/emulationstation/scripts/game-start /userdata/system/configs/emulationstation/scripts/game-end /userdata/system/configs/emulationstation/scripts/system-selected" | Out-Null
        Invoke-RemoteCommand $activeUser "if [ -f /userdata/system/custom.sh ]; then sed -i '/arcadematrix_daemon.py/d' /userdata/system/custom.sh; fi" | Out-Null
        Invoke-RemoteCommand $activeUser "mkdir -p $TargetDir" | Out-Null

        Write-Host "Preparing Batocera event hook (dynamic MQTT broker discovery via DMD $DmdIp)..."
        $batoceraTemplate = Join-Path $ScriptDir "arcadematrix_mqtt_batocera.sh"
        $batoceraHookLocal = Join-Path $TmpDir "arcadematrix_mqtt.sh"
        $batoceraHookSrc = (Get-Content -LiteralPath $batoceraTemplate -Raw -Encoding UTF8) `
            -replace '\{\{DMD_HOST\}\}', $DmdIp `
            -replace '\{\{BROKER\}\}', $DmdIp
        [System.IO.File]::WriteAllText($batoceraHookLocal, $batoceraHookSrc.Replace("`r`n", "`n"))

        Write-Host "Uploading hook & companion sync to $TargetDir..."
        Copy-ToRemote $activeUser $batoceraHookLocal "$TargetDir/arcadematrix_mqtt.sh"
        Copy-ToRemote $activeUser $companionLocal "$TargetDir/arcadematrix_companion_sync.py"
        Invoke-RemoteCommand $activeUser "chmod 755 $TargetDir/arcadematrix_mqtt.sh $TargetDir/arcadematrix_companion_sync.py" | Out-Null

        Write-Host "Verifying hook deployment..."
        $verifyOutput = & ssh @SshOpts "${activeUser}@${TargetIp}" "test -f $TargetDir/arcadematrix_mqtt.sh && wc -c < $TargetDir/arcadematrix_mqtt.sh && head -1 $TargetDir/arcadematrix_mqtt.sh" 2>$null
        if ([string]::IsNullOrWhiteSpace($verifyOutput)) {
            Write-Error "CRITICAL: Hook script was NOT written to the Batocera filesystem!"
            exit 1
        }
        Write-Host "  Hook verified: $verifyOutput"

        Write-Host "Configuring EmulationStation UI hooks (game-selected, system-selected)..."
        foreach ($evt in @("game-selected", "system-selected", "game-start", "game-end")) {
            $esCmd = "mkdir -p /userdata/system/configs/emulationstation/scripts/$evt && printf '#!/bin/sh\n/userdata/system/scripts/arcadematrix_mqtt.sh $evt `"`$@`"`\n' > /userdata/system/configs/emulationstation/scripts/$evt/arcadematrix_mqtt.sh && chmod 755 /userdata/system/configs/emulationstation/scripts/$evt/arcadematrix_mqtt.sh"
            Invoke-RemoteCommand $activeUser $esCmd | Out-Null
        }
        Invoke-RemoteCommand $activeUser "chmod -R 755 /userdata/system/configs/emulationstation/scripts" | Out-Null

        Write-Host "Syncing filesystem..."
        Invoke-RemoteCommand $activeUser "sync" | Out-Null

        Write-Host "Batocera one-shot event hooks successfully installed!"

    } elseif ($system -eq "retropie") {
        Write-Host "Installing for RetroPie..."
        Invoke-RemoteCommand $activeUser "touch /tmp/arcadematrix_daemon.stop; pkill -f '[a]rcadematrix_daemon.py' 2>/dev/null || true; sleep 0.2; pkill -9 -f '[a]rcadematrix_daemon.py' 2>/dev/null || true; pkill -f '[a]rcadematrix_companion_sync.py' 2>/dev/null || true; rm -f /tmp/arcadematrix_daemon.stop /tmp/arcadematrix_daemon.pid; mkdir -p /opt/retropie/configs/all" | Out-Null

        Write-Host "Uploading daemon & companion sync (topic: $topic)..."
        Copy-ToRemote $activeUser $daemonLocal "/opt/retropie/configs/all/arcadematrix_daemon.py"
        Copy-ToRemote $activeUser $companionLocal "/opt/retropie/configs/all/arcadematrix_companion_sync.py"
        Invoke-RemoteCommand $activeUser "chmod +x /opt/retropie/configs/all/arcadematrix_daemon.py /opt/retropie/configs/all/arcadematrix_companion_sync.py" | Out-Null

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
SystemId=$1
GamePath=
State=stopped
STATEEOF
EOF
fi
chmod +x /opt/retropie/configs/all/runcommand-onend.sh

if [ -f /opt/retropie/configs/all/autostart.sh ] && ! grep -q 'arcadematrix_daemon.py' /opt/retropie/configs/all/autostart.sh; then
    sed -i '/emulationstation/i python3 /opt/retropie/configs/all/arcadematrix_daemon.py --daemon --log-file /opt/retropie/configs/all/daemon.log' /opt/retropie/configs/all/autostart.sh
fi
: > /opt/retropie/configs/all/daemon.log
python3 /opt/retropie/configs/all/arcadematrix_daemon.py --daemon --log-file /opt/retropie/configs/all/daemon.log
sleep 1
'@
        Invoke-RemoteCommand $activeUser ($retroCmd.Replace("`r`n", "`n")) | Out-Null
    }

    Write-Host ""
    Write-Host "=============================================="
    Write-Host " Done! $system is configured."
    Write-Host " It dynamically queries DMD ${DmdIp} (/api/mqtt/broker-target)"
    Write-Host " and publishes game events on topic $topic."
    Write-Host "=============================================="
} finally {
    Remove-Item -LiteralPath $TmpDir -Recurse -Force -ErrorAction SilentlyContinue
}
