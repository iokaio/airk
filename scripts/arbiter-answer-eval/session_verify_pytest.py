# SPDX-License-Identifier: Apache-2.0
"""Run the selected public pytest selectors in the edited checkout. No shell is involved."""
from __future__ import annotations

import json
import os
from pathlib import Path
import sys


def main():
    work, scratch = Path.cwd().resolve(), Path(os.environ["TEMP"]).resolve()
    selectors = json.load(sys.stdin)
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    # Import the checked installation's test framework before adding the edited checkout.
    import pytest
    import pytest_asyncio.plugin

    sys.path.insert(0, str(work / "clients/python/src"))
    # These hooks bound ordinary Python I/O in this trusted offline evaluation.
    # CPython audit hooks are not an operating-system sandbox for hostile native code.
    read_roots = [work, scratch, Path(sys.prefix).resolve(), Path(sys.base_prefix).resolve()]
    def checked_path(value, write):
        if isinstance(value, int) or value is None:
            return
        if os.fsdecode(value).lower() in (os.devnull.lower(), r"\\.\nul"):
            return
        path = Path(os.fsdecode(value)).resolve()
        if not any(path.is_relative_to(root) for root in ([scratch] if write else read_roots)):
            raise PermissionError(f"Offline test access outside approved roots: {path}")

    def audit(event, args):
        if event == "open":
            flags = args[2]
            checked_path(args[0], bool(flags & (os.O_CREAT | os.O_WRONLY | os.O_RDWR | os.O_TRUNC | os.O_APPEND)))
        elif event in ("os.remove", "os.rmdir", "os.mkdir", "os.chmod", "os.utime", "os.truncate"):
            checked_path(args[0], True)
        elif event in ("os.rename", "os.link", "os.symlink"):
            checked_path(args[0], True)
            checked_path(args[1], True)
        elif event in ("os.listdir", "os.scandir"):
            checked_path(args[0] if args else ".", False)
        elif event in ("subprocess.Popen", "os.system", "os.startfile", "os.exec", "os.spawn", "os.posix_spawn",
                       "socket.connect", "socket.bind", "socket.sendto", "socket.getaddrinfo",
                       "ctypes.dlopen", "ctypes.dlsym") or event.startswith("winreg."):
            raise PermissionError(f"Offline test operation unavailable: {event}")

    config = scratch / "pytest.ini"
    config.write_text("[pytest]\n", encoding="utf-8")
    sys.addaudithook(audit)
    # The frozen public Python client config sets asyncio_mode=auto. Supplying the reviewed
    # options explicitly avoids discovering mutable plugins or configuration in parent repos.
    return pytest.main(["-c", str(config), "--rootdir", str(work), "--noconftest", "-p", "no:cacheprovider",
                        "-o", "asyncio_mode=auto", "--tb=short", "-q", "--", *selectors], plugins=[pytest_asyncio.plugin])


if __name__ == "__main__":
    raise SystemExit(main())