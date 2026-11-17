# SPDX-License-Identifier: Apache-2.0
set -eu
effective_uid=$(id -u)
human_uid=$(stat -f %u /dev/console)
printf 'effective_name=%s effective_uid=%s logged_in_name=%s logged_in_uid=%s\n' "$(id -un)" "$effective_uid" "$(stat -f %Su /dev/console)" "$human_uid"
[ "$effective_uid" = "$human_uid" ] && [ "$effective_uid" -ne 0 ] || exit 21
printf 'tool inventory\n'
for tool in python3 python3.11 python3.12 python3.13 node npm cmake clang codesign security git; do command -v "$tool" || true; done
for folder in "$HOME/.cargo" "$HOME/.rustup" "$HOME/airk-evidence" "$HOME/code/airk" /opt/homebrew/bin; do [ ! -e "$folder" ] || stat -f '%Su:%u %N' "$folder"; done
for binary in /opt/homebrew/bin/python3* /opt/homebrew/bin/node*; do [ ! -e "$binary" ] || ls -ld "$binary"; done
