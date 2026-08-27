# ArcadeMatrix Smart Deploy (Windows PowerShell)
# Auto-detects the Raspberry Pi architecture, builds via Docker, and deploys.

param(
    [string]$Ip = "",
    [string]$User = "",
    [string]$Pass = "",
    [switch]$SkipBuild,
    [string]$BinaryPath = ""
)

$PI_IP = "192.168.1.149"
$PI_USER = "pi"
$PI_PASS = "raspberry"

if (Test-Path -Path "scripts/defaults.sh") {
    Get-Content "scripts/defaults.sh" | ForEach-Object {
        if ($_ -match 'AM_USER="?([^"]+)"?') { $PI_USER = $matches[1] }
        if ($_ -match 'AM_PASS="?([^"]+)"?') { $PI_PASS = $matches[1] }
    }
}

if ($Ip) { $PI_IP = $Ip }
if ($User) { $PI_USER = $User }
if ($Pass) { $PI_PASS = $Pass }

$REMOTE_USER_IP = "$PI_USER@$PI_IP"
$BIN_PATH = $BinaryPath

Write-Host "=========================================================="
Write-Host "      ArcadeMatrix Smart Deploy to Raspberry Pi"
Write-Host "=========================================================="
Write-Host "[Info] Connecting to $REMOTE_USER_IP to detect OS architecture..."

$REMOTE_ARCH = ssh -o StrictHostKeyChecking=no $REMOTE_USER_IP "uname -m"
$REMOTE_ARCH = $REMOTE_ARCH.Trim()

if ($REMOTE_ARCH -eq "aarch64") {
    Write-Host "[OK] Detected 64-bit OS (aarch64) on Raspberry Pi."
    $DEFAULT_BIN_PATH = "target/aarch64-unknown-linux-gnu/release/arcadematrix"
} elseif ($REMOTE_ARCH -match "^armv") {
    Write-Host "[OK] Detected 32-bit OS ($REMOTE_ARCH) on Raspberry Pi."
    $DEFAULT_BIN_PATH = "target/armv7-unknown-linux-gnueabihf/release/arcadematrix"
} else {
    Write-Host "[Error] Unknown architecture: $REMOTE_ARCH"
    exit 1
}

if ($BIN_PATH -eq "") {
    $BIN_PATH = $DEFAULT_BIN_PATH
}

if ($BinaryPath -ne "") {
    Write-Host "[Info] Using custom binary path: $BIN_PATH"
} elseif ($SkipBuild) {
    Write-Host "[Info] SkipBuild is set. Skipping Docker build..."
} else {
    Write-Host "[Step 1] Building binary for $REMOTE_ARCH using Docker..."
    if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
        Write-Host "[Warning] Docker is not installed. Attempting to deploy existing binary if available..."
        Write-Host "If this fails, you can specify a pre-compiled binary with: .\deploy.ps1 -BinaryPath 'path/to/arcadematrix'"
    } else {
        bash scripts/build.sh "$REMOTE_ARCH"
    }
}

if (-not (Test-Path -Path $BIN_PATH)) {
    Write-Host "[Error] Compiled binary not found at $BIN_PATH"
    Write-Host "Please compile the project, or download the pre-compiled binary from GitHub Actions and provide it via -BinaryPath"
    exit 1
}

function Invoke-RemoteBash {
    param([string]$Command)
    $Bytes = [System.Text.Encoding]::UTF8.GetBytes($Command)
    $B64 = [Convert]::ToBase64String($Bytes)
    ssh -o StrictHostKeyChecking=no $REMOTE_USER_IP ('echo {0} | base64 -d | bash' -f $B64)
}

$TARGET_DIR = "/home/$PI_USER/ArcadeMatrix_RPi"

Write-Host "[Step 2] Stopping arcadematrix service on Raspberry Pi..."
Invoke-RemoteBash ('echo ''{0}'' | sudo -S systemctl stop arcadematrix.service || true' -f $PI_PASS)

Write-Host "[Step 3] Uploading binary to $TARGET_DIR..."
Invoke-RemoteBash ('mkdir -p {0}' -f $TARGET_DIR)
$SCP_DEST = '{0}:{1}/arcadematrix_temp' -f $REMOTE_USER_IP, $TARGET_DIR
scp -o StrictHostKeyChecking=no $BIN_PATH $SCP_DEST

if (Test-Path -Path "scripts") {
    Write-Host "[Info] Syncing scripts/ directory..."
    Invoke-RemoteBash ('mkdir -p {0}/scripts' -f $TARGET_DIR)
    scp -r -o StrictHostKeyChecking=no scripts/* ('{0}:{1}/scripts/' -f $REMOTE_USER_IP, $TARGET_DIR)
}

Write-Host "[Step 4] Moving binary and configuring permissions..."
Invoke-RemoteBash ('echo ''{0}'' | sudo -S mv {1}/arcadematrix_temp {1}/arcadematrix' -f $PI_PASS, $TARGET_DIR)
Invoke-RemoteBash ('echo ''{0}'' | sudo -S chmod +x {1}/arcadematrix {1}/scripts/*.sh 2>/dev/null || true' -f $PI_PASS, $TARGET_DIR)

Write-Host "[Step 4.5] Verifying config.json persistence on the DATA partition..."
$RepairConfig = "echo '$PI_PASS' | sudo -S bash -c 'cd $TARGET_DIR || exit 0; mkdir -p data; T=`$(readlink -f config.json 2>/dev/null); D=`$(readlink -f data/config.json 2>/dev/null); if [ -L config.json ] && [ x`$T = x`$D ] && [ -f config.json ]; then echo config.json already persisted in data - no changes; else echo Repairing config.json persistence...; if [ -f config.json ] && [ ! -L config.json ]; then [ -f data/config.json ] || cp -f config.json data/config.json; fi; [ -f data/config.json ] || echo {} > data/config.json; rm -f config.json; ln -s data/config.json config.json; if [ -L conf.ini ]; then rm -f conf.ini; fi; chown -h ${PI_USER}:${PI_USER} config.json 2>/dev/null || true; chown ${PI_USER}:${PI_USER} data/config.json 2>/dev/null || true; echo config.json now points to data/config.json; fi'"
Invoke-RemoteBash $RepairConfig

Write-Host "[Step 5] Restarting arcadematrix service..."
Invoke-RemoteBash ('echo ''{0}'' | sudo -S systemctl restart arcadematrix.service' -f $PI_PASS)

Write-Host "[Success] Deployment successful!"
Write-Host "=========================================================="
