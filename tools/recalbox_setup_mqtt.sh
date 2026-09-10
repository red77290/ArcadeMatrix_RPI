#!/bin/bash
# Backward-compatibility wrapper pointing to rpi_emulationstation_base_os_setup.sh
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$DIR/rpi_emulationstation_base_os_setup.sh" "$@"
