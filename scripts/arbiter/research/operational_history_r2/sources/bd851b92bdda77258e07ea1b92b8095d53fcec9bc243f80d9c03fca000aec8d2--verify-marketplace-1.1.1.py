# SPDX-License-Identifier: Apache-2.0
import importlib.util
import json
from pathlib import Path
spec = importlib.util.spec_from_file_location('release_version', Path('scripts/release-version.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
platforms = module.market_platforms(module.marketplace(), '1.1.1', 'stable')
print(json.dumps({'version': '1.1.1', 'channel': 'stable', 'platforms': sorted(platforms)}))
assert platforms == module.PLATFORMS, 'Marketplace has not returned all four stable platforms'
