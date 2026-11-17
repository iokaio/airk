# SPDX-License-Identifier: Apache-2.0
import hashlib
from pathlib import Path
base=Path('artifacts/advanced-split')
shell=base/'local-delivery-preparation-r1/root_only_mac_r3_batch.sh'
assert hashlib.sha256(shell.read_bytes()).hexdigest()=='0e9b8009f2cf3a8aec10c5c3c390fa42deef56e482b163b7e60cdc09dae8351a'
assert b'\r' not in shell.read_bytes()
script=(base/'run-mac-recovery-r125.py').read_text(encoding='utf-8')
script=script.replace('root_only_mac_r2_batch.sh','root_only_mac_r3_batch.sh').replace('77cdf32fa9f22c810a8360fa4cfec0ff26d5730add8874c52f3e33854af4598f','0e9b8009f2cf3a8aec10c5c3c390fa42deef56e482b163b7e60cdc09dae8351a').replace('mac-recovery-r125','mac-recovery-r128')
(base/'run-mac-recovery-r128.py').write_text(script,encoding='utf-8')
