# SPDX-License-Identifier: Apache-2.0
set -eu
e=$(id -u); r=$(id -ru); h=$(stat -f %u /dev/console)
[ "$e" = 501 ] && [ "$r" = 501 ] && [ "$e" = "$h" ] && [ "$e" -ne 0 ] || exit 21
export PYTHONDONTWRITEBYTECODE=1
exec /Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/python/cpython-3.13.18-macos-aarch64-none/bin/python3.13 -I -B /Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/evidence/platform-admin-repair-launch-r1/neo-next-integrated-platform-batch-r1.py /Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/evidence/platform-admin-repair-launch-r1/frozen-neo-config-r1.json
