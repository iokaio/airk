# SPDX-License-Identifier: Apache-2.0
"""Precreate and stop this fresh owned installation so Code gets its CA at startup."""
import argparse
import importlib.util
import json
from pathlib import Path

root=Path(__file__).resolve().parents[4]
spec=importlib.util.spec_from_file_location('clients',root/'scripts/check-service-clients.py')
clients=importlib.util.module_from_spec(spec);spec.loader.exec_module(clients)
parser=argparse.ArgumentParser();parser.add_argument('--runtime',type=Path,required=True);parser.add_argument('--owner',type=Path,required=True)
args=parser.parse_args()
trust_spec=importlib.util.spec_from_file_location('code_user_trust',Path(__file__).with_name('code-user-trust.py'))
trust=importlib.util.module_from_spec(trust_spec);trust_spec.loader.exec_module(trust)
fixture=clients.Fixture(args.runtime,args.owner,command_timeout=300)
fixture.install=args.owner/'native-service'
with fixture:
    ca=fixture.control/'http-1/ca.pem'
    assert ca.is_file()
    trust.install(fixture,args.owner)
assert fixture.managed_stop['record']['phase']=='complete'
print(json.dumps({'ca_file':str(ca),'native_stop_complete':True,'os_user_trust_installed':True}))
