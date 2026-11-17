# SPDX-License-Identifier: Apache-2.0
effective_uid=$(id -u) || exit 1
human_uid=$(stat -f %u /dev/console) || exit 1
printf 'effective_name=%s effective_uid=%s logged_in_name=%s logged_in_uid=%s\n' "$(id -un)" "$effective_uid" "$(stat -f %Su /dev/console)" "$human_uid"
[ "$effective_uid" = "$human_uid" ] && [ "$effective_uid" -ne 0 ] || exit 21
cd "$HOME/code/airk" || exit 1
git status --porcelain
printf 'target_owner='; stat -f '%Su:%u' target 2>/dev/null || true
printf 'cargo='; "$HOME/.cargo/bin/cargo" --version
printf 'node='; node --version
printf 'python='; python3 --version
