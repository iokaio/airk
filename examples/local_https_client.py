# SPDX-License-Identifier: Apache-2.0
"""Read AIRK's local JSON API using the endpoint and CA the native service publishes.

No token is needed (U110). TLS with the installation's AIRK CA is still required,
and read tools need a scope per call: --index/--set or --workspace.
"""
import argparse
import hashlib
import http.client
import json
import os
from pathlib import Path
import re
import ssl
import stat

MAX_REPLY = 1024 * 1024


def private_bytes(filename, maximum):
    """Reject redirects/changes; the native service must have created private ACLs."""
    path = Path(filename)
    if not path.is_absolute() or path.resolve(strict=True) != path:
        raise ValueError("Expected an absolute native-owned file without redirects")
    before = path.lstat()
    if (not stat.S_ISREG(before.st_mode) or before.st_nlink != 1
            or before.st_size > maximum):
        raise ValueError("Invalid private file")
    if os.name != "nt" and (before.st_uid != os.geteuid() or before.st_mode & 0o077):
        raise ValueError("File is not private to this user")
    with path.open("rb") as source:
        opened = os.fstat(source.fileno())
        data = source.read(maximum + 1)
        after = os.fstat(source.fileno())
    identity = lambda value: (value.st_dev, value.st_ino, value.st_size, value.st_mtime_ns)
    if identity(before) != identity(opened) or identity(opened) != identity(after) or len(data) != before.st_size:
        raise ValueError("Private file changed while reading")
    return data


def scoped(arguments, indexes=(), sets=(), workspace=None):
    """Merge per-call scope selectors into the tool arguments (U110)."""
    if not isinstance(arguments, dict):
        raise ValueError("Use a JSON argument object")
    merged = dict(arguments)
    for key, values in (("index_ids", indexes), ("index_set_ids", sets)):
        if values:
            existing = merged.get(key, [])
            if not isinstance(existing, list):
                raise ValueError(key + " must be a list")
            merged[key] = existing + [value for value in values if value not in existing]
    if workspace is not None:
        if not os.path.isabs(workspace) or merged.get("workspace", workspace) != workspace:
            raise ValueError("Use one absolute workspace folder")
        merged["workspace"] = workspace
    return merged


def query(control_dir, tool=None, arguments=None):
    if not os.path.isabs(control_dir):
        raise ValueError("Expected the absolute service control directory")
    native = Path(control_dir) / "http-1"
    endpoint = json.loads(private_bytes(str(native / "endpoint.json"), 64 * 1024))
    if endpoint.get("schema") != 1 or endpoint.get("state") != "ready":
        raise ValueError("The native endpoint is not ready; start the AIRK service")
    match = re.fullmatch(r"https://127\.0\.0\.1:([1-9][0-9]{0,4})/mcp", endpoint.get("url", ""))
    if not match or not 1 <= int(match[1]) <= 65535:
        raise ValueError("Invalid loopback endpoint")
    pem = private_bytes(str(native / "ca.pem"), 8192).decode("ascii")
    if pem.count("-----BEGIN CERTIFICATE-----") != 1:
        raise ValueError("Expected the native installation's single CA certificate")
    der = ssl.PEM_cert_to_DER_cert(pem)
    if hashlib.sha256(der).hexdigest() != endpoint.get("ca_sha256"):
        raise ValueError("Native CA identity changed")
    context = ssl.create_default_context()
    context.load_verify_locations(cadata=pem)  # Add trust; preserve normal roots and IP verification.
    route, method, body = "/api/v1/tools", "GET", None
    headers = {}  # U110: no Authorization header.
    if tool is not None:
        if not re.fullmatch(r"[A-Za-z0-9_]{1,64}", tool) or not isinstance(arguments, dict):
            raise ValueError("Use an advertised tool name and JSON argument object")
        route, method = route + "/" + tool, "POST"
        body = json.dumps(arguments).encode("utf-8")
        if len(body) > 65536:
            raise ValueError("Request exceeds the transport budget")
        headers["Content-Type"] = "application/json"
    client = http.client.HTTPSConnection("127.0.0.1", int(match[1]), context=context, timeout=125)
    try:
        client.connect()  # Validate TLS before sending the request. No redirects are followed.
        client.request(method, route, body=body, headers=headers)
        response = client.getresponse()
        data = response.read(MAX_REPLY + 1)
        if len(data) > MAX_REPLY:
            raise ValueError("Response exceeds the transport budget")
        return response.status, json.loads(data)
    finally:
        client.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--control-dir", required=True,
                        help="Absolute AIRK service control directory (holds http-1/endpoint.json and http-1/ca.pem)")
    parser.add_argument("--tool", help="Omit to list supported tools and complete schemas")
    parser.add_argument("--arguments", default="{}", help="JSON argument object")
    parser.add_argument("--index", action="append", default=[], help="Index id or name (repeatable); sent as index_ids")
    parser.add_argument("--set", action="append", default=[], help="Index set id or name (repeatable); sent as index_set_ids")
    parser.add_argument("--workspace", help="Absolute folder whose Explorer-applied default scope applies")
    args = parser.parse_args()
    arguments = json.loads(args.arguments)
    if args.tool is not None:
        arguments = scoped(arguments, args.index, args.set, args.workspace)
    elif args.index or args.set or args.workspace:
        parser.error("scope options need --tool")
    status, result = query(args.control_dir, args.tool, arguments)
    print(json.dumps(result, indent=2))
    return 0 if status == 200 and not result.get("isError", False) else 1


if __name__ == "__main__":
    raise SystemExit(main())
