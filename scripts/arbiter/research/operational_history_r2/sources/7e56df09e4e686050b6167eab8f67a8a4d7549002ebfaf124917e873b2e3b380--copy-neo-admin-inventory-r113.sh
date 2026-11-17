# SPDX-License-Identifier: Apache-2.0
set -eu
umask 077
/Users/tylerjensen/Code/airk/artifacts/tools/neo-python-3.13.18/bin/python3.13 -I -B - <<'PY'
from pathlib import Path
import base64,hashlib,importlib.util,json,os
assert os.geteuid()==501
source=Path('/Users/tylerjensen/airk-builds/advanced-split-local-delivery-r1/source')
inputs=Path('/Users/tylerjensen/airk-builds/advanced-split-local-delivery-prerequisites-r1/inputs')
assert inputs.is_dir() and not inputs.is_symlink() and inputs.resolve()==inputs
assert source.is_dir() and not source.is_symlink() and source.resolve()==source
original_index=Path('/Users/tylerjensen/Code/airk/.git/index');before=hashlib.sha256(original_index.read_bytes()).hexdigest()
raw=base64.b64decode('ew0KICAic2NoZW1hIjogImFpcmstYWRtaW4tYXNzZXRzLzEiLA0KICAiZmlsZXMiOiBbDQogICAgew0KICAgICAgIm5hbWUiOiAiYXBwLjA2YzVkMTdkLmpzIiwNCiAgICAgICJzaGEyNTYiOiAiMDZjNWQxN2RlMTg2MDg0MGM1ODIzZjE1MDc3MzgyMmVjY2YwOGZhZWFjNDhiNDRjNDg4YzVkY2EwNjI3ZTAwNiIsDQogICAgICAiYnl0ZXMiOiAxMzIzNzYzLA0KICAgICAgImNvbnRlbnRfdHlwZSI6ICJ0ZXh0L2phdmFzY3JpcHQ7IGNoYXJzZXQ9dXRmLTgiDQogICAgfSwNCiAgICB7DQogICAgICAibmFtZSI6ICJhcHAuYjBkMTc5MzkuY3NzIiwNCiAgICAgICJzaGEyNTYiOiAiYjBkMTc5MzkyNDkwZTU2OTkyMzNjZWY4MDQ3ZDA2NTBlNGY0YWY2YTI1Y2NlOWFhMmZlZjU1NzY3ODJjYjA5NCIsDQogICAgICAiYnl0ZXMiOiAxMjAyNiwNCiAgICAgICJjb250ZW50X3R5cGUiOiAidGV4dC9jc3M7IGNoYXJzZXQ9dXRmLTgiDQogICAgfSwNCiAgICB7DQogICAgICAibmFtZSI6ICJpbmRleC5odG1sIiwNCiAgICAgICJzaGEyNTYiOiAiZTNhYzNlY2VhYzljOTdmNDIyZTE3ZWI5YWI3MGI4MjY5NjMwNzExMDc2MTYwNWI1MDk1YmZiMGRhZDRkNTZhNiIsDQogICAgICAiYnl0ZXMiOiA2MTAsDQogICAgICAiY29udGVudF90eXBlIjogInRleHQvaHRtbDsgY2hhcnNldD11dGYtOCINCiAgICB9LA0KICAgIHsNCiAgICAgICJuYW1lIjogImxhdW5jaC5odG1sIiwNCiAgICAgICJzaGEyNTYiOiAiZTNhYzNlY2VhYzljOTdmNDIyZTE3ZWI5YWI3MGI4MjY5NjMwNzExMDc2MTYwNWI1MDk1YmZiMGRhZDRkNTZhNiIsDQogICAgICAiYnl0ZXMiOiA2MTAsDQogICAgICAiY29udGVudF90eXBlIjogInRleHQvaHRtbDsgY2hhcnNldD11dGYtOCINCiAgICB9DQogIF0sDQogICJidW5kbGVfc2hhMjU2IjogImZmM2I4NTAwOGE0YjA2MDUwYTdhMDE4MDE1NDgyNTQyNDBiM2I2MGFjN2E1MDAyODIwMjFmYmI2YTMwMmRkNGQiLA0KICAidG9vbGNoYWluIjogew0KICAgICJub2RlIjogInYyNC4xNS4wIiwNCiAgICAibnBtIjogIjExLjEyLjEiDQogIH0NCn0NCg==');assert hashlib.sha256(raw).hexdigest()=='2390999ff5e181925e8037dd8ad7319e000ac0325c90a5b8dc34c350733b3150'
spec=importlib.util.spec_from_file_location('copied_admin_inventory_owner',source/'scripts/build-admin-assets.py');owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
block=owner.block_from_inventory(json.loads(raw))
handoff=inputs/'runtime-handoff-r1';assert not handoff.exists() and not handoff.is_symlink();handoff.mkdir(mode=0o700)
output=handoff/'admin-assets.json'
with output.open('xb') as stream:stream.write(raw)
assert hashlib.sha256(output.read_bytes()).hexdigest()=='2390999ff5e181925e8037dd8ad7319e000ac0325c90a5b8dc34c350733b3150'
assert hashlib.sha256(original_index.read_bytes()).hexdigest()==before
print(json.dumps({'result':'checked_admin_inventory_matches_copied_source','inventory':str(output),'inventory_file_sha256':'2390999ff5e181925e8037dd8ad7319e000ac0325c90a5b8dc34c350733b3150','admin_build_block':block,'original_git_index_unchanged':True,'no_build_install_or_service_change':True}))
PY
