# SPDX-License-Identifier: Apache-2.0
#!/bin/bash
# SPDX-License-Identifier: Apache-2.0
set -u
RUN=/Users/tylerjensen/airk-builds/status-1.8.3-20261101
PREVIOUS=/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P
export PATH="$PREVIOUS/tools:$PREVIOUS/tools/node-v24.15.0-darwin-arm64/bin:$HOME/.cargo/bin:$PATH"
cd "$RUN/source" || exit 1
python -m unittest discover -s scripts/tests -v > "$RUN/python-contracts-reviewed.log" 2>&1
printf 'python-contracts-reviewed %s\n' "$?" >> "$RUN/reviewed-results.txt"
# The existing native build/qualification owns the Mac's heavy work first.
for attempt in $(seq 1 360); do
    if [ "$(cat "$RUN/phase.txt")" = complete ]; then break; fi
    sleep 5
done
if [ "$(cat "$RUN/phase.txt")" != complete ]; then
    printf 'extension-reviewed prerequisite-timeout\n' >> "$RUN/reviewed-results.txt"
    exit 1
fi
export AIRK_TEST_BINARY="$RUN/source/artifacts/runtime/aarch64-apple-darwin/AIRK.app/Contents/MacOS/airk"
if [ ! -f "$AIRK_TEST_BINARY" ]; then
    printf 'extension-reviewed native-build-unavailable\n' >> "$RUN/reviewed-results.txt"
    exit 1
fi
npm test --prefix packages/vscode > "$RUN/extension-reviewed.log" 2>&1
result=$?
printf 'extension-reviewed %s\n' "$result" >> "$RUN/reviewed-results.txt"
exit "$result"
