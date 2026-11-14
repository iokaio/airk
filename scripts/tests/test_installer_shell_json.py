# SPDX-License-Identifier: Apache-2.0
"""Exercise actual shell serializer functions without native setup."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
GIT_BASH = Path('C:/Program Files/Git/bin/bash.exe')
SHELL = str(GIT_BASH) if os.name == 'nt' and GIT_BASH.is_file() else shutil.which('sh')
FIELDS = ['schema', 'operation_id', 'kind', 'created_unix_ms', 'expires_unix_ms', 'requested_by', 'acknowledgements', 'locations', 'payload_inventory_sha256', 'options']


def function(text, name):
    start = text.index(name + '() {')
    return text[start:text.index('\n}', start) + 2]


@unittest.skipUnless(SHELL, 'POSIX shell is unavailable')
class ShellCanonicalOperation(unittest.TestCase):
    def test_both_real_serializers_preserve_unicode_quotes_and_backslashes(self):
        for name, requested_by in [('installers/linux/install.sh.in', 'install-sh'), ('installers/macos/Install AIRK.command.in', 'dmg')]:
            with self.subTest(wrapper=name), tempfile.TemporaryDirectory(dir=ROOT / 'artifacts') as directory:
                root = Path(directory)
                (root / 'payload').mkdir()
                (root / 'payload/payload-inventory.json').write_bytes(b'{}')
                (root / 'work').mkdir()
                prefix = '/Users/fixture-\u00e9-\u65e5\u672c\u8a9e-\U0001f642'
                locations = {role: prefix + '/' + role + ' "quoted"' + chr(92) + 'literal' for role in ['program', 'data', 'state', 'cache', 'logs']}
                for role, value in locations.items():
                    (root / role).write_text(value, encoding='utf-8', newline='')
                text = (ROOT / name).read_text(encoding='utf-8')
                serializer = function(text, 'write_operation')
                encoder = text[text.index('json_path() {'):text.index('\n', text.index('json_path() {'))]
                assignments = '\n'.join('j_' + role + '=$(json_path "$(cat "' + (root / role).as_posix() + '")")' for role in locations)
                script = 'set -eu\numask 077\nexport LC_ALL=C\n' + encoder + '\n' + assignments + '\n' + serializer + '\nsequence=0\nwork="$CASE_WORK"\ninventory_sha256=' + ('5' * 64) + '\nwrite_operation install false false skip false\n'
                (root / 'case.sh').write_text(script, encoding='utf-8', newline='')
                result = subprocess.run([SHELL, (root / 'case.sh').as_posix()], cwd=root, env={**os.environ, 'CASE_WORK': (root / 'work').as_posix()}, capture_output=True, timeout=30)
                self.assertEqual(result.returncode, 0, result.stderr.decode('utf-8', 'replace'))
                raw = (root / 'work/operation-1/operation.json').read_bytes()
                operation = json.loads(raw)
                self.assertEqual(raw, (json.dumps(operation, ensure_ascii=False, separators=(',', ':')) + '\n').encode('utf-8'))
                self.assertEqual(list(operation), FIELDS)
                self.assertEqual(operation['locations'], locations)
                self.assertEqual(operation['requested_by'], requested_by)
                self.assertEqual(operation['options'], {'login_startup': False, 'trust': 'skip', 'start': False, 'delete_indexes': 'keep'})
                self.assertEqual((root / 'work/operation-1/operation-request.json').read_bytes(), raw)


@unittest.skipUnless(SHELL, 'POSIX shell is unavailable')
class ShellOriginalRecovery(unittest.TestCase):
    def test_native_location_scalar_preserves_trailing_lf_empty_and_failure(self):
        for wrapper in ['installers/linux/install.sh.in', 'installers/macos/Install AIRK.command.in']:
            for value in ['', '/original/root\n', '/original/root.\n\n', '/original/root.', '/original/\u00e9-\u65e5\U0001f642']:
                for code in [0, 73]:
                    with self.subTest(wrapper=wrapper, value=value, native_exit=code), tempfile.TemporaryDirectory(dir=ROOT / 'artifacts') as directory:
                        root = Path(directory)
                        (root / 'scalar').write_bytes(value.encode('utf-8'))
                        helper = function((ROOT / wrapper).read_text(encoding='utf-8'), 'recovery_location')
                        script = 'set -eu\nrecovery_field() { cat "$CASE_ROOT/scalar"; return '+str(code)+'; }\n'+helper+'\nprogram=unassigned\nif recovery_location program; then printf success > "$CASE_ROOT/result"; else printf "%s" "$?" > "$CASE_ROOT/result"; fi\nprintf "%s" "$program" > "$CASE_ROOT/observed"\n'
                        (root / 'case.sh').write_text(script, encoding='utf-8', newline='')
                        result = subprocess.run([SHELL, (root / 'case.sh').as_posix()], env={**os.environ, 'CASE_ROOT':root.as_posix()}, capture_output=True, timeout=30)
                        self.assertEqual(result.returncode, 0, result.stderr.decode('utf-8', 'replace'))
                        self.assertEqual((root / 'result').read_bytes(), b'success' if code == 0 else b'73')
                        self.assertEqual((root / 'observed').read_bytes(), value.encode('utf-8') if code == 0 else b'unassigned')

    def test_native_wrapper_lease_reexec_preserves_original_and_silent_flag(self):
        for wrapper in ['installers/linux/install.sh.in','installers/macos/Install AIRK.command.in']:
            for silent in [False,True]:
                with self.subTest(wrapper=wrapper,silent=silent), tempfile.TemporaryDirectory(dir=ROOT / 'artifacts') as directory:
                    root=Path(directory)
                    (root / 'native.sh').write_text('#!/bin/sh\nif [ "${5:-}" = --recovery-field ]; then printf false; else printf "%s\\n" "$@" > "$CASE_ROOT/observed-args"; fi\n',encoding='utf-8',newline='')
                    (root / 'native.sh').chmod(0o700)
                    text=(ROOT / wrapper).read_text(encoding='utf-8')
                    original='/private/original request é/operation-request.json'
                    script='set -eu\nwork="$CASE_ROOT"\nimage="$CASE_ROOT/native.sh"\nunattended='+('true' if silent else 'false')+'\npackage_dir="/verified original"\ninstaller_dir="/verified original"\nfail() { exit 81; }\n'+function(text,'lease_wrapper')+'\nlease_wrapper "'+original+'"\nexit 91\n'
                    (root / 'case.sh').write_text(script,encoding='utf-8',newline='')
                    result=subprocess.run([SHELL,(root / 'case.sh').as_posix()],env={**os.environ,'CASE_ROOT':root.as_posix()},capture_output=True,timeout=30)
                    self.assertEqual(result.returncode,0,result.stderr.decode('utf-8','replace'))
                    expected=['__airk_role','setup','--operation',original,'--wrapper-lease-script','/verified original/'+('Install AIRK.command' if 'macos' in wrapper else 'install.sh')]
                    if silent: expected.append('--wrapper-silent')
                    self.assertEqual((root / 'observed-args').read_text(encoding='utf-8').splitlines(),expected)

    def test_resume_refuses_option_overrides_before_native_execution(self):
        for wrapper in ['installers/linux/install.sh.in', 'installers/macos/Install AIRK.command.in']:
            with self.subTest(wrapper=wrapper), tempfile.TemporaryDirectory(dir=ROOT / 'artifacts') as directory:
                root=Path(directory)
                request=root / 'operation-request.json'
                request.write_bytes(b'opaque original request')
                for arguments in [['--no-start'], ['--program-dir', '/foreign'], ['--acknowledge-reset']]:
                    result=subprocess.run([SHELL, (ROOT / wrapper).as_posix(), '--resume', request.as_posix(), *arguments], capture_output=True, timeout=30)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn(b'overrides are forbidden', result.stderr)
                    self.assertEqual(request.read_bytes(), b'opaque original request')

    def test_native_removal_resume_reconciles_before_original_completion(self):
        for wrapper in ['installers/linux/install.sh.in', 'installers/macos/Install AIRK.command.in']:
            for completed in [False, True]:
                with self.subTest(wrapper=wrapper, completed=completed), tempfile.TemporaryDirectory(dir=ROOT / 'artifacts') as directory:
                    root=Path(directory)
                    original=b'opaque original removal request\n'
                    (root / 'operation-request.json').write_bytes(original)
                    fields={'kind':'uninstall', 'requested-by':'airkctl', 'inventory-sha256':'5'*64,
                            'wrapper-lease-held':'true', 'completion-required':'false' if completed else 'true'}
                    fields.update({role:'/original/'+role for role in ['program','data','state','cache','logs']})
                    for name,value in fields.items(): (root / name).write_text(value, encoding='utf-8', newline='')
                    (root / 'native.sh').write_text('#!/bin/sh\n[ "$4" = "$CASE_ROOT/operation-request.json" ] || exit 81\nif [ "${5:-}" = --recovery-field ]; then cat "$CASE_ROOT/$6"; else printf complete > "$CASE_ROOT/completed"; fi\n', encoding='utf-8', newline='')
                    (root / 'native.sh').chmod(0o700)
                    text=(ROOT / wrapper).read_text(encoding='utf-8')
                    start=text.index('recovery_field() {')
                    end=text.index('\nsupport=',start) if 'macos' in wrapper else text.index('\nd_program=',start)
                    script='set -eu\nfail() { echo "$*" >&2; exit 1; }\nwork="$CASE_ROOT"\nresume="$CASE_ROOT/operation-request.json"\nimage="$CASE_ROOT/native.sh"\ninventory_sha256='+'5'*64+'\n'+text[start:end]+'\nexit 91\n'
                    (root / 'case.sh').write_text(script,encoding='utf-8',newline='')
                    result=subprocess.run([SHELL,(root / 'case.sh').as_posix()],cwd=root,env={**os.environ,'CASE_ROOT':root.as_posix()},capture_output=True,timeout=30)
                    self.assertEqual(result.returncode,0,result.stderr.decode('utf-8','replace'))
                    self.assertEqual((root / 'completed').exists(),not completed)
                    self.assertEqual((root / 'operation-request.json').read_bytes(),original)

    def test_scalar_projection_preserves_original_and_completed_resume_skips_prepare(self):
        for wrapper, requested_by in [('installers/linux/install.sh.in', 'install-sh'), ('installers/macos/Install AIRK.command.in', 'dmg')]:
            for completed in [False, True]:
                with self.subTest(wrapper=wrapper, completed=completed), tempfile.TemporaryDirectory(dir=ROOT / 'artifacts') as directory:
                    root = Path(directory)
                    original = b'original operation bytes remain opaque to the wrapper\n'
                    (root / 'operation-request.json').write_bytes(original)
                    fields = {'requested-by': requested_by, 'kind': 'repair', 'inventory-sha256': '5' * 64, 'trust': 'skip',
                              'wrapper-lease-held':'true', 'completion-required':'false' if completed else 'true', 'placement-required': 'false' if completed else 'true', 'operation-id': '0e' * 16}
                    for role in ['program', 'data', 'state', 'cache', 'logs']:
                        fields[role] = '/Users/fixture-\u00e9-\u65e5\u672c\u8a9e-\U0001f642/' + role + ' \"quoted\"' + chr(92) + 'literal.\n\n'
                    for name, value in fields.items():
                        (root / name).write_text(value, encoding='utf-8', newline='')
                    stub = root / 'native.sh'
                    stub.write_text('#!/bin/sh\n[ "$1" = __airk_role ] && [ "$2" = setup ] && [ "$3" = --operation ] && [ "$4" = "$CASE_ROOT/operation-request.json" ] && [ "$5" = --recovery-field ] || exit 81\ncat "$CASE_ROOT/$6"\n', encoding='utf-8', newline='')
                    stub.chmod(0o700)
                    text = (ROOT / wrapper).read_text(encoding='utf-8')
                    start = text.index('recovery_field() {')
                    end = text.index('\nsupport=', start) if requested_by == 'dmg' else text.index('\nd_program=', start)
                    projection = text[start:end]
                    start = text.index('if [ -n "$resume" ]; then\n    operation=$resume')
                    end = text.index('\nelse\nwrite_operation', start)
                    prepare = text[start:end] + '\nfi\n'
                    script = 'set -eu\nfail() { echo "$*" >&2; exit 1; }\nwork="$CASE_ROOT"\nresume="$CASE_ROOT/operation-request.json"\nimage="$CASE_ROOT/native.sh"\nunattended=true\ninventory_sha256=' + '5' * 64 + '\n' + projection + '\nrun_setup() { printf "%s" "$operation" > "$CASE_ROOT/prepared"; }\nsetup_failure() { :; }\n' + prepare
                    script += '\nfor role in program data state cache logs; do eval "value=\\$$role"; printf "%s" "$value" > "$CASE_ROOT/observed-$role"; done\n'
                    (root / 'case.sh').write_text(script, encoding='utf-8', newline='')
                    result = subprocess.run([SHELL, (root / 'case.sh').as_posix()], cwd=root, env={**os.environ, 'CASE_ROOT': root.as_posix()}, capture_output=True, timeout=30)
                    self.assertEqual(result.returncode, 0, result.stderr.decode('utf-8', 'replace'))
                    self.assertEqual((root / 'operation-request.json').read_bytes(), original)
                    self.assertEqual((root / 'prepared').exists(), not completed)
                    if not completed:
                        self.assertEqual((root / 'prepared').read_text(), (root / 'operation-request.json').as_posix())
                    for role in ['program', 'data', 'state', 'cache', 'logs']:
                        self.assertEqual((root / ('observed-' + role)).read_text(encoding='utf-8'), fields[role])


if __name__ == '__main__':
    unittest.main()
