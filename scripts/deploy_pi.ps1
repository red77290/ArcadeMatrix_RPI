<#
.SYNOPSIS
ArcadeMatrix RPi Deploy Script for Windows

.DESCRIPTION
Compiles the Rust project (using build_local_64.ps1) and deploys it to the Raspberry Pi.
It uses native Windows 10/11 OpenSSH commands (scp and ssh).
It will prompt for the Raspberry Pi password unless SSH keys are configured.
#>

$PI_IP = "192.168.1.169"
$PI_USER = "pi"
$PI_PASS = "raspberry"
$BIN_PATH = "target/aarch64-unknown-linux-gnu/release/arcadematrix"

Write-Host "========================================="
Write-Host "   🚀 ArcadeMatrix RPi Deploy Script"
Write-Host "========================================="

Write-Host "🔨 1. Building 64-bit binary..."
& .\scripts\build_local_64.ps1
if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ Error: Build failed." -ForegroundColor Red
    exit 1
}

if (-Not (Test-Path $BIN_PATH)) {
    Write-Host "❌ Error: Binary not found at $BIN_PATH. Build failed?" -ForegroundColor Red
    exit 1
}

Write-Host "🛑 2. Stopping arcadematrix service on Raspberry Pi..."
Write-Host "Note: You may be prompted for the SSH password if keys are not set up." -ForegroundColor Yellow
ssh -4 ${PI_USER}@${PI_IP} "echo '${PI_PASS}' | sudo -S systemctl stop arcadematrix.service || true"

Write-Host "📤 3. Uploading new binary to Raspberry Pi..."
scp -4 ${BIN_PATH} ${PI_USER}@${PI_IP}:/home/${PI_USER}/arcadematrix_temp

Write-Host "⚙️  4. Moving binary, setting permissions, and restarting service..."
ssh -4 ${PI_USER}@${PI_IP} "echo '${PI_PASS}' | sudo -S mv /home/${PI_USER}/arcadematrix_temp /usr/local/bin/arcadematrix && echo '${PI_PASS}' | sudo -S chmod +x /usr/local/bin/arcadematrix && echo '${PI_PASS}' | sudo -S systemctl restart arcadematrix.service"

Write-Host "✅ Deployment successful! Service restarted." -ForegroundColor Green
