#!/bin/bash
set -e

echo "=== Running End-to-End Test for autoInstall.sh ==="

# Mock environment
MOCK_HOME="/tmp/mock_home"
MOCK_USER="mockadmin"

rm -rf $MOCK_HOME
mkdir -p $MOCK_HOME

export SUDO_USER="$MOCK_USER"
export HOME="$MOCK_HOME"

# Mock systemctl and apt-get to avoid actual system changes
mkdir -p /tmp/mock_bin
cat << 'EOF' > /tmp/mock_bin/systemctl
#!/bin/bash
echo "Mocked systemctl $@"
EOF
cat << 'EOF' > /tmp/mock_bin/apt-get
#!/bin/bash
echo "Mocked apt-get $@"
EOF
cat << 'EOF' > /tmp/mock_bin/sudo
#!/bin/bash
if [ "$1" = "DEBIAN_FRONTEND=noninteractive" ]; then
    shift
fi
if echo "$@" | grep -qE "/etc/|/boot/|/var/"; then
  echo "Mocked sudo (ignored): $@"
else
  "$@"
fi
EOF
cat << 'EOF' > /tmp/mock_bin/chown
#!/bin/bash
echo "Mocked chown $@"
EOF
chmod +x /tmp/mock_bin/systemctl /tmp/mock_bin/apt-get /tmp/mock_bin/sudo /tmp/mock_bin/chown
export PATH="/tmp/mock_bin:$PATH"

# Run autoInstall (with SKIP_BUILD to avoid compiling rust)
export SKIP_BUILD=1
mkdir -p /tmp/mock_run
cp autoInstall.sh /tmp/mock_run/
cd /tmp/mock_run
bash autoInstall.sh || true

echo "=== Verifying Results ==="
if [ ! -d "$MOCK_HOME/ArcadeMatrix_RPi" ]; then
    echo "❌ FAILED: Repository not cloned to mock home"
    exit 1
fi

if ! grep -q "alias am=" "$MOCK_HOME/.bash_aliases"; then
    echo "❌ FAILED: Aliases not written to .bash_aliases"
    exit 1
fi

echo "✅ SUCCESS: autoInstall.sh behaved correctly in custom user environment."
