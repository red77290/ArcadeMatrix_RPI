#!/bin/bash
set -e
echo "📦 Installing build dependencies inside Docker..."
apt-get update
DEBIAN_FRONTEND=noninteractive apt-get install -y wget kpartx qemu-user-static e2fsprogs fdisk dosfstools exfatprogs exfat-fuse xz-utils sudo parted python3 rsync

echo "📥 Downloading latest Raspberry Pi OS Lite ($ARCH Bookworm)..."
if [ "$ARCH" = "aarch64" ]; then
    IMG_URL="https://downloads.raspberrypi.com/raspios_lite_arm64/images/raspios_lite_arm64-2024-07-04/2024-07-04-raspios-bookworm-arm64-lite.img.xz"
    BIN_TARGET="aarch64-unknown-linux-gnu"
    export PKG_CONFIG_PATH=/usr/lib/aarch64-linux-gnu/pkgconfig
    export CC=aarch64-linux-gnu-gcc
    export CXX=aarch64-linux-gnu-g++
    export USER_DEFINES="-mcpu=cortex-a72"
    QEMU_BIN="/usr/bin/qemu-aarch64-static"
else
    IMG_URL="https://downloads.raspberrypi.com/raspios_lite_armhf/images/raspios_lite_armhf-2024-07-04/2024-07-04-raspios-bookworm-armhf-lite.img.xz"
    BIN_TARGET="armv7-unknown-linux-gnueabihf"
    export PKG_CONFIG_PATH=/usr/lib/arm-linux-gnueabihf/pkgconfig
    export CC=arm-linux-gnueabihf-gcc
    export CXX=arm-linux-gnueabihf-g++
    export USER_DEFINES="-mcpu=cortex-a7 -mfpu=neon-vfpv4 -mfloat-abi=hard"
    QEMU_BIN="/usr/bin/qemu-arm-static"
fi

IMG_FILE="/tmp/ArcadeMatrix_Build_${RANDOM}.img"

if [ -f "/workspace/raspios_${ARCH}.img.xz" ]; then
    echo "Using cached /workspace/raspios_${ARCH}.img.xz"
    cp /workspace/raspios_${ARCH}.img.xz /tmp/raspios.img.xz
else
    wget -O /tmp/raspios.img.xz "$IMG_URL"
    cp /tmp/raspios.img.xz /workspace/raspios_${ARCH}.img.xz || true
fi

echo "🗜️ Extracting OS image natively in /tmp..."
xz -d -c /tmp/raspios.img.xz > $IMG_FILE

IMAGE_SIZE=${IMAGE_SIZE:-14G}

echo "📏 Expanding image to total size of $IMAGE_SIZE for DATA partition..."
truncate -s $IMAGE_SIZE $IMG_FILE

echo "📏 Expanding ROOT partition (p2) to 8GB to make room for build tools..."
parted -s $IMG_FILE resizepart 2 8192MiB

echo "💽 Creating 3rd partition (DATA)..."
# Get the starting sector for the new partition
START_SECTOR=$(fdisk -l $IMG_FILE | grep "img2" | awk '{print $3}')
# If grep fails to match exactly, fallback to tail -1 (which would be p2 at this point)
if [ -z "$START_SECTOR" ]; then
    START_SECTOR=$(fdisk -l $IMG_FILE | tail -1 | awk '{print $3}')
fi
NEXT_SECTOR=$((START_SECTOR + 1))

# Create new partition taking up the rest of the file. Using 'ntfs' sets the MBR type to 0x07 (exFAT/NTFS)
parted -s $IMG_FILE mkpart primary ntfs ${NEXT_SECTOR}s 100%


echo "🔗 Mounting partitions to loop devices..."
LOOP_DEV=$(losetup -Pf --show $IMG_FILE)

# Force the kernel to read the partition table
partprobe $LOOP_DEV || true
sleep 3

# Inside docker, udev might not create the partition device nodes automatically.
# Let's explicitly ensure they exist, or use kpartx.
kpartx -av $LOOP_DEV || true
sleep 2

# Actually, kpartx creates /dev/mapper/loopXpY. So let's check what exists.
ls -l /dev/mapper/ /dev/loop*

if [ -e "/dev/mapper/$(basename ${LOOP_DEV})p3" ]; then
    PART_BOOT="/dev/mapper/$(basename ${LOOP_DEV})p1"
    PART_ROOT="/dev/mapper/$(basename ${LOOP_DEV})p2"
    PART_DATA="/dev/mapper/$(basename ${LOOP_DEV})p3"
else
    PART_BOOT="${LOOP_DEV}p1"
    PART_ROOT="${LOOP_DEV}p2"
    PART_DATA="${LOOP_DEV}p3"
fi

echo "🧹 Formatting DATA partition as exFAT..."
mkfs.exfat $PART_DATA

echo "📏 Expanding ROOT filesystem..."
e2fsck -f -p $PART_ROOT || true
resize2fs $PART_ROOT

echo "📂 Mounting root and boot partitions..."
mkdir -p /mnt/rootfs
mount $PART_ROOT /mnt/rootfs
mount $PART_BOOT /mnt/rootfs/boot/firmware

echo "📂 Mounting DATA partition..."
mkdir -p /mnt/rootfs/home/pi/ArcadeMatrix_RPi/data
# Use mount.exfat-fuse if available, fallback to standard mount
mount.exfat-fuse $PART_DATA /mnt/rootfs/home/pi/ArcadeMatrix_RPi/data || mount -t exfat $PART_DATA /mnt/rootfs/home/pi/ArcadeMatrix_RPi/data

echo "🔑 Enabling SSH and setting up default 'pi' user..."
touch /mnt/rootfs/boot/firmware/ssh
echo 'pi:$6$QM6/3dOlZrhCz7hG$RmgUadMoSC0mutMdHHhzjd52prRdb3zFcgOp5yZhza8LHQBwh.RbaFpBlf1YJSws6qz/H46VLIJ6YtQq6cNR/.' > /mnt/rootfs/boot/firmware/userconf.txt

echo "🛠️ Preparing CHROOT environment..."
cp $QEMU_BIN /mnt/rootfs/usr/bin/
mount --bind /dev /mnt/rootfs/dev
mount --bind /sys /mnt/rootfs/sys
mount --bind /proc /mnt/rootfs/proc

echo "📁 Copying ArcadeMatrix project into image..."
mkdir -p /mnt/rootfs/home/pi/ArcadeMatrix_RPi
# Only copy scripts and the configuration, no heavy source code or target/ directory
cp /workspace/conf.ini /mnt/rootfs/home/pi/ArcadeMatrix_RPi/
cp -r /workspace/scripts /mnt/rootfs/home/pi/ArcadeMatrix_RPi/
cp /workspace/autoInstall.sh /mnt/rootfs/home/pi/ArcadeMatrix_RPi/

echo "📁 Injecting cross-compiled Rust binary..."
mkdir -p /mnt/rootfs/usr/local/bin
cp /workspace/target/$BIN_TARGET/release/arcadematrix /mnt/rootfs/usr/local/bin/arcadematrix
chmod +x /mnt/rootfs/usr/local/bin/arcadematrix
chown -R 1000:1000 /mnt/rootfs/home/pi/ArcadeMatrix_RPi || true

echo "📁 Copying large media directly to DATA partition..."
cp -r /workspace/fighters_32 /mnt/rootfs/home/pi/ArcadeMatrix_RPi/data/ 2>/dev/null || true
cp -r /workspace/fighters_64 /mnt/rootfs/home/pi/ArcadeMatrix_RPi/data/ 2>/dev/null || true
cp -r /workspace/gifs /mnt/rootfs/home/pi/ArcadeMatrix_RPi/data/ 2>/dev/null || true
cp -r /workspace/fonts /mnt/rootfs/home/pi/ArcadeMatrix_RPi/data/ 2>/dev/null || true
cp -r /workspace/api/www /mnt/rootfs/home/pi/ArcadeMatrix_RPi/data/www 2>/dev/null || true
ln -s /home/pi/ArcadeMatrix_RPi/data/www /mnt/rootfs/home/pi/ArcadeMatrix_RPi/api/www || true

echo "📝 Injecting chroot setup script (only for systemd & symlinks)..."
cp /workspace/scripts/chroot_setup.sh /mnt/rootfs/tmp/chroot_setup.sh
chmod +x /mnt/rootfs/tmp/chroot_setup.sh

# Pass DATA partition UUID to chroot to setup fstab
DATA_UUID=$(blkid -s UUID -o value $PART_DATA)
echo "$DATA_UUID" > /mnt/rootfs/tmp/data_uuid.txt

echo "🚀 Entering ARM emulator to setup systemd services..."
chroot /mnt/rootfs /bin/bash /tmp/chroot_setup.sh

echo "🧹 Cleaning up mounts..."
umount /mnt/rootfs/proc
umount /mnt/rootfs/sys
umount /mnt/rootfs/dev
umount /mnt/rootfs/home/pi/ArcadeMatrix_RPi/data
umount /mnt/rootfs/boot/firmware
umount /mnt/rootfs

losetup -d $LOOP_DEV || true
kpartx -d $LOOP_DEV || true

mv $IMG_FILE /workspace/ArcadeMatrix_Release_${ARCH}.img
echo "🎉 Image built successfully: ArcadeMatrix_Release_${ARCH}.img"
