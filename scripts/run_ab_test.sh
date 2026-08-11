#!/usr/bin/env bash
# =============================================================================
# ArcadeMatrix - Test A/B automatique (bus mémoire vs pulsing matériel)
# -----------------------------------------------------------------------------
# PROBLEME : impossible d'enchainer des commandes en SSH, car activer le pulsing
# HW tue le Wi-Fi et gele la session SSH. Ce script fait TOUT en tache detachee,
# survit a la chute du SSH, puis REMET le pulsing OFF a la fin pour que le Wi-Fi
# revienne et qu'on puisse lire les logs.
#
# Deroulement (~6 min, sans intervention) :
#   1. Test A : rotation=date  (statique, bus mémoire au repos) + pulsing ON
#              -> echantillonnage 120s -> diag_A_date.log
#   2. Test B : rotation=gifs  (charge le bus mémoire)          + pulsing ON
#              -> echantillonnage 120s -> diag_B_gifs.log
#   3. Restauration : pulsing OFF (+ rotation d'origine) -> Wi-Fi revient
#
# USAGE (sur le Pi) :
#   cd ~/ArcadeMatrix_RPi
#   sudo bash scripts/run_ab_test.sh
#   -> affiche les chemins puis rend la main. Le SSH va tomber : c'est normal.
#   -> reconnecte-toi ~6 min plus tard, puis :
#        grep -c PERTE ~/diag_A_date.log ~/diag_B_gifs.log
# =============================================================================
set -u

# --- Reglages ----------------------------------------------------------------
SAMPLE="${SAMPLE:-120}"                 # duree d'echantillonnage par manche (s)
SERVICE="${SERVICE:-arcadematrix}"
SETTLE="${SETTLE:-12}"                  # attente apres restart avant d'echantillonner
OUTDIR="${OUTDIR:-/home/$USER}"
DIAG="$(cd "$(dirname "$0")" && pwd)/wifi_diag.sh"

# --- Localiser conf.ini ------------------------------------------------------
CONF="${CONF:-}"
if [ -z "$CONF" ]; then
  for p in "/home/$USER/ArcadeMatrix_RPi/conf.ini" \
           "$(dirname "$0")/../conf.ini" \
           "/root/ArcadeMatrix_RPi/conf.ini"; do
    [ -f "$p" ] && { CONF="$p"; break; }
  done
fi

# --- Auto-daemonisation : survit a la chute du SSH ---------------------------
if [ "${_AB_DETACHED:-0}" != "1" ]; then
  if [ -z "$CONF" ] || [ ! -f "$CONF" ]; then
    echo "ERREUR: conf.ini introuvable. Definis CONF=/chemin/conf.ini"; exit 1
  fi
  if [ ! -f "$DIAG" ]; then
    echo "ERREUR: wifi_diag.sh introuvable a cote ($DIAG)"; exit 1
  fi
  export _AB_DETACHED=1 SAMPLE SERVICE SETTLE OUTDIR DIAG CONF
  setsid bash "$0" >/dev/null 2>&1 < /dev/null &
  child=$!
  ORIG_ROT="$(grep -E '^rotation=' "$CONF" | head -1 | cut -d= -f2)"
  cat <<EOF
===================================================================
 Test A/B lance en tache detachee (PID $child).
   conf.ini : $CONF
   rotation d'origine : ${ORIG_ROT:-inconnue}  (sera restauree a la fin)
   Manche A (date) puis B (gifs), 120s chacune, pulsing ON.
   A la fin : pulsing OFF -> le Wi-Fi revient tout seul.
-------------------------------------------------------------------
 Ta session SSH VA GELER quand le pulsing s'active : c'est attendu.
 Reconnecte-toi dans ~6 minutes, puis :
   grep -c PERTE $OUTDIR/diag_A_date.log $OUTDIR/diag_B_gifs.log
 Suivi de progression (une fois reconnecte) :
   cat $OUTDIR/ab_test_progress.log
 Pour tout arreter : sudo kill $child
===================================================================
EOF
  exit 0
fi

# =============================================================================
# ----------------------------- Corps detache ---------------------------------
# =============================================================================
PROG="$OUTDIR/ab_test_progress.log"
ts(){ date '+%Y-%m-%d %H:%M:%S'; }
log(){ echo "[$(ts)] $*" >> "$PROG"; sync 2>/dev/null; }

: > "$PROG"
ORIG_ROT="$(grep -E '^rotation=' "$CONF" | head -1 | cut -d= -f2)"
ORIG_PULSE="$(grep -E '^disable_hardware_pulsing=' "$CONF" | head -1 | cut -d= -f2)"
log "Debut. rotation d'origine=$ORIG_ROT pulsing_d'origine=$ORIG_PULSE"

set_conf(){ # $1=cle $2=valeur
  if grep -qE "^$1=" "$CONF"; then
    sed -i -E "s/^$1=.*/$1=$2/" "$CONF"
  else
    # ajoute sous [matrix]
    sed -i "/^\[matrix\]/a $1=$2" "$CONF"
  fi
}

run_manche(){ # $1=label $2=rotation $3=logfile
  local label="$1" rot="$2" out="$3"
  log ">>> Manche $label : pulsing ON, rotation=$rot"
  set_conf disable_hardware_pulsing false
  set_conf rotation "$rot"
  sync 2>/dev/null
  systemctl restart "$SERVICE" >/dev/null 2>&1
  log "    service redemarre, attente ${SETTLE}s de stabilisation..."
  sleep "$SETTLE"
  log "    echantillonnage ${SAMPLE}s -> $out"
  # Appel SYNCHRONE du diag (bloque jusqu'a la fin grace a _DIAG_DETACHED=1)
  _DIAG_DETACHED=1 LOG="$out" DURATION="$SAMPLE" INTERVAL=2 CONF="$CONF" \
    bash "$DIAG" >/dev/null 2>&1
  local n; n="$(grep -c PERTE "$out" 2>/dev/null)"
  log "    Manche $label terminee. Lignes PERTE=$n"
}

run_manche "A_date" "date" "$OUTDIR/diag_A_date.log"
run_manche "B_gifs" "gifs" "$OUTDIR/diag_B_gifs.log"

# --- Restauration : pulsing OFF pour ravoir le Wi-Fi -------------------------
log "<<< Restauration : pulsing OFF, rotation=${ORIG_ROT:-gifs}"
set_conf disable_hardware_pulsing true
set_conf rotation "${ORIG_ROT:-gifs}"
sync 2>/dev/null
systemctl restart "$SERVICE" >/dev/null 2>&1
sleep 5

A="$(grep -c PERTE "$OUTDIR/diag_A_date.log" 2>/dev/null)"
B="$(grep -c PERTE "$OUTDIR/diag_B_gifs.log" 2>/dev/null)"
log "TERMINE. PERTE  A_date=$A   B_gifs=$B"
log "Interpretation : A~0 & B>>0 => contention bus (fix logiciel). A deja eleve => pulsing matériel (dongle)."
sync 2>/dev/null
