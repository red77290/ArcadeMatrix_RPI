#!/bin/sh
# Ultra-light EmulationStation transition self-healer (< 0.5 ms fast path via tmpfs PID check).
if [ -f /tmp/arcadematrix_daemon.pid ]; then
    _pid="$(cat /tmp/arcadematrix_daemon.pid 2>/dev/null)"
    if [ -n "$_pid" ] && kill -0 "$_pid" 2>/dev/null; then
        exit 0
    fi
fi
[ -f /tmp/arcadematrix_daemon.stop ] && exit 0
sh "/recalbox/share/userscripts/arcadematrix_launcher(permanent).sh" "$@" >/dev/null 2>&1 &
exit 0
