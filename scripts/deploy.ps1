# ArcadeMatrix Smart Deploy (Windows)
# Auto-detects the Raspberry Pi architecture, builds via Docker, and deploys.

param (
    [string]$PI_IP = "192.168.1.149",
    [string]$PI_USER = "",
    [string]$PI_PASS = "",
    [switch]$SkipBuild = $false,
    [string]$BinaryPath = ""
)

$DefaultUser = "pi"
$DefaultPass = "raspberry"
if (Test-Path "scripts/defaults.sh") {
    $env_content = Get-Content "scripts/defaults.sh"
    foreach ($line in $env_content) {
        if ($line -match "^export AM_USER=`"(.*)`"") { $DefaultUser = $matches[1] }
        if ($line -match "^export AM_PASS=`"(.*)`"") { $DefaultPass = $matches[1] }
    }
}

if ([string]::IsNullOrEmpty($PI_USER)) {
    $PI_USER = $DefaultUser
}
if ([string]::IsNullOrEmpty($PI_PASS)) {
    $PI_PASS = $DefaultPass
}

# Test if SSH is available
if (-not (Get-Command ssh -ErrorAction SilentlyContinue)) {
    Write-Host "[Error] Windows OpenSSH client is not installed."
    Write-Host "Please install it via Windows Settings > Apps > Optional Features."
    exit 1
}

Write-Host "=========================================================="
Write-Host "      ArcadeMatrix Smart Deploy to Raspberry Pi"
Write-Host "=========================================================="
Write-Host "[Info] Connecting to ${PI_USER}@${PI_IP} to detect OS architecture..."

# Detect remote architecture
# Use plink or standard ssh. We will use standard ssh since Windows 10/11 has it.
# We have to bypass strict host checking
$REMOTE_USER_IP = '{0}@{1}' -f $PI_USER, $PI_IP
$REMOTE_ARCH = ssh -o StrictHostKeyChecking=no $REMOTE_USER_IP 'uname -m'

if ($REMOTE_ARCH -match "aarch64") {
    Write-Host "[OK] Detected 64-bit OS (aarch64) on Raspberry Pi."
    $BIN_PATH = "target/aarch64-unknown-linux-gnu/release/arcadematrix"
} elseif ($REMOTE_ARCH -match "armv") {
    Write-Host "[OK] Detected 32-bit OS ($REMOTE_ARCH) on Raspberry Pi."
    $BIN_PATH = "target/armv7-unknown-linux-gnueabihf/release/arcadematrix"
} else {
    Write-Host "[Error] Unknown architecture: $REMOTE_ARCH"
    exit 1
}

if ($BinaryPath -ne "") {
    $BIN_PATH = $BinaryPath
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

Write-Host "[Step 1.5] Verifying config.json persistence on the DATA partition..."
# On correctly-provisioned images config.json is a symlink into the writable,
# persistent data/ dir (like gifs/, fonts/, ...). Boxes from an older
# (conf.ini-era) image instead have a plain config.json on the rootfs plus a stale
# conf.ini symlink; when the rootfs is read-only/overlay the backend's saves land
# on a volatile file and revert to defaults on restart. We VERIFY first: if
# config.json already resolves to data/config.json, nothing is touched. Only a
# broken layout is repaired, and an existing data/config.json is never clobbered.
$RepairConfig = "echo '$PI_PASS' | sudo -S bash -c 'cd /home/$PI_USER/ArcadeMatrix_RPi || exit 0; mkdir -p data; T=`$(readlink -f config.json 2>/dev/null); D=`$(readlink -f data/config.json 2>/dev/null); if [ -L config.json ] && [ x`$T = x`$D ] && [ -f config.json ]; then echo config.json already persisted in data - no changes; else echo Repairing config.json persistence...; if [ -f config.json ] && [ ! -L config.json ]; then [ -f data/config.json ] || cp -f config.json data/config.json; fi; [ -f data/config.json ] || echo {} > data/config.json; rm -f config.json; ln -s data/config.json config.json; if [ -L conf.ini ]; then rm -f conf.ini; fi; chown -h ${PI_USER}:${PI_USER} config.json 2>/dev/null || true; chown ${PI_USER}:${PI_USER} data/config.json 2>/dev/null || true; echo config.json now points to data/config.json; fi'"
Invoke-RemoteBash $RepairConfig

Write-Host "[Step 2] Stopping arcadematrix service on Raspberry Pi..."
Invoke-RemoteBash ('echo ''{0}'' | sudo -S systemctl stop arcadematrix.service || true' -f $PI_PASS)

Write-Host "[Step 3] Uploading correct binary..."
$SCP_DEST = '{0}:/home/{1}/arcadematrix_temp' -f $REMOTE_USER_IP, $PI_USER
scp -o StrictHostKeyChecking=no $BIN_PATH $SCP_DEST

Write-Host "[Step 4] Moving binary and starting service..."
Invoke-RemoteBash ('echo ''{0}'' | sudo -S mv /home/{1}/arcadematrix_temp /home/{1}/ArcadeMatrix_RPi/arcadematrix' -f $PI_PASS, $PI_USER)
Invoke-RemoteBash ('echo ''{0}'' | sudo -S chmod +x /home/{1}/ArcadeMatrix_RPi/arcadematrix' -f $PI_PASS, $PI_USER)

$CHECK_SERVICE_CMD = 'if systemctl list-unit-files | grep -q arcadematrix.service; then echo ''installed''; else echo ''missing''; fi'
$BYTES = [System.Text.Encoding]::UTF8.GetBytes($CHECK_SERVICE_CMD)
$B64 = [Convert]::ToBase64String($BYTES)
$CHECK_SERVICE = ssh -o StrictHostKeyChecking=no $REMOTE_USER_IP ('echo {0} | base64 -d | bash' -f $B64)

if ($CHECK_SERVICE -match "installed") {
    Write-Host "[OK] Service already installed, restarting only..."
    Invoke-RemoteBash ('echo ''{0}'' | sudo -S systemctl restart arcadematrix.service' -f $PI_PASS)
} else {
    Write-Host "[Warning] Service not found, running full autoInstall.sh setup..."
    Invoke-RemoteBash ('echo ''{0}'' | sudo -S env SKIP_BUILD=1 bash -c "$(curl -sSL https://raw.githubusercontent.com/red77290/ArcadeMatrix_RPI/main/autoInstall.sh)"' -f $PI_PASS)
}

Write-Host "[Success] Deployment successful!"
Write-Host "=========================================================="
