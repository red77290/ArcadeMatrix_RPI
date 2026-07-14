#!/bin/bash
set -e
echo "📦 Installing build dependencies inside Docker..."
apt-get update
DEBIAN_FRONTEND=noninteractive apt-get install -y wget kpartx qemu-user-static e2fsprogs fdisk dosfstools exfatprogs xz-utils sudo parted python3 rsync

echo "📥 Downloading latest Raspberry Pi OS Lite (ARM64 Bookworm)..."
IMG_URL="https://downloads.raspberrypi.com/raspios_lite_arm64/images/raspios_lite_arm64-2024-07-04/2024-07-04-raspios-bookworm-arm64-lite.img.xz"

IMG_FILE="ArcadeMatrix_Build_${RANDOM}.img"

if [ ! -f "raspios.img.xz" ]; then
    wget -O raspios.img.xz "$IMG_URL"
fi

echo "🗜️ Extracting OS image..."
xz -d -c raspios.img.xz > $IMG_FILE

IMAGE_SIZE=${IMAGE_SIZE:-14G}

echo "📏 Expanding image to total size of $IMAGE_SIZE for DATA partition..."
truncate -s $IMAGE_SIZE $IMG_FILE

echo "📏 Expanding ROOT partition (p2) to 4GB to make room for build tools..."
parted -s $IMG_FILE resizepart 2 4000M

echo "💽 Creating 3rd partition (DATA)..."
# Get the starting sector for the new partition
START_SECTOR=$(fdisk -l $IMG_FILE | grep "img2" | awk '{print $3}')
# If grep fails to match exactly, fallback to tail -1 (which would be p2 at this point)
if [ -z "$START_SECTOR" ]; then
    START_SECTOR=$(fdisk -l $IMG_FILE | tail -1 | awk '{print $3}')
fi
NEXT_SECTOR=$((START_SECTOR + 1))

# Create new partition taking up the rest of the file
parted -s $IMG_FILE mkpart primary fat32 ${NEXT_SECTOR}s 100%

echo "🔧 Changing DATA partition type to 0x07 (exFAT/NTFS) for Windows/Mac compatibility..."
echo "type=7" | sfdisk $IMG_FILE --part 3

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
mount $PART_DATA /mnt/rootfs/home/pi/ArcadeMatrix_RPi/data

echo "🛠️ Preparing CHROOT environment..."
cp /usr/bin/qemu-aarch64-static /mnt/rootfs/usr/bin/
mount --bind /dev /mnt/rootfs/dev
mount --bind /sys /mnt/rootfs/sys
mount --bind /proc /mnt/rootfs/proc

echo "📁 Copying ArcadeMatrix project into image..."
mkdir -p /mnt/rootfs/home/pi/ArcadeMatrix_RPi
# Copy everything except the scripts/ and large media folders to avoid filling the 2GB root partition
# Copy everything except the scripts/ and large media folders to avoid filling the 2GB root partition
rsync -a --exclude='scripts' --exclude='.*' --exclude='venv' --exclude='*.img' --exclude='*.xz' --exclude='__pycache__' --exclude='fighters_32' --exclude='fighters_64' --exclude='gifs' --exclude='fonts' /workspace/ /mnt/rootfs/home/pi/ArcadeMatrix_RPi/
chown -R 1000:1000 /mnt/rootfs/home/pi/ArcadeMatrix_RPi || true

echo "📁 Copying large media directly to DATA partition..."
cp -r /workspace/fighters_32 /mnt/rootfs/home/pi/ArcadeMatrix_RPi/data/ 2>/dev/null || true
cp -r /workspace/fighters_64 /mnt/rootfs/home/pi/ArcadeMatrix_RPi/data/ 2>/dev/null || true
cp -r /workspace/gifs /mnt/rootfs/home/pi/ArcadeMatrix_RPi/data/ 2>/dev/null || true
cp -r /workspace/fonts /mnt/rootfs/home/pi/ArcadeMatrix_RPi/data/ 2>/dev/null || true

echo "📝 Injecting chroot setup script..."
cp /workspace/scripts/chroot_setup.sh /mnt/rootfs/tmp/chroot_setup.sh
chmod +x /mnt/rootfs/tmp/chroot_setup.sh

# Pass DATA partition UUID to chroot to setup fstab
DATA_UUID=$(blkid -s UUID -o value $PART_DATA)
echo "$DATA_UUID" > /mnt/rootfs/tmp/data_uuid.txt

echo "🚀 Entering ARM emulator to compile and obfuscate Python code..."
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

mv $IMG_FILE /workspace/ArcadeMatrix_Release.img
echo "🎉 Image built successfully: ArcadeMatrix_Release.img"
