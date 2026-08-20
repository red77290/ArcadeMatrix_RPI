#!/bin/bash
set -e
set -x
AM_USER=${AM_USER:-pi}

echo "🔧 [chroot] Starting ArcadeMatrix OS Setup via autoInstall.sh..."

PROJ_DIR="/home/$AM_USER/ArcadeMatrix_RPi"
cd $PROJ_DIR

# Link scripts early so autoInstall.sh can find recovery.sh
ln -s $PROJ_DIR/data/scripts $PROJ_DIR/scripts

# 1. Run the base auto-installer (systemd setup)
chmod +x autoInstall.sh
# Create a dummy Cargo.toml to satisfy autoInstall.sh's root check
touch Cargo.toml
export SKIP_BUILD=1
export SUDO_USER=$AM_USER
./autoInstall.sh

# 3. Add extra Release-only steps (DATA Partition)
echo "🔗 [chroot] Configuring DATA partition..."

# Initialize the DATA partition
mkdir -p $PROJ_DIR/data/gifs
mkdir -p $PROJ_DIR/data/fonts
mkdir -p $PROJ_DIR/data/fighters_32
mkdir -p $PROJ_DIR/data/fighters_64
mkdir -p $PROJ_DIR/data/crypto_icons
mkdir -p $PROJ_DIR/data/stock_icons
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


chown -R $AM_USER:$AM_USER $PROJ_DIR/data || true

# Add DATA partition to fstab so it mounts on boot
echo "LABEL=DATA  $PROJ_DIR/data  exfat  defaults,uid=1000,gid=1000,umask=000  0  2" >> /etc/fstab

# Create symlinks to the DATA partition
ln -s $PROJ_DIR/data/gifs $PROJ_DIR/gifs
ln -s $PROJ_DIR/data/fonts $PROJ_DIR/fonts
ln -s $PROJ_DIR/data/fighters_32 $PROJ_DIR/fighters_32
ln -s $PROJ_DIR/data/fighters_64 $PROJ_DIR/fighters_64
ln -s $PROJ_DIR/data/crypto_icons $PROJ_DIR/crypto_icons
ln -s $PROJ_DIR/data/stock_icons $PROJ_DIR/stock_icons

echo "⚙️ [chroot] Copying conf.ini to DATA partition..."
cp $PROJ_DIR/conf.ini $PROJ_DIR/data/conf.ini
cp $PROJ_DIR/conf.ini.backup $PROJ_DIR/data/conf.ini.backup || true
chown $AM_USER:$AM_USER $PROJ_DIR/data/conf.ini $PROJ_DIR/data/conf.ini.backup || true

# Create symlink for conf.ini
rm -f $PROJ_DIR/conf.ini || true
ln -s $PROJ_DIR/data/conf.ini $PROJ_DIR/conf.ini

echo "🧹 [chroot] Cleanup..."
rm -f /tmp/chroot_setup.sh

echo "✨ [chroot] Setup complete!"



apt-get clean
rm -rf /var/lib/apt/lists/*
history -c

echo "✅ [chroot] Setup Complete!"
