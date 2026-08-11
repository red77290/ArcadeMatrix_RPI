#!/usr/bin/env bash
# =============================================================================
# ArcadeMatrix - Wi-Fi / DMA contention diagnostic logger
# -----------------------------------------------------------------------------
# Le Wi-Fi/SSH tombe quand le pulsing HW est actif : impossible de mesurer en
# temps reel. Ce script tourne DETACHE, echantillonne periodiquement et ecrit
# TOUT dans un fichier de log sur disque, qu'on recupere APRES coup
# (via la carte SD, ou une fois le Wi-Fi revenu).
#
# USAGE (sur le Pi, en root) :
#   sudo bash scripts/wifi_diag.sh
#   -> se relance tout seul en tache detachee (setsid) et rend la main.
#   -> le PID et le chemin du log sont affiches.
#
# Reglages (variables d'env) :
#   DURATION=300   duree totale en secondes (defaut 300 = 5 min)
#   INTERVAL=2     periode d'echantillonnage en secondes (defaut 2)
#   PROC=arcadematrix   nom du process a suivre
#   LOG=/chemin/log     fichier de sortie (defaut auto, voir plus bas)
#
# Pour STOPPER avant la fin :  sudo kill <PID affiche>
# =============================================================================

set -u

DURATION="${DURATION:-300}"
INTERVAL="${INTERVAL:-2}"
PROC="${PROC:-arcadematrix}"

# --- Choix d'un log persistant et inscriptible -------------------------------
pick_log() {
  local candidates=(
    "/boot/firmware/arcadematrix_wifi_diag.log"   # visible depuis la carte SD (Bookworm)
    "/boot/arcadematrix_wifi_diag.log"            # visible depuis la carte SD (ancien)
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

# --- Auto-daemonisation : survit a la chute du SSH ---------------------------
if [ "${_DIAG_DETACHED:-0}" != "1" ]; then
  export _DIAG_DETACHED=1 DURATION INTERVAL PROC LOG
  setsid bash "$0" >/dev/null 2>&1 < /dev/null &
  child=$!
  echo "==================================================================="
  echo " Diagnostic ArcadeMatrix lance en tache detachee."
  echo "   PID     : $child"
  echo "   Log     : $LOG"
  echo "   Duree   : ${DURATION}s   Intervalle : ${INTERVAL}s"
  echo "-------------------------------------------------------------------"
  echo " Le Wi-Fi peut tomber : recupere le log ensuite via la carte SD"
  echo " ou une fois la connexion revenue :"
  echo "   sudo cat $LOG"
  echo " Pour arreter : sudo kill $child"
  echo "==================================================================="
  exit 0
fi

# =============================================================================
# ---------------------- Corps du daemon (detache) ----------------------------
# =============================================================================
ts()  { date '+%Y-%m-%d %H:%M:%S'; }
sect(){ printf '\n----- [%s] %s -----\n' "$(ts)" "$1" >> "$LOG"; }

: > "$LOG" 2>/dev/null || LOG="/tmp/arcadematrix_wifi_diag.log"; : > "$LOG"

NCPU="$(nproc 2>/dev/null || echo 4)"
GW="$(ip route 2>/dev/null | awk '/default/{print $3; exit}')"
MAINPID="$(pgrep -x "$PROC" | head -1)"

# --- Localisation de conf.ini (chemins usuels) -------------------------------
CONF="${CONF:-}"
if [ -z "$CONF" ]; then
  for p in "/home/$USER/ArcadeMatrix_RPi/conf.ini" \
           "$(dirname "$0")/../conf.ini" \
           "/root/ArcadeMatrix_RPi/conf.ini" \
           "$HOME/ArcadeMatrix_RPi/conf.ini"; do
    [ -f "$p" ] && { CONF="$p"; break; }
  done
fi

# --- Detection de l'IRQ Wi-Fi/SDIO (avec libelle du peripherique) ------------
WIFI_IRQS="$(grep -Ei 'mmc|sdio|brcmf|dwc_otg|xhci' /proc/interrupts 2>/dev/null \
             | awk -F: '{gsub(/ /,"",$1); print $1}' | tr '\n' ' ')"

# ============================ EN-TETE (contexte fixe) ========================
{
  echo "==================================================================="
  echo " ArcadeMatrix Wi-Fi/DMA diagnostic  -  demarrage $(ts)"
  echo "==================================================================="
  echo "Host        : $(hostname 2>/dev/null)"
  echo "Kernel      : $(uname -a)"
  echo "Modele      : $(tr -d '\0' < /proc/device-tree/model 2>/dev/null)"
  echo "nproc       : $NCPU"
  echo "Passerelle  : ${GW:-inconnue}"
  echo "PID app     : ${MAINPID:-introuvable}"
  echo "IRQ Wi-Fi   : ${WIFI_IRQS:-aucune detectee}"
  echo "Duree/Interv: ${DURATION}s / ${INTERVAL}s"
  echo
  echo "isolcpus    : $(cat /sys/devices/system/cpu/isolated 2>/dev/null)"
  echo "nohz_full   : $(cat /sys/devices/system/cpu/nohz_full 2>/dev/null)"
  echo "cmdline     : $(cat /proc/cmdline 2>/dev/null)"
  echo
  echo "Governor    : $(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null)"
  echo
  echo "--- conf.ini [MATRIX] ($CONF) ---"
  sed -n '/\[MATRIX\]/,/^\[/p' "$CONF" 2>/dev/null | grep -Ev '^\s*#|^\s*$'
  echo
  echo "--- /proc/interrupts (lignes IRQ Wi-Fi, avec peripherique) ---"
  { head -1 /proc/interrupts; for irq in $WIFI_IRQS; do grep -E "^\s*$irq:" /proc/interrupts; done; } 2>/dev/null
  echo
  echo "--- Affinite IRQ Wi-Fi (smp_affinity_list) ---"
  for irq in $WIFI_IRQS; do
    echo "  IRQ $irq -> cores $(cat /proc/irq/$irq/smp_affinity_list 2>/dev/null)"
  done
  echo
  echo "--- TOUS les threads du process (PID $MAINPID) ---"
  echo "  TID   PSR RTPRIO NI  %CPU COMMAND   (PSR=core, RTPRIO=99 => temps-reel)"
  [ -n "${MAINPID:-}" ] && ps -T -p "$MAINPID" -o tid,psr,rtprio,ni,pcpu,comm 2>/dev/null | tail -n +2
  echo "  Affinite process principal :"
  [ -n "${MAINPID:-}" ] && taskset -cp "$MAINPID" 2>/dev/null
} >> "$LOG" 2>&1

# --- Etat reseau initial + marqueur dmesg ------------------------------------
sect "ETAT RESEAU INITIAL"
{ ip -s link show 2>/dev/null; echo; iwconfig 2>/dev/null | grep -Ei 'link|signal|rate'; } >> "$LOG" 2>&1
LAST_DMESG_TS="$(dmesg 2>/dev/null | tail -1 | sed -E 's/^\[([0-9.]+)\].*/\1/')"

# --- Fonctions d'echantillonnage /proc ---------------------------------------
declare -A PREV_IRQ   # somme des interruptions par IRQ (delta entre samples)
read_irq_totals() {   # renvoie "irq total" par ligne pour les IRQ Wi-Fi
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

# Snapshot /proc/stat par core -> "%busy" approx sur l'intervalle
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
cpu_busy_line >/dev/null   # amorce les compteurs

# --- Suivi fuite/crash : mémoire, threads, FD, redémarrage du process --------
proc_stats() {   # renvoie "rss=<KB> thr=<n> fd=<n>" pour le PID courant
  local pid="${1:-0}" rss="?" thr="?" fd="?"
  if [ -n "$pid" ] && [ -r "/proc/$pid/status" ]; then
    rss="$(awk '/^VmRSS:/{print $2}' "/proc/$pid/status" 2>/dev/null)"
    thr="$(awk '/^Threads:/{print $2}' "/proc/$pid/status" 2>/dev/null)"
    fd="$(ls "/proc/$pid/fd" 2>/dev/null | wc -l | tr -d ' ')"
  fi
  echo "rss=${rss:-?}KB thr=${thr:-?} fd=${fd:-?}"
}
mem_avail() {   # MemAvailable système en MB (pression mémoire globale)
  awk '/^MemAvailable:/{printf "%d", $2/1024}' /proc/meminfo 2>/dev/null
}
FIRST_PID="$MAINPID"
RESTARTS=0

# ============================ BOUCLE D'ECHANTILLONNAGE =======================
sect "DEBUT ECHANTILLONNAGE"
START=$(date +%s); N=0
while :; do
  now=$(date +%s); [ $((now-START)) -ge "$DURATION" ] && break
  N=$((N+1))

  # 0) Re-detection du PID : si l'app a crashe/redemarre, le PID change
  CURPID="$(pgrep -x "$PROC" | head -1)"
  RESTART_FLAG=""
  if [ -n "$CURPID" ] && [ "$CURPID" != "${MAINPID:-}" ]; then
    RESTARTS=$((RESTARTS+1))
    RESTART_FLAG="  *** REDEMARRAGE APP DETECTE : PID ${MAINPID:-?} -> $CURPID (restart #$RESTARTS) ***"
    MAINPID="$CURPID"
  fi
  [ -z "${CURPID:-}" ] && RESTART_FLAG="  *** PROCESS ABSENT (crash, pas encore relance) ***"

  # 1) Charge CPU par core
  CPU="$(cpu_busy_line)"

  # 2) ksoftirqd : combien de CPU consomment-ils (signe de starvation softirq)
  KSOFT="$(ps -eLo psr,pcpu,comm 2>/dev/null | awk '/ksoftirqd/{printf "core%s=%s%% ",$1,$2}')"

  # 3) Delta interruptions Wi-Fi (0 = plus servi = lien fige)
  IRQD=""
  while read -r irq sum; do
    [ -z "${irq:-}" ] && continue
    local_prev="${PREV_IRQ[$irq]:-$sum}"
    IRQD+="irq$irq:+$((sum-local_prev)) "
    PREV_IRQ[$irq]=$sum
  done < <(read_irq_totals)

  # 4) Ping passerelle (1 paquet, timeout court) -> perte = coupure Wi-Fi
  PING="n/a"
  if [ -n "${GW:-}" ]; then
    if ping -c1 -W1 "$GW" >/dev/null 2>&1; then PING="OK"; else PING="PERTE"; fi
  fi

  # 5) Thread hzeller (prio 99) : sur quel core, quel %cpu
  HZ="$(ps -T -p "${MAINPID:-0}" -o tid,psr,rtprio,pcpu,comm 2>/dev/null \
        | awk '$3==99 {printf "tid%s@core%s %s%% (%s) ",$1,$2,$4,$5}')"
  [ -z "$HZ" ] && HZ="(aucun thread rtprio99 dans le process)"

  # 5b) Fuite/crash : RSS, threads, FD du process + memoire dispo systeme
  PSTATS="$(proc_stats "${MAINPID:-0}")"
  MEMAV="$(mem_avail)"

  printf '[%s] #%03d ping=%-5s | %s memAvail=%sMB | CPU %s| wifiIRQΔ %s| hz99 %s%s\n' \
    "$(ts)" "$N" "$PING" "$PSTATS" "${MEMAV:-?}" "$CPU" "${IRQD:-none }" "$HZ" "$RESTART_FLAG" >> "$LOG"

  # 6) Toutes les ~10 samples : temperature/throttling + top threads app
  if [ $((N % 10)) -eq 1 ]; then
    {
      echo "    temp/throttle: $(vcgencmd measure_temp 2>/dev/null) $(vcgencmd get_throttled 2>/dev/null)"
      echo "    top threads (PID $MAINPID):"
      ps -T -p "${MAINPID:-0}" -o tid,psr,rtprio,ni,pcpu,comm 2>/dev/null | tail -n +2 \
        | sort -k5 -nr | head -8 | sed 's/^/      /'
    } >> "$LOG"
  fi

  # 7) Nouvelles lignes dmesg (erreurs brcmfmac/timeout)
  NEWDM="$(dmesg 2>/dev/null | awk -v t="${LAST_DMESG_TS:-0}" \
           '{ ts=$1; gsub(/[][]/,"",ts); if (ts+0 > t+0) print }')"
  if [ -n "$NEWDM" ]; then
    echo "    >>> DMESG:" >> "$LOG"
    echo "$NEWDM" | sed 's/^/      /' >> "$LOG"
    LAST_DMESG_TS="$(dmesg 2>/dev/null | tail -1 | sed -E 's/^\[([0-9.]+)\].*/\1/')"
  fi

  sync 2>/dev/null           # flush disque : le log survit meme si le Pi se fige
  sleep "$INTERVAL"
done

sect "FIN - ETAT RESEAU FINAL"
{ ip -s link show 2>/dev/null; echo; echo "dmesg (queue) :"; dmesg 2>/dev/null | tail -40; } >> "$LOG"
echo "" >> "$LOG"
echo "--- journalctl arcadematrix (60 dernieres lignes : panics/restarts) ---" >> "$LOG"
journalctl -u arcadematrix --no-pager -n 60 2>/dev/null >> "$LOG" || \
  echo "  (journalctl indisponible)" >> "$LOG"
echo "" >> "$LOG"
echo "=== Diagnostic termine $(ts) ($N echantillons) ===" >> "$LOG"
echo "=== Redemarrages de l'app detectes pendant le test : $RESTARTS (PID initial ${FIRST_PID:-?} -> final ${MAINPID:-?}) ===" >> "$LOG"
sync 2>/dev/null
