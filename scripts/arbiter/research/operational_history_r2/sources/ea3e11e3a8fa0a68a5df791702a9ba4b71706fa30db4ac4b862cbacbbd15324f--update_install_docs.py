# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p = Path('docs/service-installation.md')
lines = p.read_text(encoding='utf-8').splitlines()
for i, line in enumerate(lines):
    if line.startswith('`airk runtime location [--channel'):
        lines[i] = '`airk runtime location [--channel stable|preview]` reports native user defaults without creating directories. Fresh installations use `runtime`, separate `index-v1` data and `service-v1` state leaves; preview appends `-preview`. See [storage formats](storage-formats.md) for the full mapping. Existing installations at historical `service-1.4` locations retain their bound paths instead of silently creating a second service or relocating owned data. Explicit install paths remain available to trusted user/machine deployment policy; preview live-data cutover and managed policy presentation remain open.'
    if line.startswith('The report includes distinct configuration, data, state, cache and log defaults.'):
        lines[i] = line.replace('On macOS, configuration, data and state use separate `AIRK/configuration/service-1.4`, `AIRK/data/service-1.4` and `AIRK/state/service-1.4`', 'On macOS, fresh configuration, data and state use separate `AIRK/configuration/settings`, `AIRK/data/index-v1` and `AIRK/state/service-v1`').replace('Log/cache defaults use', 'Existing legacy layouts keep their recorded leaves. Log/cache defaults use')
p.write_text('\n'.join(lines)+'\n', encoding='utf-8', newline='')
