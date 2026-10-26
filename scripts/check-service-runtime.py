# SPDX-License-Identifier: Apache-2.0
"""Exercise an explicitly selected native pair offline in disposable service scopes."""
import argparse
import importlib.util
import json
import os
from pathlib import Path

spec = importlib.util.spec_from_file_location("airk_runtime_clients", Path(__file__).with_name("check-service-clients.py"))
clients = importlib.util.module_from_spec(spec)
spec.loader.exec_module(clients)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    args = parser.parse_args()
    # Affect this fixture process and its children only; setup has no downloader.
    for name in ("HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY"):
        os.environ[name] = "http://127.0.0.1:1"
    print(json.dumps(clients.qualify(args.runtime, files=1, model=True), indent=2))


if __name__ == "__main__":
    main()
