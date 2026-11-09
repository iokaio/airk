# SPDX-License-Identifier: Apache-2.0
"""Names and publication boundary shared by installer assembly and downloads."""
import re

TARGETS = {
    "x86_64-pc-windows-msvc": ("win32-x64", "exe", "windows_inno"),
    "aarch64-apple-darwin": ("darwin-arm64", "dmg", "macos_dmg"),
    "x86_64-apple-darwin": ("darwin-x64", "dmg", "macos_dmg"),
    "x86_64-unknown-linux-gnu": ("linux-x64", "tar.gz", "linux_tar"),
}


def required(version, channel):
    if not re.fullmatch(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)", version):
        raise ValueError("Invalid installer package version")
    if channel not in ("stable", "preview"):
        raise ValueError("Invalid installer channel")
    return channel == "stable" and tuple(map(int, version.split("."))) >= (1, 8, 14)


def artifact(version, target):
    return f"airk-{version}-{target}-installer.{TARGETS[target][1]}"


def manifest(version, target):
    return f"airk-{version}-{target}-installer.manifest.json"
