# SPDX-License-Identifier: Apache-2.0
"""Explicit parent-operated P14 signed Neo preflight/launch/collect. Never auto-launch."""
import argparse, base64, hashlib, importlib.util, json, re, shlex, subprocess, tempfile
from pathlib import Path
BASE = Path(__file__).resolve().parent
PYTHON = '/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools/python3.13'
def sha(p): return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def read(p): return json.loads(Path(p).read_text(encoding='utf-8-sig'))
def fresh(p, value):
    with Path(p).open('x', encoding='utf-8') as output: output.write(json.dumps(value, indent=2) + '\n')

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['prepare', 'preflight', 'launch-build', 'launch', 'sign-existing', 'collect'])
    parser.add_argument('--label', default='p14-native-signed-quota-r1')
    args = parser.parse_args(); assert re.fullmatch(r'p14-native-signed-quota-[a-z0-9-]+', args.label)
    spec = importlib.util.spec_from_file_location('prepare', BASE / 'p14-mac-signed-prepare-r2.py')
    prepare = importlib.util.module_from_spec(spec); spec.loader.exec_module(prepare)
    plan = prepare.prepare(args.label)
    if args.action == 'prepare': print(json.dumps(plan, indent=2)); return
    connection = read(BASE / 'neo-connection.local.json')
    peer = connection['user'] + '@' + connection['host']
    ssh = ['ssh', '-i', connection['identity_path'], *connection['ssh_options'], peer]
    scp = ['scp', '-i', connection['identity_path'], *connection['ssh_options']]
    remote = plan['remote']; preflight = BASE / (args.label + '-preflight.json')
    def remote_python(code):
        encoded = base64.b64encode(code.encode()).decode()
        return subprocess.run([*ssh, PYTHON + ' -c ' + shlex.quote('import base64;exec(base64.b64decode(' + repr(encoded) + '))')],
                              capture_output=True, text=True, timeout=60)
    if args.action == 'preflight':
        assert not preflight.exists()
        config = read(plan['config_template'])
        code = """import json,os,hashlib,shutil,subprocess
from pathlib import Path
remote=Path(REMOTE)
assert not remote.exists(), 'Fresh remote run already exists'
uid=os.getuid();assert uid==501==Path('/dev/console').stat().st_uid
helper=Path(HELPER);assert helper.is_file() and not helper.is_symlink() and helper.stat().st_uid==uid
keys=[str(Path.home()/'Library/Keychains/airk-signing.keychain-db'),str(Path.home()/'Library/Keychains/login.keychain-db')]
metadata=subprocess.run([str(helper),*keys],capture_output=True,text=True,timeout=15)
identities=subprocess.run(['/usr/bin/security','find-identity','-v','-p','codesigning'],capture_output=True,text=True,timeout=15)
assert identities.returncode==metadata.returncode==0
identity='4D8496DCAB73794378194D040D9177FB28A0F982 "Developer ID Application: Ioka LLC (5PX7D99PXA)"'
assert identity in identities.stdout
free=shutil.disk_usage(remote.parent).free;assert free>=MINIMUM
print(json.dumps({'passed':True,'remote':str(remote),'uid':uid,'console_uid':Path('/dev/console').stat().st_uid,'free_bytes':free,'identity_stdout':identities.stdout,'identity_stderr':identities.stderr,'keychain_metadata':metadata.stdout,'keychain_stderr':metadata.stderr,'keychain_helper':str(helper),'keychain_helper_sha256':hashlib.sha256(helper.read_bytes()).hexdigest(),'target_absent':not (remote/'target').exists(),'notarization_requested':False}))
""".replace('REMOTE', repr(remote)).replace('HELPER', repr(config['keychain_helper'])).replace('MINIMUM', str(plan['minimum_free_bytes']))
        result = remote_python(code)
        assert result.returncode == 0, result.stdout + result.stderr
        value = json.loads(result.stdout); value.update(prepared_sha256=sha(BASE / (args.label + '-signed-prepared.json')))
        fresh(preflight, value); print(json.dumps(value, indent=2)); return
    if args.action in ['launch', 'launch-build']:
        proof = read(preflight); assert proof['passed'] and proof['remote'] == remote
        assert proof['prepared_sha256'] == sha(BASE / (args.label + '-signed-prepared.json'))
        config = read(plan['config_template']); config['keychain_helper_sha256'] = proof['keychain_helper_sha256']
        local_config = BASE / (args.label + '-sign-config.json'); fresh(local_config, config)
        subprocess.run([*ssh, 'umask 077; mkdir ' + shlex.quote(remote)], check=True)
        for name, local in {**plan['assets'], 'sign-config.json': str(local_config)}.items():
            subprocess.run([*scp, local, peer + ':' + remote + '/' + name], check=True)
        mode = 'build-only' if args.action == 'launch-build' else 'full'
        command = 'cd ' + shlex.quote(remote) + ' && nohup ' + PYTHON + ' owner.py ' + mode + ' > owner-' + mode + '.log 2>&1 < /dev/null &'
        subprocess.run([*ssh, command], check=True)
        fresh(BASE / (args.label + '-active.json'), {'remote': remote, 'mode': mode, 'source': plan['source'], 'preflight_sha256': sha(preflight)})
        print(args.action, remote); return
    if args.action == 'sign-existing':
        result = remote_python('from pathlib import Path\nimport json\nr=Path(' + repr(remote) + ')\np=json.loads((r/"unsigned-qualification.json").read_text())\nassert p["status"]=="passed" and p["target_removed"] and not (r/"target").exists()\nassert not (r/"signed-qualification.json").exists()\nprint("producer qualified; target absent")')
        assert result.returncode == 0, result.stdout + result.stderr
        command = 'cd ' + shlex.quote(remote) + ' && nohup ' + PYTHON + ' owner.py sign-existing > owner-sign-existing.log 2>&1 < /dev/null &'
        subprocess.run([*ssh, command], check=True)
        fresh(BASE / (args.label + '-sign-active.json'), {'remote': remote, 'mode': 'sign-existing', 'source': plan['source']})
        print('sign-existing', remote); return
    # Immutable explicit collection of public test logs/receipts only; no keys/user state.
    result = remote_python("from pathlib import Path\nimport json\nr=Path(" + repr(remote) + ")\nassert not (r/'target').exists()\nfiles=[p.relative_to(r).as_posix() for p in r.iterdir() if p.is_file() and p.suffix in ['.json','.log','.stdout','.stderr']]\nfiles += [p.relative_to(r).as_posix() for p in (r/'cargo-timings').glob('*.html')]\nfor p in [r/'source/artifacts/p14-mac-developerid-r1/signing.json',r/'source/artifacts/p14-mac-release-r1/airk-build.json']:\n if p.is_file():files.append(p.relative_to(r).as_posix())\nprint(json.dumps(files))")
    assert result.returncode == 0, result.stdout + result.stderr
    for name in json.loads(result.stdout):
        assert not Path(name).is_absolute() and '..' not in Path(name).parts
        dest = BASE / (args.label + '-collected-' + name.replace('/', '_'))
        with tempfile.TemporaryDirectory(prefix='p14-signed-collect-', dir=BASE) as tmp:
            current = Path(tmp) / 'download'; subprocess.run([*scp, peer + ':' + remote + '/' + name, str(current)], check=True)
            if dest.exists(): assert dest.read_bytes() == current.read_bytes(), 'Immutable receipt changed: ' + name
            else:
                with dest.open('xb') as output: output.write(current.read_bytes())
    print('collected', remote)

if __name__ == '__main__': main()
