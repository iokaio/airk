# SPDX-License-Identifier: Apache-2.0
"""Read AIRK's local JSON API using a connection saved by verified native setup."""
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
    """Reject redirects/changes; native setup must have created private ACLs."""
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


def query(connection_file, tool=None, arguments=None):
    connection = json.loads(private_bytes(connection_file, 60 * 1024))
    endpoint = connection["endpoint"]
    if connection["schema"] != 1 or endpoint["schema"] != 1 or endpoint["state"] != "ready":
        raise ValueError("Obtain a current ready connection from native AIRK setup")
    match = re.fullmatch(r"https://127\.0\.0\.1:([1-9][0-9]{0,4})/mcp", endpoint["url"])
    if not match or not 1 <= int(match[1]) <= 65535:
        raise ValueError("Invalid loopback endpoint")
    pem = private_bytes(connection["ca_certificate_file"], 8192).decode("ascii")
    if pem.count("-----BEGIN CERTIFICATE-----") != 1:
        raise ValueError("Expected the native installation's single CA certificate")
    der = ssl.PEM_cert_to_DER_cert(pem)
    if hashlib.sha256(der).hexdigest() != endpoint["ca_sha256"]:
        raise ValueError("Native CA identity changed")
    token = private_bytes(connection["credential_file"], 103).decode("ascii")
    if not re.fullmatch(r"airk1\.[0-9a-f]{32}\.[0-9a-f]{64}", token) or token.split(".")[1] != connection["connection_id"]:
        raise ValueError("Invalid native credential identity")
    context = ssl.create_default_context()
    context.load_verify_locations(cadata=pem)  # Add trust; preserve normal roots and IP verification.
    route, method, body = "/api/v1/tools", "GET", None
    headers = {"Authorization": "Bearer " + token}
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
        client.connect()  # Validate TLS before sending credentials. No redirects are followed.
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
    parser.add_argument("--connection", required=True, help="Protected JSON from verified native setup")
    parser.add_argument("--tool", help="Omit to list supported tools and complete schemas")
    parser.add_argument("--arguments", default="{}", help="JSON argument object; never a bearer token")
    args = parser.parse_args()
    status, result = query(args.connection, args.tool, json.loads(args.arguments))
    print(json.dumps(result, indent=2))
    return 0 if status == 200 and not result.get("isError", False) else 1


if __name__ == "__main__":
    raise SystemExit(main())
