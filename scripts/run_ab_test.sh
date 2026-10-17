#!/usr/bin/env bash
# =============================================================================
# ArcadeMatrix - Automated A/B Test (Memory Bus vs Hardware Pulsing)
# -----------------------------------------------------------------------------
# PROBLEM: Chaining commands over SSH fails when hardware pulsing is enabled,
# because HW pulsing can disrupt onboard Wi-Fi and freeze the SSH session.
# This script runs entirely in a detached background session, survives SSH
# disconnection, and restores hardware pulsing OFF at the end so Wi-Fi
# recovers automatically and logs can be inspected.
#
# Execution flow (~6 min, unattended):
#   1. Round A: rotation=date  (static display, idle memory bus) + pulsing ON
#               -> sample for 120s -> diag_A_date.log
#   2. Round B: rotation=gifs  (high memory bus load)            + pulsing ON
#               -> sample for 120s -> diag_B_gifs.log
#   3. Restore: pulsing OFF (+ original rotation) -> Wi-Fi recovers
#
# USAGE (on the Raspberry Pi):
#   cd ~/ArcadeMatrix_RPi
#   sudo bash scripts/run_ab_test.sh
#   -> prints output paths and returns immediately. SSH will drop: this is normal.
#   -> reconnect ~6 minutes later, then inspect:
#        grep -c LOSS ~/diag_A_date.log ~/diag_B_gifs.log
# =============================================================================
set -u

# --- Settings ----------------------------------------------------------------
SAMPLE="${SAMPLE:-120}"                 # sampling duration per round (s)
SERVICE="${SERVICE:-arcadematrix}"
SETTLE="${SETTLE:-12}"                  # wait time after service restart before sampling
OUTDIR="${OUTDIR:-/home/$USER}"
DIAG="$(cd "$(dirname "$0")" && pwd)/wifi_diag.sh"

# --- Locate config.json ------------------------------------------------------
CONF="${CONF:-}"
if [ -z "$CONF" ]; then
  for p in "/home/$USER/ArcadeMatrix_RPi/config.json" \
           "$(dirname "$0")/../config.json" \
           "/root/ArcadeMatrix_RPi/config.json"; do
    [ -f "$p" ] && { CONF="$p"; break; }
  done
fi

read_orig_state() {
  python3 - "$CONF" <<'PY' 2>/dev/null
import json, sys
try:
    with open(sys.argv[1], "r", encoding="utf-8") as f:
        data = json.load(f)
    rot = data.get("rotation", {})
    if isinstance(rot, dict):
        enabled = rot.get("enabled", [])
    else:
        enabled = []
    pulse = data.get("matrix", {}).get("disable_hardware_pulsing", False)
    print(json.dumps({"enabled": enabled, "disable_hardware_pulsing": bool(pulse)}))
except Exception:
    print(json.dumps({"enabled": ["gifs:10"], "disable_hardware_pulsing": False}))
PY
}

# --- Auto-daemonize: survive SSH disconnection -------------------------------
if [ "${_AB_DETACHED:-0}" != "1" ]; then
  if [ -z "$CONF" ] || [ ! -f "$CONF" ]; then
    echo "ERROR: config.json not found. Set CONF=/path/to/config.json"; exit 1
  fi
  if [ ! -f "$DIAG" ]; then
    echo "ERROR: wifi_diag.sh not found next to script ($DIAG)"; exit 1
  fi
  export _AB_DETACHED=1 SAMPLE SERVICE SETTLE OUTDIR DIAG CONF
  setsid bash "$0" >/dev/null 2>&1 < /dev/null &
  child=$!
  ORIG_STATE="$(read_orig_state)"
  cat <<EOF
===================================================================
 A/B test launched in detached session (PID $child).
   config.json : $CONF
   initial state : $ORIG_STATE (will be restored at the end)
   Round A (date) then Round B (gifs), ${SAMPLE}s each, pulsing ON.
   At completion: pulsing OFF -> Wi-Fi recovers automatically.
-------------------------------------------------------------------
 Your SSH session WILL FREEZE when hardware pulsing activates: expected.
 Reconnect in ~6 minutes, then run:
   grep -c LOSS $OUTDIR/diag_A_date.log $OUTDIR/diag_B_gifs.log
 Follow progress (once reconnected):
   cat $OUTDIR/ab_test_progress.log
 To abort early: sudo kill $child
===================================================================
EOF
  exit 0
fi

# =============================================================================
# ----------------------------- Detached Body ---------------------------------
# =============================================================================
PROG="$OUTDIR/ab_test_progress.log"
ts(){ date '+%Y-%m-%d %H:%M:%S'; }
log(){ echo "[$(ts)] $*" >> "$PROG"; sync 2>/dev/null; }

: > "$PROG"
ORIG_STATE="$(read_orig_state)"
log "Starting A/B test. Initial state=$ORIG_STATE"

apply_round_conf(){ # $1=disable_hardware_pulsing(true/false) $2=engine_id
  python3 - "$CONF" "$1" "$2" <<'PY' 2>/dev/null
import json, sys
path, pulse_str, mode = sys.argv[1], sys.argv[2], sys.argv[3]
with open(path, "r", encoding="utf-8") as f:
    data = json.load(f)
data.setdefault("matrix", {})["disable_hardware_pulsing"] = (pulse_str.lower() == "true")
rot = data.setdefault("rotation", {})
if isinstance(rot, dict):
    rot["enabled"] = [f"{mode}:10"]
with open(path, "w", encoding="utf-8") as f:
    json.dump(data, f, indent=2)
PY
}

restore_orig_conf(){ # $1=ORIG_STATE_JSON
  python3 - "$CONF" "$1" <<'PY' 2>/dev/null
import json, sys
path, state_json = sys.argv[1], sys.argv[2]
state = json.loads(state_json)
with open(path, "r", encoding="utf-8") as f:
    data = json.load(f)
data.setdefault("matrix", {})["disable_hardware_pulsing"] = True
rot = data.setdefault("rotation", {})
if isinstance(rot, dict) and state.get("enabled"):
    rot["enabled"] = state["enabled"]
with open(path, "w", encoding="utf-8") as f:
    json.dump(data, f, indent=2)
PY
}

run_round(){ # $1=label $2=rotation_mode $3=logfile
  local label="$1" rot="$2" out="$3"
  log ">>> Round $label: pulsing ON, rotation=$rot"
  apply_round_conf false "$rot"
  sync 2>/dev/null
  systemctl restart "$SERVICE" >/dev/null 2>&1
  log "    service restarted, waiting ${SETTLE}s to settle..."
  sleep "$SETTLE"
  log "    sampling for ${SAMPLE}s -> $out"
  # Synchronous invocation of wifi_diag.sh (blocks until finished via _DIAG_DETACHED=1)
  _DIAG_DETACHED=1 LOG="$out" DURATION="$SAMPLE" INTERVAL=2 CONF="$CONF" \
    bash "$DIAG" >/dev/null 2>&1
  local n; n="$(grep -c LOSS "$out" 2>/dev/null || echo 0)"
  log "    Round $label finished. LOSS count=$n"
}

run_round "A_date" "date" "$OUTDIR/diag_A_date.log"
run_round "B_gifs" "gifs" "$OUTDIR/diag_B_gifs.log"

# --- Restore: pulsing OFF so Wi-Fi recovers ----------------------------------
log "<<< Restoring configuration: pulsing OFF and original rotation"
restore_orig_conf "$ORIG_STATE"
sync 2>/dev/null
systemctl restart "$SERVICE" >/dev/null 2>&1
sleep 5

A="$(grep -c LOSS "$OUTDIR/diag_A_date.log" 2>/dev/null || echo 0)"
B="$(grep -c LOSS "$OUTDIR/diag_B_gifs.log" 2>/dev/null || echo 0)"
log "COMPLETED. LOSS  A_date=$A   B_gifs=$B"
log "Interpretation: A~0 & B>>0 => memory bus contention (software fix). A already high => hardware pulsing interference."
sync 2>/dev/null
