#!/bin/sh
# ArcadeMatrix daemon launcher & persistent watchdog (Recalbox).
# Ensures arcadematrix_daemon.py is running as a double-forked daemon AND keeps a single
# lightweight background supervisor alive to auto-restart it within 2s if ever killed.

PID_FILE="/tmp/arcadematrix_daemon.pid"
SUP_PID_FILE="/tmp/arcadematrix_supervisor.pid"
STOP_FILE="/tmp/arcadematrix_daemon.stop"
LOG_FILE="/recalbox/share/userscripts/daemon.log"
DAEMON_PY="/recalbox/share/arcadematrix_daemon.py"

case " $* " in
    *" -action shutdown "*|*" -action reboot "*|*" -action stop "*|*" -action quit "*)
        exit 0
        ;;
esac

[ -f "$STOP_FILE" ] && exit 0

is_pid_alive() {
    [ -f "$1" ] || return 1
    _pid="$(cat "$1" 2>/dev/null)"
    [ -n "$_pid" ] && kill -0 "$_pid" 2>/dev/null
}

is_daemon_running() {
    if is_pid_alive "$PID_FILE"; then
        return 0
    fi
    for _c in /proc/[0-9]*/cmdline; do
        [ -r "$_c" ] || continue
        case "$(tr '\0' ' ' < "$_c" 2>/dev/null)" in
            *arcadematrix_daemon.py*) return 0 ;;
        esac
    done
    (ps ax 2>/dev/null || ps -ef 2>/dev/null || ps w 2>/dev/null || ps) | grep -q '[a]rcadematrix_daemon.py'
}

start_daemon_once() {
    if is_daemon_running; then
        return 0
    fi
    python3 "$DAEMON_PY" --daemon --log-file "$LOG_FILE" </dev/null >/dev/null 2>&1
}

start_daemon_once

if ! is_pid_alive "$SUP_PID_FILE"; then
    _sup_body='
        trap "" HUP TERM INT
        echo "$$" > "'"$SUP_PID_FILE"'"
        while [ ! -f "'"$STOP_FILE"'" ]; do
            _dpid="$(cat "'"$PID_FILE"'" 2>/dev/null)"
            if [ -z "$_dpid" ] || ! kill -0 "$_dpid" 2>/dev/null; then
                _found=0
                for _c in /proc/[0-9]*/cmdline; do
                    [ -r "$_c" ] || continue
                    case "$(tr "\0" " " < "$_c" 2>/dev/null)" in
                        *arcadematrix_daemon.py*) _found=1; break ;;
                    esac
                done
                if [ "$_found" = "0" ]; then
                    python3 "'"$DAEMON_PY"'" --daemon --log-file "'"$LOG_FILE"'" </dev/null >/dev/null 2>&1
                fi
            fi
            sleep 2
        done
        rm -f "'"$SUP_PID_FILE"'"
    '
    if command -v setsid >/dev/null 2>&1; then
        setsid sh -c "$_sup_body" </dev/null >/dev/null 2>&1 &
    else
        nohup sh -c "$_sup_body" </dev/null >/dev/null 2>&1 &
    fi
fi

exit 0
