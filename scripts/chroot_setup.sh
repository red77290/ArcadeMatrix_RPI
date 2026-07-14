#!/bin/bash
set -e
set -x
echo "🔧 [chroot] Starting ArcadeMatrix OS Setup..."

# Disable interactive prompts
export DEBIAN_FRONTEND=noninteractive

echo "📦 [chroot] Installing dependencies..."
apt-get update
apt-get install -y python3 python3-pip python3-dev python3-pil python3-flask python3-venv git build-essential cython3 python3-psutil curl

# Create ArcadeMatrix directory if missing and set permissions
PROJ_DIR="/home/pi/ArcadeMatrix_RPi"
chown -R pi:pi $PROJ_DIR || true
cd $PROJ_DIR

# Set up virtual environment
python3 -m venv venv
./venv/bin/pip install Pillow Flask psutil

echo "🛠️ [chroot] Compiling hzeller RGB Matrix Library..."
git clone https://github.com/hzeller/rpi-rgb-led-matrix.git
cd rpi-rgb-led-matrix
../venv/bin/pip install .
cd ..

echo "🔒 [chroot] Cythonizing source code for protection..."
# Install Cython in venv
./venv/bin/pip install Cython

# Find all python files except main.py and compile them
find api core engines -name "*.py" -type f | while read -r file; do
    echo "Compiling $file..."
    ./venv/bin/cythonize -i -3 "$file"
    # Remove the original source and intermediate C files
    rm "$file"
    rm "${file%.py}.c" || true
done

echo "🔗 [chroot] Configuring DATA partition..."
DATA_UUID=$(cat /tmp/data_uuid.txt)

# Initialize the DATA partition
mkdir -p $PROJ_DIR/data/gifs
mkdir -p $PROJ_DIR/data/fonts
mkdir -p $PROJ_DIR/data/fighters_32
mkdir -p $PROJ_DIR/data/fighters_64

# Move existing fonts, gifs, and fighters to data partition
if [ -d "$PROJ_DIR/fonts" ]; then
    cp -r $PROJ_DIR/fonts/* $PROJ_DIR/data/fonts/ || true
    rm -rf $PROJ_DIR/fonts
fi
if [ -d "$PROJ_DIR/gifs" ]; then
    cp -r $PROJ_DIR/gifs/* $PROJ_DIR/data/gifs/ || true
    rm -rf $PROJ_DIR/gifs
fi
if [ -d "$PROJ_DIR/fighters_32" ]; then
    cp -r $PROJ_DIR/fighters_32/* $PROJ_DIR/data/fighters_32/ || true
    rm -rf $PROJ_DIR/fighters_32
fi
if [ -d "$PROJ_DIR/fighters_64" ]; then
    cp -r $PROJ_DIR/fighters_64/* $PROJ_DIR/data/fighters_64/ || true
    rm -rf $PROJ_DIR/fighters_64
fi

chown -R pi:pi $PROJ_DIR/data || true

# Add DATA partition to fstab so it mounts on boot
echo "UUID=$DATA_UUID  $PROJ_DIR/data  exfat  defaults,uid=1000,gid=1000,umask=000  0  2" >> /etc/fstab

# Create symlinks to the DATA partition
ln -s $PROJ_DIR/data/gifs $PROJ_DIR/gifs
ln -s $PROJ_DIR/data/fonts $PROJ_DIR/fonts
ln -s $PROJ_DIR/data/fighters_32 $PROJ_DIR/fighters_32
ln -s $PROJ_DIR/data/fighters_64 $PROJ_DIR/fighters_64

echo "⚙️ [chroot] Configuring conf.ini defaults in DATA partition..."
cat <<EOT > $PROJ_DIR/data/conf.ini
[MATRIX]
ROWS = 64
COLS = 64
HARDWARE_MAPPING = adafruit-hat
CHAIN = 1
PARALLEL = 1
EOT
chown pi:pi $PROJ_DIR/data/conf.ini || true

# Create symlink for conf.ini
rm -f $PROJ_DIR/conf.ini || true
ln -s $PROJ_DIR/data/conf.ini $PROJ_DIR/conf.ini

echo "🔌 [chroot] Setting up Systemd Service..."
SERVICE_FILE="/etc/systemd/system/arcadematrix.service"
cat > $SERVICE_FILE << EOF
[Unit]
Description=ArcadeMatrix RPi Daemon
After=network.target

[Service]
ExecStart=$PROJ_DIR/venv/bin/python $PROJ_DIR/main.py
WorkingDirectory=$PROJ_DIR
StandardOutput=inherit
StandardError=inherit
Restart=always
User=root

[Install]
WantedBy=multi-user.target
EOF

systemctl enable arcadematrix.service

echo "🚫 [chroot] Applying Anti-Flicker tweaks (Audio Disable)..."
cat > /etc/modprobe.d/snd-blacklist.conf << EOF
blacklist snd_bcm2835
EOF
sed -i 's/dtparam=audio=on/dtparam=audio=off/g' /boot/firmware/config.txt || true

echo "🧹 [chroot] Cleaning up..."
rm -rf /tmp/chroot_setup.sh /tmp/data_uuid.txt
apt-get clean
rm -rf /var/lib/apt/lists/*
history -c

echo "✅ [chroot] Setup Complete!"
