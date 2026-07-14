#!/bin/bash
# ArcadeMatrix_RPi Installation Script
# Recommended OS: Raspberry Pi OS Lite (32-bit or 64-bit)

echo "======================================"
echo "      ArcadeMatrix RPi Installer      "
echo "======================================"
echo "⚠️ WARNING for Raspberry Pi 5 users: "
echo "The hzeller rgb-led-matrix library does NOT support the Pi 5 natively"
echo "due to the new RP1 GPIO chip. You MUST use an active adapter board."
echo "Pi 3, Pi 4, and Zero 2 W are fully supported out of the box."
echo "======================================"

# 1. Stop existing service if installed
if systemctl list-unit-files | grep -q arcadematrix.service; then
    echo "Stopping existing ArcadeMatrix service..."
    sudo systemctl stop arcadematrix.service
    sudo systemctl disable arcadematrix.service 2>/dev/null
fi

# 2. Update and install dependencies
echo "Updating packages and installing system dependencies..."
sudo apt-get update
sudo apt-get install -y python3 python3-pip python3-dev python3-pil python3-flask python3-venv git build-essential curl cython3

# 2. Check if we are in the project root
if [ ! -f "main.py" ]; then
    echo "main.py not found. It looks like you ran this script standalone."
    echo "Cloning the ArcadeMatrix_RPI repository..."
    git clone https://github.com/red77290/ArcadeMatrix_RPI.git
    cd ArcadeMatrix_RPI || { echo "Failed to enter directory"; exit 1; }
else
    echo "Found main.py, proceeding with local files..."
fi

CURRENT_DIR=$(pwd)

# 3. Setup Python Virtual Environment
echo "Setting up Python Virtual Environment..."
python3 -m venv venv
./venv/bin/pip install -r requirements.txt

# 4. Configure Matrix Hardware
echo ""
echo "--- MATRIX HARDWARE CONFIGURATION ---"
echo "Press Enter to accept the defaults."
echo "Select your Hardware Mapping:"
echo "1) Adafruit HAT"
echo "2) Adafruit HAT with PWM modification"
echo "3) Joy-IT HAT / Regular (Active-3, supports 3 Parallel chains) (Default)"
echo "4) Regular Pi 1"
echo "5) Classic Pi 1"
read -p "Enter number [3]: " MAPPING_NUM

MAPPING_NUM=${MAPPING_NUM:-3}

case $MAPPING_NUM in
    1) MAPPING="adafruit-hat" ;;
    2) MAPPING="adafruit-hat-pwm" ;;
    3) MAPPING="regular" ;;
    4) MAPPING="regular-pi1" ;;
    5) MAPPING="classic-pi1" ;;
    *) MAPPING="regular" ;;
esac

read -p "Panel Rows/Height (e.g., 32, 64) [32]: " ROWS
ROWS=${ROWS:-32}

read -p "Panel Cols/Width (e.g., 64, 128) [64]: " COLS
COLS=${COLS:-64}

read -p "Chain Length (Number of panels in series) [1]: " CHAIN
CHAIN=${CHAIN:-1}

read -p "Parallel Chains [1]: " PARALLEL
PARALLEL=${PARALLEL:-1}

# Write initial configuration to conf.ini
cat <<EOT > conf.ini
[MATRIX]
ROWS = $ROWS
COLS = $COLS
HARDWARE_MAPPING = $MAPPING
CHAIN = $CHAIN
PARALLEL = $PARALLEL
EOT
echo "Saved initial Matrix configuration to conf.ini"
echo ""

# 5. Install hzeller's rgbmatrix library
if [ ! -d "rpi-rgb-led-matrix" ]; then
    echo "Cloning hzeller's rpi-rgb-led-matrix library..."
    git clone https://github.com/hzeller/rpi-rgb-led-matrix.git
fi

echo "Compiling rgbmatrix library via scikit-build-core (this might take a few minutes)..."
cd rpi-rgb-led-matrix
# The library uses modern CMake via pyproject.toml now
../venv/bin/pip install .
cd ..


# 6. Anti-Flicker Performance Tweaks (Disable Audio)
echo "Applying anti-flicker optimizations (Disabling Onboard Audio)..."

# Blacklist sound module
sudo bash -c "cat > /etc/modprobe.d/snd-blacklist.conf << EOF
blacklist snd_bcm2835
EOF"

# Disable audio in config.txt (handling both older OS and Bookworm paths)
CONFIG_TXT="/boot/config.txt"
if [ -f "/boot/firmware/config.txt" ]; then
    CONFIG_TXT="/boot/firmware/config.txt"
fi

if grep -q "dtparam=audio=on" "$CONFIG_TXT"; then
    sudo sed -i 's/dtparam=audio=on/dtparam=audio=off/g' "$CONFIG_TXT"
    echo "Disabled audio in $CONFIG_TXT"
elif ! grep -q "dtparam=audio=off" "$CONFIG_TXT"; then
    echo "dtparam=audio=off" | sudo tee -a "$CONFIG_TXT" > /dev/null
fi

# 7. Setup Systemd Service
echo "Setting up systemd service for auto-start..."
SERVICE_FILE="/etc/systemd/system/arcadematrix.service"

sudo bash -c "cat > $SERVICE_FILE << EOF
[Unit]
Description=ArcadeMatrix RPi Daemon
After=network.target

[Service]
ExecStart=$CURRENT_DIR/venv/bin/python $CURRENT_DIR/main.py
WorkingDirectory=$CURRENT_DIR
StandardOutput=inherit
StandardError=inherit
Restart=always
User=root
# root is required to interact with GPIO for the LED Matrix

[Install]
WantedBy=multi-user.target
EOF"

sudo systemctl daemon-reload
sudo systemctl enable arcadematrix.service
sudo systemctl restart arcadematrix.service

IP_ADDR=$(hostname -I | awk '{print $1}')
echo "======================================"
echo "Installation Complete!"
echo "The service has been started. You can check its status with:"
echo "sudo systemctl status arcadematrix.service"
echo ""
if [ -n "$IP_ADDR" ]; then
    echo "You can access the Web UI at: http://$IP_ADDR:8080"
else
    echo "You can access the Web UI at: http://<raspberry-pi-ip>:8080"
fi
echo "======================================"
echo "⚠️ IMPORTANT: Audio has been disabled to prevent matrix flickering."
echo "Please REBOOT your Raspberry Pi now to apply the hardware changes!"
echo "Command: sudo reboot"
echo "======================================"
