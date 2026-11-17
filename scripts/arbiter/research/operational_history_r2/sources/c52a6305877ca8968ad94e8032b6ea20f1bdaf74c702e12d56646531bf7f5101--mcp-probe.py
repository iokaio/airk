# SPDX-License-Identifier: Apache-2.0
import json
import ssl
import subprocess
import urllib.request

ctl = r'C:\Users\tdotn\AppData\Local\AIRK Service\program\bin\airkctl.exe'
connection = '86169114369b81231ea81d52fdd50e26'
def cli(*args):
    return json.loads(subprocess.check_output([ctl, *args], text=True))

info = cli('agents', 'info', '--connection-id', connection, '--output', 'json')
headers = cli('agents', 'credential', '--connection-id', connection, '--format', 'header-json')
headers.update({'Content-Type': 'application/json', 'Accept': 'application/json, text/event-stream'})
ctx = ssl.create_default_context(cafile=info['ca_certificate_file'])
url = info['endpoint']['url']
def call(method, params, ident):
    body = {'jsonrpc': '2.0', 'method': method, 'params': params}
    if ident is not None:
        body['id'] = ident
    req = urllib.request.Request(url, json.dumps(body).encode(), headers, method='POST')
    with urllib.request.urlopen(req, context=ctx, timeout=30) as reply:
        sid = reply.headers.get('mcp-session-id')
        if sid:
            headers['mcp-session-id'] = sid
        raw = reply.read().decode()
        if not raw:
            return None
        if raw.startswith('event:') or raw.startswith('data:'):
            raw = next(line[6:] for line in raw.splitlines() if line.startswith('data: '))
        return json.loads(raw)

try:
    init = call('initialize', {'protocolVersion': '2025-11-25', 'capabilities': {}, 'clientInfo': {'name': 'airk-recovery-probe', 'version': '1'}}, 1)
    print(json.dumps({'initialize': init}))
    call('notifications/initialized', {}, None)
    catalog = call('tools/list', {}, 2)
    names = [tool['name'] for tool in catalog['result']['tools']]
    print(json.dumps({'tools': names}))
    status = call('tools/call', {'name': 'status', 'arguments': {}}, 3)
    print(json.dumps({'status': status}))
finally:
    if 'mcp-session-id' in headers:
        req = urllib.request.Request(url, headers=headers, method='DELETE')
        with urllib.request.urlopen(req, context=ctx, timeout=10) as reply:
            print(json.dumps({'session_close': reply.status}))
