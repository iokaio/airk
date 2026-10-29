# SPDX-License-Identifier: Apache-2.0
# Each RUN can receive an empty or restored cache, independently of earlier RUNs.
set -eu
for cache in /usr/local/cargo/registry /usr/local/cargo/git /build/target; do
    test -d "$cache" && test ! -L "$cache"
    chown -R --no-dereference 10001:10001 "$cache"
done
# Ownership tests and compilation must still execute as the ordinary build user.
exec runuser --user airkbuild -- "$@"
