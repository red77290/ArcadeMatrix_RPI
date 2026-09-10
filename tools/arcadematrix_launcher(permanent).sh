#!/bin/sh
# ArcadeMatrix daemon launcher (Recalbox) - starts arcadematrix_daemon.py at EmulationStation
# boot/startup, restarting cleanly instead of piling up ghost processes if run more than once.
#
# Recalbox convention: userscripts named "*(permanent)" only fire on ES startup/shutdown, not on
# every single game launch - required here since the daemon itself polls /tmp/es_state.inf in a
# loop rather than being invoked once per game (unlike the Batocera hook script, see
# arcadematrix_mqtt_batocera.sh).
if [ -z "$1" ] || [ "$1" = "-action" -a "$2" = "start" ]; then
    pkill -f arcadematrix_daemon.py || true
    python3 /recalbox/share/arcadematrix_daemon.py > /recalbox/share/userscripts/daemon.log 2>&1 &
fi
