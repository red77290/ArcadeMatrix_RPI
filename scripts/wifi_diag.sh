#!/usr/bin/env bash
# =============================================================================
# ArcadeMatrix - Wi-Fi / DMA contention diagnostic logger
# -----------------------------------------------------------------------------
# Wi-Fi/SSH may drop when hardware pulsing is active, making real-time SSH
# monitoring impossible. This script runs DETACHED, periodically samples system
# and network metrics, and flushes everything to a persistent log file on disk
# that can be retrieved afterwards (via SD card or once Wi-Fi recovers).
#
# USAGE (on the Raspberry Pi, as root):
#   sudo bash scripts/wifi_diag.sh
#   -> re-launches itself in a detached session (setsid) and returns immediately.
#   -> prints the background PID and log file path.
#
# Environment variables:
#   DURATION=300        total sampling duration in seconds (default 300 = 5 min)
#   INTERVAL=2          sampling interval in seconds (default 2)
#   PROC=arcadematrix   process name to monitor
#   LOG=/path/to/log    output log file (default auto-detected below)
#
# To STOP early:  sudo kill <printed PID>
# =============================================================================

set -u

DURATION="${DURATION:-300}"
INTERVAL="${INTERVAL:-2}"
PROC="${PROC:-arcadematrix}"

# --- Select a persistent, writable log path ----------------------------------
pick_log() {
  local candidates=(
    "/boot/firmware/arcadematrix_wifi_diag.log"   # visible from SD card (Bookworm)
    "/boot/arcadematrix_wifi_diag.log"            # visible from SD card (legacy)
    "/var/log/arcadematrix_wifi_diag.log"
    "$HOME/arcadematrix_wifi_diag.log"
    "/tmp/arcadematrix_wifi_diag.log"
  )
  for c in "${candidates[@]}"; do
    local d; d="$(dirname "$c")"
    if [ -d "$d" ] && [ -w "$d" ]; then echo "$c"; return 0; fi
  done
  echo "/tmp/arcadematrix_wifi_diag.log"
}
LOG="${LOG:-$(pick_log)}"

# --- Auto-daemonize: survive SSH disconnection -------------------------------
if [ "${_DIAG_DETACHED:-0}" != "1" ]; then
  export _DIAG_DETACHED=1 DURATION INTERVAL PROC LOG
  setsid bash "$0" >/dev/null 2>&1 < /dev/null &
  child=$!
  echo "==================================================================="
  echo " ArcadeMatrix diagnostic started in detached session."
  echo "   PID      : $child"
  echo "   Log      : $LOG"
  echo "   Duration : ${DURATION}s   Interval : ${INTERVAL}s"
  echo "-------------------------------------------------------------------"
  echo " Wi-Fi may drop during testing: retrieve the log afterwards from"
  echo " the SD card or once network connectivity recovers:"
  echo "   sudo cat $LOG"
  echo " To stop early: sudo kill $child"
  echo "==================================================================="
  exit 0
fi

# =============================================================================
# ---------------------- Daemon Body (Detached) -------------------------------
# =============================================================================
ts()  { date '+%Y-%m-%d %H:%M:%S'; }
sect(){ printf '\n----- [%s] %s -----\n' "$(ts)" "$1" >> "$LOG"; }

: > "$LOG" 2>/dev/null || LOG="/tmp/arcadematrix_wifi_diag.log"; : > "$LOG"

NCPU="$(nproc 2>/dev/null || echo 4)"
GW="$(ip route 2>/dev/null | awk '/default/{print $3; exit}')"
MAINPID="$(pgrep -x "$PROC" | head -1)"

# --- Locate config.json (standard paths) -------------------------------------
CONF="${CONF:-}"
if [ -z "$CONF" ]; then
  for p in "/home/$USER/ArcadeMatrix_RPi/config.json" \
           "$(dirname "$0")/../config.json" \
           "/root/ArcadeMatrix_RPi/config.json" \
           "$HOME/ArcadeMatrix_RPi/config.json"; do
    [ -f "$p" ] && { CONF="$p"; break; }
  done
fi

# --- Detect Wi-Fi/SDIO IRQs (with device labels) -----------------------------
WIFI_IRQS="$(grep -Ei 'mmc|sdio|brcmf|dwc_otg|xhci' /proc/interrupts 2>/dev/null \
             | awk -F: '{gsub(/ /,"",$1); print $1}' | tr '\n' ' ')"

# ============================ HEADER (static context) ========================
{
  echo "==================================================================="
  echo " ArcadeMatrix Wi-Fi/DMA diagnostic  -  started $(ts)"
  echo "==================================================================="
  echo "Host        : $(hostname 2>/dev/null)"
  echo "Kernel      : $(uname -a)"
  echo "Model       : $(tr -d '\0' < /proc/device-tree/model 2>/dev/null)"
  echo "nproc       : $NCPU"
  echo "Gateway     : ${GW:-unknown}"
  echo "App PID     : ${MAINPID:-not found}"
  echo "Wi-Fi IRQs  : ${WIFI_IRQS:-none detected}"
  echo "Dur/Interval: ${DURATION}s / ${INTERVAL}s"
  echo
  echo "isolcpus    : $(cat /sys/devices/system/cpu/isolated 2>/dev/null)"
  echo "nohz_full   : $(cat /sys/devices/system/cpu/nohz_full 2>/dev/null)"
  echo "cmdline     : $(cat /proc/cmdline 2>/dev/null)"
  echo
  echo "Governor    : $(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null)"
  echo
  echo "--- config.json .matrix ($CONF) ---"
  if command -v jq >/dev/null 2>&1 && [ -f "$CONF" ]; then
    jq '.matrix' "$CONF" 2>/dev/null
  elif command -v python3 >/dev/null 2>&1 && [ -f "$CONF" ]; then
    python3 -c 'import json,sys; print(json.dumps(json.load(open(sys.argv[1])).get("matrix", {}), indent=2))' "$CONF" 2>/dev/null
  fi
  echo
  echo "--- /proc/interrupts (Wi-Fi IRQ lines with device names) ---"
  { head -1 /proc/interrupts; for irq in $WIFI_IRQS; do grep -E "^\s*$irq:" /proc/interrupts; done; } 2>/dev/null
  echo
  echo "--- Wi-Fi IRQ Affinity (smp_affinity_list) ---"
  for irq in $WIFI_IRQS; do
    echo "  IRQ $irq -> cores $(cat /proc/irq/$irq/smp_affinity_list 2>/dev/null)"
  done
  echo
  echo "--- ALL process threads (PID $MAINPID) ---"
  echo "  TID   PSR RTPRIO NI  %CPU COMMAND   (PSR=core, RTPRIO=99 => real-time)"
  [ -n "${MAINPID:-}" ] && ps -T -p "$MAINPID" -o tid,psr,rtprio,ni,pcpu,comm 2>/dev/null | tail -n +2
  echo "  Main process CPU affinity:"
  [ -n "${MAINPID:-}" ] && taskset -cp "$MAINPID" 2>/dev/null
} >> "$LOG" 2>&1

# --- Initial network state + dmesg marker ------------------------------------
sect "INITIAL NETWORK STATE"
{ ip -s link show 2>/dev/null; echo; iwconfig 2>/dev/null | grep -Ei 'link|signal|rate'; } >> "$LOG" 2>&1
LAST_DMESG_TS="$(dmesg 2>/dev/null | tail -1 | sed -E 's/^\[([0-9.]+)\].*/\1/')"

# --- /proc sampling helpers --------------------------------------------------
declare -A PREV_IRQ   # total interrupts per IRQ (delta between samples)
read_irq_totals() {   # outputs "irq total" per line for Wi-Fi IRQs
  local irq
  for irq in $WIFI_IRQS; do
    local line; line="$(grep -E "^\s*$irq:" /proc/interrupts 2>/dev/null)"
    [ -z "$line" ] && continue
    local sum=0 f
    for f in $(echo "$line" | awk '{for(i=2;i<=NF;i++) if($i ~ /^[0-9]+$/) print $i}'); do
      sum=$((sum + f))
    done
    echo "$irq $sum"
  done
}

# Per-core /proc/stat snapshot -> approximate "%busy" over interval
declare -A PREV_IDLE PREV_TOT
cpu_busy_line() {
  local out="" cpu idle tot
  while read -r cpu user nice system idle iowait irq softirq steal _; do
    [[ "$cpu" =~ ^cpu[0-9]+$ ]] || continue
    tot=$((user+nice+system+idle+iowait+irq+softirq+steal))
    idle=$((idle+iowait))
    local pid="${PREV_IDLE[$cpu]:-0}" ptot="${PREV_TOT[$cpu]:-0}"
    local dtot=$((tot-ptot)) didle=$((idle-pid)) busy=0
    [ "$dtot" -gt 0 ] && busy=$(( (100*(dtot-didle))/dtot ))
    out+="$cpu:${busy}% "
    PREV_IDLE[$cpu]=$idle; PREV_TOT[$cpu]=$tot
  done < /proc/stat
  echo "$out"
}
cpu_busy_line >/dev/null   # prime counters

# --- Leak/crash tracking: memory, threads, FDs, process restarts -------------
proc_stats() {   # outputs "rss=<KB> thr=<n> fd=<n>" for current PID
  local pid="${1:-0}" rss="?" thr="?" fd="?"
  if [ -n "$pid" ] && [ -r "/proc/$pid/status" ]; then
    rss="$(awk '/^VmRSS:/{print $2}' "/proc/$pid/status" 2>/dev/null)"
    thr="$(awk '/^Threads:/{print $2}' "/proc/$pid/status" 2>/dev/null)"
    fd="$(ls "/proc/$pid/fd" 2>/dev/null | wc -l | tr -d ' ')"
  fi
  echo "rss=${rss:-?}KB thr=${thr:-?} fd=${fd:-?}"
}
mem_avail() {   # System MemAvailable in MB (global memory pressure)
  awk '/^MemAvailable:/{printf "%d", $2/1024}' /proc/meminfo 2>/dev/null
}
FIRST_PID="$MAINPID"
RESTARTS=0

# ============================ SAMPLING LOOP ==================================
sect "START SAMPLING"
START=$(date +%s); N=0
while :; do
  now=$(date +%s); [ $((now-START)) -ge "$DURATION" ] && break
  N=$((N+1))

  # 0) Re-detect PID: if the app crashed/restarted, its PID changes
  CURPID="$(pgrep -x "$PROC" | head -1)"
  RESTART_FLAG=""
  if [ -n "$CURPID" ] && [ "$CURPID" != "${MAINPID:-}" ]; then
    RESTARTS=$((RESTARTS+1))
    RESTART_FLAG="  *** APP RESTART DETECTED: PID ${MAINPID:-?} -> $CURPID (restart #$RESTARTS) ***"
    MAINPID="$CURPID"
  fi
  [ -z "${CURPID:-}" ] && RESTART_FLAG="  *** PROCESS ABSENT (crashed, not yet restarted) ***"

  # 1) Per-core CPU load
  CPU="$(cpu_busy_line)"

  # 2) ksoftirqd CPU usage (sign of softirq starvation)
  KSOFT="$(ps -eLo psr,pcpu,comm 2>/dev/null | awk '/ksoftirqd/{printf "core%s=%s%% ",$1,$2}')"

  # 3) Wi-Fi interrupt delta (0 = no longer serviced = frozen link)
  IRQD=""
  while read -r irq sum; do
    [ -z "${irq:-}" ] && continue
    local_prev="${PREV_IRQ[$irq]:-$sum}"
    IRQD+="irq$irq:+$((sum-local_prev)) "
    PREV_IRQ[$irq]=$sum
  done < <(read_irq_totals)

  # 4) Gateway ping (1 packet, short timeout) -> LOSS = Wi-Fi drop
  PING="n/a"
  if [ -n "${GW:-}" ]; then
    if ping -c1 -W1 "$GW" >/dev/null 2>&1; then PING="OK"; else PING="LOSS"; fi
  fi

  # 5) hzeller real-time thread (prio 99): core and %CPU
  HZ="$(ps -T -p "${MAINPID:-0}" -o tid,psr,rtprio,pcpu,comm 2>/dev/null \
        | awk '$3==99 {printf "tid%s@core%s %s%% (%s) ",$1,$2,$4,$5}')"
  [ -z "$HZ" ] && HZ="(no rtprio99 thread in process)"

  # 5b) Leak/crash metrics: process RSS, threads, FDs + system available memory
  PSTATS="$(proc_stats "${MAINPID:-0}")"
  MEMAV="$(mem_avail)"

  printf '[%s] #%03d ping=%-5s | %s memAvail=%sMB | CPU %s| wifiIRQΔ %s| hz99 %s%s\n' \
    "$(ts)" "$N" "$PING" "$PSTATS" "${MEMAV:-?}" "$CPU" "${IRQD:-none }" "$HZ" "$RESTART_FLAG" >> "$LOG"

  # 6) Every ~10 samples: temperature/throttling + top app threads
  if [ $((N % 10)) -eq 1 ]; then
    {
      echo "    temp/throttle: $(vcgencmd measure_temp 2>/dev/null) $(vcgencmd get_throttled 2>/dev/null)"
      echo "    top threads (PID $MAINPID):"
      ps -T -p "${MAINPID:-0}" -o tid,psr,rtprio,ni,pcpu,comm 2>/dev/null | tail -n +2 \
        | sort -k5 -nr | head -8 | sed 's/^/      /'
    } >> "$LOG"
  fi

  # 7) New dmesg lines (brcmfmac/timeout errors)
  NEWDM="$(dmesg 2>/dev/null | awk -v t="${LAST_DMESG_TS:-0}" \
           '{ ts=$1; gsub(/[][]/,"",ts); if (ts+0 > t+0) print }')"
  if [ -n "$NEWDM" ]; then
    echo "    >>> DMESG:" >> "$LOG"
    echo "$NEWDM" | sed 's/^/      /' >> "$LOG"
    LAST_DMESG_TS="$(dmesg 2>/dev/null | tail -1 | sed -E 's/^\[([0-9.]+)\].*/\1/')"
  fi

  sync 2>/dev/null           # flush to disk so log survives even if the Pi freezes
  sleep "$INTERVAL"
done

sect "END - FINAL NETWORK STATE"
{ ip -s link show 2>/dev/null; echo; echo "dmesg (tail):"; dmesg 2>/dev/null | tail -40; } >> "$LOG"
echo "" >> "$LOG"
echo "--- journalctl arcadematrix (last 60 lines: panics/restarts) ---" >> "$LOG"
journalctl -u arcadematrix --no-pager -n 60 2>/dev/null >> "$LOG" || \
  echo "  (journalctl unavailable)" >> "$LOG"
echo "" >> "$LOG"
echo "=== Diagnostic completed $(ts) ($N samples) ===" >> "$LOG"
echo "=== App restarts detected during test: $RESTARTS (initial PID ${FIRST_PID:-?} -> final ${MAINPID:-?}) ===" >> "$LOG"
sync 2>/dev/null
