# ArcadeMatrix Smart Deploy (Windows)
# Auto-detects the Raspberry Pi architecture, builds via Docker, and deploys.

param (
    [string]$PI_IP = "192.168.1.149",
    [string]$PI_USER = "pi",
    [string]$PI_PASS = "raspberry"
)

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
$REMOTE_ARCH = ssh -o StrictHostKeyChecking=no "${PI_USER}@${PI_IP}" "uname -m"

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

Write-Host "[Step 1] Building binary for $REMOTE_ARCH using Docker..."
# Convert path for WSL/Docker if needed, or just rely on bash
bash scripts/build.sh "$REMOTE_ARCH"

if (-not (Test-Path -Path $BIN_PATH)) {
    Write-Host "[Error] Compiled binary not found at $BIN_PATH"
    exit 1
}

function Invoke-RemoteBash {
    param([string]$Command)
    $Bytes = [System.Text.Encoding]::UTF8.GetBytes($Command)
    $B64 = [Convert]::ToBase64String($Bytes)
    ssh -o StrictHostKeyChecking=no "${PI_USER}@${PI_IP}" "echo $B64 | base64 -d | bash"
}

Write-Host "[Step 2] Stopping arcadematrix service on Raspberry Pi..."
Invoke-RemoteBash "echo '${PI_PASS}' | sudo -S systemctl stop arcadematrix.service || true"

Write-Host "[Step 3] Uploading correct binary..."
scp -o StrictHostKeyChecking=no $BIN_PATH "${PI_USER}@${PI_IP}:/home/${PI_USER}/arcadematrix_temp"

Write-Host "[Step 4] Moving binary and starting service..."
Invoke-RemoteBash "echo '${PI_PASS}' | sudo -S mv /home/${PI_USER}/arcadematrix_temp /usr/local/bin/arcadematrix"
Invoke-RemoteBash "echo '${PI_PASS}' | sudo -S chmod +x /usr/local/bin/arcadematrix"

$CHECK_SERVICE_CMD = "if systemctl list-unit-files | grep -q arcadematrix.service; then echo 'installed'; else echo 'missing'; fi"
$BYTES = [System.Text.Encoding]::UTF8.GetBytes($CHECK_SERVICE_CMD)
$B64 = [Convert]::ToBase64String($BYTES)
$CHECK_SERVICE = ssh -o StrictHostKeyChecking=no "${PI_USER}@${PI_IP}" "echo $B64 | base64 -d | bash"

if ($CHECK_SERVICE -match "installed") {
    Write-Host "[OK] Service already installed, restarting only..."
    Invoke-RemoteBash "echo '${PI_PASS}' | sudo -S systemctl restart arcadematrix.service"
} else {
    Write-Host "[Warning] Service not found, running full autoInstall.sh setup..."
    Invoke-RemoteBash "echo '${PI_PASS}' | sudo -S env SKIP_BUILD=1 bash /home/${PI_USER}/ArcadeMatrix_RPi/autoInstall.sh"
}

Write-Host "[Success] Deployment successful!"
Write-Host "=========================================================="
