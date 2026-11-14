# SPDX-License-Identifier: Apache-2.0
"""Acquire authenticated build-only model bytes, checked against the native catalog."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import stat
import tempfile
import urllib.error
import urllib.parse
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
API = "https://api.github.com/repos/iokaio/airk/releases/assets/"
MAX_ARCHIVE = 160 * 1024 * 1024


def inventory():
    spec = importlib.util.spec_from_file_location("model_input_policy", ROOT / "scripts/int8-policy.py")
    owner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(owner)
    catalog = owner.catalog()
    return {f"{profile['id']}/{asset['name']}": (asset['bytes'], asset['sha256'])
            for profile in catalog['profiles'] for asset in profile['assets']}


def verify(path, size, digest):
    if path.is_symlink() or not path.is_file() or path.stat().st_size != size:
        raise ValueError("Model input size or file type differs")
    with path.open('rb') as source:
        actual = hashlib.file_digest(source, 'sha256').hexdigest()
    if actual != digest:
        raise ValueError("Model input digest differs from native catalog")


def unpack(archive, destination, expected):
    destination = Path(destination)
    if destination.is_symlink():
        raise ValueError("Model destination must be unlinked")
    with zipfile.ZipFile(archive) as zipped:
        entries = zipped.infolist()
        if len(entries) != len(expected) or {entry.filename for entry in entries} != set(expected):
            raise ValueError("Model input archive has a different exact roster")
        with tempfile.TemporaryDirectory(prefix='airk-model-inputs-') as scratch:
            staged = Path(scratch)
            for entry in entries:
                size, digest = expected[entry.filename]
                if (not re.fullmatch(r'[a-z0-9-]+/(model\.onnx|tokenizer\.json|LICENSE)', entry.filename)
                        or entry.is_dir() or stat.S_ISLNK(entry.external_attr >> 16) or entry.file_size != size):
                    raise ValueError("Unsafe model input entry")
                path = staged / entry.filename
                path.parent.mkdir(parents=True, exist_ok=True)
                with zipped.open(entry) as source, path.open('xb') as output:
                    remaining = size
                    while block := source.read(min(1024 * 1024, remaining + 1)):
                        remaining -= len(block)
                        if remaining < 0:
                            raise ValueError("Model input exceeds pinned size")
                        output.write(block)
                verify(path, size, digest)
            # Validate every preexisting destination before publishing any new file.
            for name, (size, digest) in expected.items():
                path = destination / name
                if path.parent.is_symlink():
                    raise ValueError("Model profile directory must be unlinked")
                if path.exists() or path.is_symlink():
                    verify(path, size, digest)
            for name in expected:
                path = destination / name
                path.parent.mkdir(parents=True, exist_ok=True)
                if not path.exists():
                    with path.open('xb') as output, (staged / name).open('rb') as source:
                        shutil.copyfileobj(source, output)


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, message, headers, newurl):
        return None


def fetch(url, token, *, binary=False, opener=None):
    opener = opener or urllib.request.build_opener(NoRedirect())
    headers = {'Accept': 'application/octet-stream' if binary else 'application/vnd.github+json',
               'User-Agent': 'AIRK-model-build-inputs', 'X-GitHub-Api-Version': '2022-11-28'}
    if token:
        headers['Authorization'] = 'Bearer ' + token
    try:
        return opener.open(urllib.request.Request(url, headers=headers), timeout=120)
    except urllib.error.HTTPError as error:
        if binary and error.code in (301, 302, 303, 307, 308):
            redirected = error.headers.get('Location', '')
            parsed = urllib.parse.urlsplit(redirected)
            if (parsed.scheme != 'https' or parsed.hostname not in
                    ('release-assets.githubusercontent.com', 'objects.githubusercontent.com')
                    or parsed.username or parsed.password or parsed.port not in (None, 443)):
                raise ValueError('Untrusted model asset redirect') from None
            # The repository credential must never reach the asset CDN.
            try:
                return opener.open(urllib.request.Request(redirected, headers={'User-Agent': headers['User-Agent']}), timeout=120)
            except (urllib.error.URLError, OSError):
                raise ValueError('Official model asset CDN request failed') from None
        raise ValueError('Official model asset request failed') from None


def acquire(asset_id, destination, token, expected=None):
    expected = inventory() if expected is None else expected
    if not re.fullmatch(r'[1-9][0-9]*', str(asset_id)) or not token:
        raise ValueError('Authenticated official build-input asset ID is required')
    url = API + str(asset_id)
    with fetch(url, token) as response:
        data = response.read(1024 * 1024 + 1)
    if len(data) > 1024 * 1024:
        raise ValueError('Model asset metadata exceeds bound')
    asset = json.loads(data)
    size = asset.get('size')
    digest = asset.get('digest', '')
    if (type(asset.get('id')) is not int or asset['id'] != int(asset_id) or asset.get('url') != url
            or asset.get('state') != 'uploaded' or type(size) is not int or not 0 < size <= MAX_ARCHIVE
            or not re.fullmatch(r'sha256:[0-9a-f]{64}', digest)):
        raise ValueError('Official model asset metadata is incomplete or differs')
    with tempfile.TemporaryDirectory(prefix='airk-model-asset-') as scratch:
        path = Path(scratch) / 'inputs.zip'
        with fetch(url, token, binary=True) as response, path.open('xb') as output:
            count = 0
            while block := response.read(1024 * 1024):
                count += len(block)
                if count > size:
                    raise ValueError('Official model asset exceeds advertised bound')
                output.write(block)
        verify(path, size, digest[7:])
        unpack(path, destination, expected)
    return {'asset_id': int(asset_id), 'archive_sha256': digest[7:], 'files': sorted(expected)}


def bundle(source, output, expected=None):
    expected = inventory() if expected is None else expected
    for name, (size, digest) in expected.items():
        verify(Path(source) / name, size, digest)
    with zipfile.ZipFile(output, 'x', compression=zipfile.ZIP_STORED) as zipped:
        for name in sorted(expected):
            entry = zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0))
            entry.external_attr = (stat.S_IFREG | 0o644) << 16
            zipped.writestr(entry, (Path(source) / name).read_bytes())
    # A source may change after its first check. Verify the actual archive
    # bytes before returning a candidate digest, as every consumer also does.
    with zipfile.ZipFile(output) as zipped:
        for name, (size, digest) in expected.items():
            if zipped.getinfo(name).file_size != size:
                raise ValueError('Packaged model input size differs from native catalog')
            with zipped.open(name) as incoming:
                if hashlib.file_digest(incoming, 'sha256').hexdigest() != digest:
                    raise ValueError('Packaged model input digest differs from native catalog')
    return {'sha256': hashlib.sha256(Path(output).read_bytes()).hexdigest(),
            'bytes': Path(output).stat().st_size, 'files': sorted(expected)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    packaging = commands.add_parser('bundle')
    packaging.add_argument('--source', type=Path, required=True)
    packaging.add_argument('--output', type=Path, required=True)
    acquisition = commands.add_parser('acquire')
    acquisition.add_argument('--asset-id', required=True)
    acquisition.add_argument('--destination', type=Path, required=True)
    acquisition.add_argument('--token-file', type=Path)
    args = parser.parse_args()
    if args.command == 'bundle':
        result = bundle(args.source, args.output)
    else:
        token = args.token_file.read_text(encoding='utf-8').strip() if args.token_file else os.environ.get('GH_TOKEN', '')
        result = acquire(args.asset_id, args.destination, token)
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
