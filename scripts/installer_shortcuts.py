# SPDX-License-Identifier: Apache-2.0
"""Package-local shell shortcuts; installed aliases belong to native setup."""
import re


def enabled(version):
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version):
        raise ValueError("Invalid shortcut package version")
    return tuple(map(int, version.split("."))) >= (1, 9, 0)


def portable(version, target):
    """Return exact name, bytes and executable mode, or no historical shortcut."""
    if not enabled(version):
        return None
    if target == "x86_64-pc-windows-msvc":
        return ("rk.cmd", b'@REM SPDX-License-Identifier: Apache-2.0\r\n@echo off\r\nsetlocal EnableExtensions DisableDelayedExpansion\r\nset "ERRORLEVEL="\r\n"%~dp0runtime\\airkctl.exe" %*\r\nexit /b %errorlevel%\r\n', False)
    if target not in ("x86_64-unknown-linux-gnu", "aarch64-apple-darwin", "x86_64-apple-darwin"):
        raise ValueError("Unsupported shortcut target")
    cli = "runtime/AIRK.app/Contents/MacOS/airkctl" if target.endswith("apple-darwin") else "runtime/airkctl"
    script = '#!/bin/sh\n# SPDX-License-Identifier: Apache-2.0\ncase "$0" in\n  */*) rk_dir=${0%/*} ;;\n  *) rk_dir=. ;;\nesac\nexec "$rk_dir/' + cli + '" "$@"\n'
    return ("rk", script.encode("utf-8"), True)
