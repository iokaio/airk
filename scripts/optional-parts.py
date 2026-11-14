# SPDX-License-Identifier: Apache-2.0
"""Acquire one native-pinned optional release part from authenticated build storage."""
import argparse
import importlib.util
import json
import os
from pathlib import Path


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    owner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(owner)
    return owner


publication = load('optional_part_publication', 'optional-publication.py')
transport = load('optional_part_transport', 'model-build-inputs.py')


def acquire(name, destination, nomination, token, owner):
    if name not in owner['assets'] or type(nomination) is not int or nomination <= 0 or not token:
        raise ValueError('Optional build-input nomination is missing or not in the native catalog')
    pin = owner['assets'][name]
    url = transport.API + str(nomination)
    with transport.fetch(url, token) as response:
        encoded = response.read(1024 * 1024 + 1)
    if len(encoded) > 1024 * 1024:
        raise ValueError('Optional build-input metadata exceeds bound')
    asset = json.loads(encoded)
    if (type(asset.get('id')) is not int or asset['id'] != nomination or asset.get('url') != url
            or asset.get('name') != name or asset.get('state') != 'uploaded'
            or type(asset.get('size')) is not int or asset['size'] != pin['bytes']
            or asset.get('digest') != 'sha256:' + pin['sha256']):
        raise ValueError('Optional official build-input metadata differs from native pins')
    destination = Path(destination)
    destination.mkdir(parents=True, exist_ok=False)
    path = destination / name
    with transport.fetch(url, token, binary=True) as response, path.open('xb') as output:
        count = 0
        while block := response.read(1024 * 1024):
            count += len(block)
            if count > pin['bytes']:
                raise ValueError('Optional build-input exceeds native pinned size')
            output.write(block)
        output.flush()
        os.fsync(output.fileno())
    publication.verify_part(path, pin)
    return {'name': name, 'bytes': pin['bytes'], 'sha256': pin['sha256'], 'asset_id': nomination}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--name', required=True)
    parser.add_argument('--tag', required=True)
    parser.add_argument('--destination', type=Path, required=True)
    args = parser.parse_args()
    encoded = os.environ.get('AIRK_ARBITER_BUILD_INPUT_ASSETS', '')
    if not encoded or len(encoded.encode()) > 65536:
        raise ValueError('Authenticated optional build-input asset nominations are unavailable')
    nominations = json.loads(encoded)
    if not isinstance(nominations, dict):
        raise ValueError('Optional build-input nominations require an exact name/ID map')
    owner = publication.projection(args.tag)
    if set(nominations) != set(owner['assets']):
        raise ValueError('Optional build-input nominations differ from the native exact part roster')
    result = acquire(args.name, args.destination, nominations[args.name], os.environ.get('GH_TOKEN', ''), owner)
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
