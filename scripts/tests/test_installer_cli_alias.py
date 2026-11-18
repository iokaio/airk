# SPDX-License-Identifier: Apache-2.0
"""Exercise Unix installer alias dispatch without running native setup."""
import os
from pathlib import Path
import shutil
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[2]
BASH = Path('C:/Program Files/Git/bin/bash.exe')
SHELL = str(BASH) if os.name == 'nt' and BASH.is_file() else shutil.which('bash')
WRAPPERS = ('installers/linux/install.sh.in', 'installers/macos/Install AIRK.command.in')


def function(text, name):
    start = text.index(name + '() {')
    return text[start:text.index('\n}', start) + 2]


def shell(script, arguments=()):
    return subprocess.run([SHELL, '--noprofile', '--norc', '-c', script, 'case', *arguments],
                          capture_output=True, text=True, encoding='utf-8', timeout=15)


@unittest.skipUnless(SHELL, 'Bash unavailable')
class InstallerAlias(unittest.TestCase):
    def test_templates_parse_and_capture_original_path_before_reset(self):
        for name in WRAPPERS:
            text = (ROOT / name).read_text(encoding='utf-8')
            result = subprocess.run([SHELL, '-n'], input=text, capture_output=True,
                                    text=True, encoding='utf-8', timeout=15)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertLess(text.index('invoking_path=${PATH-}'), text.index('PATH=/usr/bin:'))
            self.assertIn('cat "$program/airkctl-aliases.txt"', text)
            self.assertIn("printf 'Canonical CLI: %s\\n'", text)
            self.assertNotIn('Use rk (', text)
            self.assertNotIn('case :$invoking_path: in', text)
            self.assertIn('If these commands are not found, add ~/.local/bin to PATH', text)

    def select(self, name, mode, interactive, statuses, reply='', native_exit=0):
        text = (ROOT / name).read_text(encoding='utf-8')
        # The mock is a shell function: no program, model, file or setup is run.
        script = r'''
set -eu
fail() { printf '%s\n' "$*" >&2; exit 81; }
image=native operation='/original plan' invoking_path='/caller tools:/other'
alias_mode=$1 interactive=$2 alias_name=chosen statuses=$3 native_exit=$4
native() {
    [ "$*" = "__airk_role setup --operation /original plan --probe-cli-alias --cli-alias $alias_name --cli-alias-search-path /caller tools:/other" ] || exit 79
    [ "$native_exit" = 0 ] || return "$native_exit"
    case $alias_name in rk|chosen) printf '%s' "$statuses" ;; custom) printf available ;; *) printf collision ;; esac
}
'''
        script += function(text, 'choose_cli_alias')
        # A here-document provides interactive input without a filesystem fixture.
        script += '\nchoose_cli_alias <<\'REPLY\'\n' + reply + '\nREPLY\nprintf "%s:%s" "$alias_mode" "$alias_name"\n'
        return shell(script, [mode, 'true' if interactive else 'false', statuses, str(native_exit)])

    def test_interactive_default_custom_and_none(self):
        for name in WRAPPERS:
            for status in ('available', 'owned'):
                r = self.select(name, 'auto', True, status)
                self.assertEqual((r.returncode, r.stdout), (0, 'name:rk'), r.stderr)
            r = self.select(name, 'auto', True, 'collision', 'custom')
            self.assertEqual((r.returncode, r.stdout), (0, 'name:custom'), r.stderr)
            r = self.select(name, 'auto', True, 'collision')
            self.assertEqual((r.returncode, r.stdout), (0, 'none:'), r.stderr)

    def test_unattended_auto_and_no_alias_do_not_probe_or_guess(self):
        for name in WRAPPERS:
            for mode in ('auto', 'none'):
                r = self.select(name, mode, False, 'unusable', native_exit=77)
                self.assertEqual((r.returncode, r.stdout), (0, mode + ':chosen'), r.stderr)

    def test_explicit_collision_unknown_scalar_and_native_failure_refuse(self):
        for name in WRAPPERS:
            for interactive in (True, False):
                r = self.select(name, 'name', interactive, 'collision')
                self.assertEqual(r.returncode, 81)
            for status, code in [('unknown', 0), ('available', 73)]:
                r = self.select(name, 'auto', True, status, native_exit=code)
                self.assertEqual(r.returncode, 81)

    def test_alias_overrides_refused_on_resume_before_filesystem_or_native(self):
        for name in WRAPPERS:
            text = (ROOT / name).read_text(encoding='utf-8')
            for args in [('--cli-alias', 'custom'), ('--no-cli-alias',)]:
                r = shell(text, ['--resume', '/missing-original-request', *args])
                self.assertNotEqual(r.returncode, 0)
                self.assertIn('overrides are forbidden', r.stderr)
            for args in [('--cli-alias', ''), ('--no-cli-alias', '--cli-alias', 'custom')]:
                r = shell(text, args)
                self.assertNotEqual(r.returncode, 0)
                self.assertTrue('nonempty' in r.stderr or 'only one' in r.stderr)

    def test_lease_seals_choice_only_before_first_reexec(self):
        # Root must be explicitly supplied by the same-user guarded test launcher.
        work = os.environ.get('AIRK_INSTALLER_ALIAS_TEST_ROOT')
        self.assertIsNotNone(work, 'guard and supply AIRK_INSTALLER_ALIAS_TEST_ROOT')
        for name in WRAPPERS:
            text = (ROOT / name).read_text(encoding='utf-8')
            for mode, flags in [('auto', ['--auto-cli-alias', '--cli-alias-search-path', '/original path:/bin']),
                                ('name', ['--cli-alias', 'custom', '--cli-alias-search-path', '/original path:/bin']),
                                ('none', ['--no-cli-alias'])]:
                for fresh in (True, False):
                    script = r'''
set -eu
work=$1 image=native unattended=true alias_mode=$2 alias_name=custom
invoking_path='/original path:/bin' package_dir='/package' installer_dir='/package'
fail() { exit 81; }
native() { printf false; }
exec() { printf '%s\n' "$@"; exit 0; }
'''
                    script += function(text, 'lease_wrapper') + '\nlease_wrapper "/original operation"' + (' fresh' if fresh else '')
                    r = shell(script, [Path(work).as_posix(), mode])
                    expected = ['native', '__airk_role', 'setup', '--operation', '/original operation',
                                '--wrapper-lease-script', '/package/' + ('Install AIRK.command' if 'macos' in name else 'install.sh')]
                    if fresh:
                        expected += flags
                    expected += ['--wrapper-silent']
                    self.assertEqual((r.returncode, r.stdout.splitlines()), (0, expected), r.stderr)
            self.assertIn('lease_wrapper "$operation" fresh', text)
            self.assertIn('lease_wrapper "$resume"', text)


if __name__ == '__main__':
    unittest.main()
