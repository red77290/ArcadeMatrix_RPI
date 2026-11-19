#!/bin/sh
# ArcadeMatrix daemon launcher & persistent watchdog (Recalbox & RetroPie).
# Ensures arcadematrix_daemon.py is running as a double-forked daemon AND keeps a single
# lightweight background supervisor alive to auto-restart it within 2s if ever killed.

if [ -d "/opt/retropie/configs/all" ] && [ ! -d "/recalbox/share" ]; then
    DEFAULT_LOG_FILE="/opt/retropie/configs/all/daemon.log"
    DEFAULT_DAEMON_PY="/opt/retropie/configs/all/arcadematrix_daemon.py"
    DEFAULT_SELF_SCRIPT="/opt/retropie/configs/all/arcadematrix_launcher.sh"
else
    DEFAULT_LOG_FILE="/recalbox/share/userscripts/daemon.log"
    DEFAULT_DAEMON_PY="/recalbox/share/arcadematrix_daemon.py"
    DEFAULT_SELF_SCRIPT="/recalbox/share/userscripts/arcadematrix_launcher(permanent).sh"
fi

PID_FILE="${AM_PID_FILE:-/tmp/arcadematrix_daemon.pid}"
SUP_PID_FILE="${AM_SUP_PID_FILE:-/tmp/arcadematrix_supervisor.pid}"
STOP_FILE="${AM_STOP_FILE:-/tmp/arcadematrix_daemon.stop}"
LOG_FILE="${AM_LOG_FILE:-$DEFAULT_LOG_FILE}"
DAEMON_PY="${AM_DAEMON_PY:-$DEFAULT_DAEMON_PY}"
SELF_SCRIPT="${AM_SELF_SCRIPT:-$DEFAULT_SELF_SCRIPT}"
NL='
'

is_pid_alive() {
    [ -f "$1" ] || return 1
    _pid="$(cat "$1" 2>/dev/null)"
    [ -n "$_pid" ] && kill -0 "$_pid" 2>/dev/null
}

# Strictly matches ONLY a python* interpreter executing arcadematrix_daemon.py
# (never false-matches sh/bash/ssh/grep wrappers whose command line mentions the filename).
is_python_daemon_proc() {
    _pdir="$1"
    [ -r "$_pdir/cmdline" ] || return 1
    _lines="$(tr '\0' '\n' < "$_pdir/cmdline" 2>/dev/null)" || return 1
    [ -n "$_lines" ] || return 1
    _a0="${_lines%%"$NL"*}"
    case "${_a0##*/}" in
        python*) ;;
        *) return 1 ;;
    esac
    case "$_lines" in
        *arcadematrix_daemon.py*) return 0 ;;
    esac
    return 1
}

is_daemon_running() {
    if [ -f "$PID_FILE" ]; then
        _dpid="$(cat "$PID_FILE" 2>/dev/null)"
        if [ -n "$_dpid" ] && kill -0 "$_dpid" 2>/dev/null; then
            if [ ! -d "/proc/$_dpid" ] || is_python_daemon_proc "/proc/$_dpid"; then
                return 0
            fi
        fi
    fi
    for _pdir in /proc/[0-9]*; do
        [ -d "$_pdir" ] || continue
        if is_python_daemon_proc "$_pdir"; then
            return 0
        fi
    done
    return 1
}

start_daemon_once() {
    if is_daemon_running; then
        return 0
    fi
    python3 "$DAEMON_PY" --daemon --log-file "$LOG_FILE" </dev/null >/dev/null 2>&1
}

if [ "${1:-}" = "--supervisor-loop" ]; then
    trap "" HUP TERM INT
    echo "$$" > "$SUP_PID_FILE"
    while [ ! -f "$STOP_FILE" ]; do
        start_daemon_once
        sleep 2
    done
    rm -f "$SUP_PID_FILE"
    exit 0
fi

case " $* " in
    *" -action shutdown "*|*" -action reboot "*|*" -action stop "*|*" -action quit "*)
        exit 0
        ;;
esac

[ -f "$STOP_FILE" ] && exit 0

start_daemon_once

if ! is_pid_alive "$SUP_PID_FILE"; then
    if command -v setsid >/dev/null 2>&1; then
        setsid sh "$SELF_SCRIPT" --supervisor-loop </dev/null >/dev/null 2>&1 &
    else
        nohup sh "$SELF_SCRIPT" --supervisor-loop </dev/null >/dev/null 2>&1 &
    fi
fi

exit 0
