# SPDX-License-Identifier: Apache-2.0
"""Build AIRK's static downloads page from published GitHub release metadata.

Split releases (split_receipts.split_version) present two steps: Install AIRK Service (the
per-platform installer and its service pair ZIP) and Install VS Code Extension (one universal
VSIX). Their assets are advertised only after each one's GitHub-computed digest agrees with the
release's own published build record (release.json), whose bytes are fetched and checked too.
"""
import argparse
import hashlib
import importlib.util
from datetime import datetime
from html import escape
import json
from pathlib import Path
import re
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent.parent
REPOSITORY = "iokaio/airk"
GITHUB = f"https://github.com/{REPOSITORY}"
PLATFORMS = (
    ("win32-x64", "x86_64-pc-windows-msvc", "Windows", "x64", "WIN"),
    ("darwin-arm64", "aarch64-apple-darwin", "macOS", "Apple silicon", "MAC / ARM"),
    ("darwin-x64", "x86_64-apple-darwin", "macOS", "Intel x64", "MAC / X64"),
    ("linux-x64", "x86_64-unknown-linux-gnu", "Linux", "x64", "LINUX"),
)
PRODUCT_SCREENSHOTS = {"workbench-1.7.1-results.png": "workbench", "explorer-1.7.1.png": "explorer"}
NUMBER = r"(?:0|[1-9][0-9]*)"
TAG = re.compile(rf"v({NUMBER}\.{NUMBER}\.{NUMBER})(?:-build\.([1-9][0-9]*))?")
_spec = importlib.util.spec_from_file_location("standalone_contract", ROOT / "scripts/standalone-installer-contract.py")
standalone = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(standalone)
_spec = importlib.util.spec_from_file_location("split_receipts", ROOT / "scripts/split_receipts.py")
receipts = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(receipts)
RECORD_FILES = {"release.json", "release-notes.md", "SHA256SUMS"}
RECORD_BYTES = 1 << 20


def installer_names(version):
    return {platform: standalone.artifact(version, target) for platform, target, *_ in PLATFORMS}


def installer_manifest_names(version):
    return {platform: standalone.manifest(version, target) for platform, target, *_ in PLATFORMS}


def release_metadata(release, *, require_installers=False, record=None):
    """Accept only complete versioned releases with direct AIRK asset URLs.

    `record` is the release's own published release.json bytes; split releases advertise their
    assets only when it is given and agrees with every asset's GitHub digest."""
    tag = release["tag_name"]
    match = TAG.fullmatch(tag)
    version = match[1]
    published = datetime.fromisoformat(release["published_at"].replace("Z", "+00:00"))
    if published.tzinfo is None or not isinstance(release["prerelease"], bool):
        raise ValueError(f"{tag}: invalid publication metadata")
    if receipts.split_version(version):
        return split_metadata(release, tag, version, published, record)
    expected = {"release.json", "release-notes.md", "SHA256SUMS"}
    for platform, target, *_ in PLATFORMS:
        expected.update((f"airk-{version}-{platform}.vsix", f"airk-{version}-{target}.zip",
                         f"airk-{version}-{platform}.vsix-manifest.json",
                         f"airk-{version}-{target}.native-manifest.json"))
    assets = release["assets"]
    names = {asset["name"] for asset in assets}
    installer_assets, installer_manifests = installer_names(version), installer_manifest_names(version)
    optional = set(installer_assets.values()) | set(installer_manifests.values())
    if not expected <= names or names - expected - optional or len(names) != len(assets):
        raise ValueError(f"{tag}: incomplete or unexpected release assets")
    for platform, name in installer_assets.items():
        if (name in names) != (installer_manifests[platform] in names):
            raise ValueError(f"{tag}: incomplete standalone installer assets")
    if require_installers and standalone.required(version, "preview" if release["prerelease"] else "stable"):
        if not optional <= names:
            raise ValueError(f"{tag}: incomplete required standalone installer assets")
    for asset in assets:
        expected_url = f"{GITHUB}/releases/download/{tag}/{asset['name']}"
        if (asset.get("state") != "uploaded"
                or asset.get("browser_download_url") != expected_url
                or not re.fullmatch(r"sha256:[0-9a-f]{64}", asset.get("digest") or "")
                or not isinstance(asset.get("size"), int) or asset["size"] <= 0):
            raise ValueError(f"{tag}: invalid asset URL, state, size or digest")
    by_name = {asset["name"]: asset for asset in assets}
    installers = {platform: {"url": by_name[name]["browser_download_url"],
                             "manifest": by_name[installer_manifests[platform]]["browser_download_url"]}
                  for platform, name in installer_assets.items() if name in by_name}
    return {"tag": tag, "version": version, "published": published, "installers": installers,
            "channel": "preview" if release["prerelease"] else "stable"}


def checked_assets(tag, assets):
    for asset in assets:
        expected_url = f"{GITHUB}/releases/download/{tag}/{asset['name']}"
        if (asset.get("state") != "uploaded"
                or asset.get("browser_download_url") != expected_url
                or not re.fullmatch(r"sha256:[0-9a-f]{64}", asset.get("digest") or "")
                or not isinstance(asset.get("size"), int) or asset["size"] <= 0):
            raise ValueError(f"{tag}: invalid asset URL, state, size or digest")
    return {asset["name"]: asset for asset in assets}


def split_metadata(release, tag, version, published, record):
    """The exact split catalog (split_receipts.release_assets), verified against release.json."""
    channel = "preview" if release["prerelease"] else "stable"
    expected = receipts.release_assets(version, channel) | RECORD_FILES
    assets = release["assets"]
    names = {asset["name"] for asset in assets}
    if names != expected or len(names) != len(assets):
        raise ValueError(f"{tag}: incomplete or unexpected release assets")
    by_name = checked_assets(tag, assets)
    verified = False
    if record is not None:
        if len(record) > RECORD_BYTES or by_name["release.json"]["digest"] != "sha256:" + hashlib.sha256(record).hexdigest():
            raise ValueError(f"{tag}: release.json differs from its published digest")
        document = json.loads(record)
        published_assets = document.get("assets") if isinstance(document, dict) else None
        if (not isinstance(document, dict) or document.get("schema") != "airk-github-release/1" or document.get("tag") != tag
                or document.get("package_version") != version or document.get("channel") != channel
                or not isinstance(published_assets, dict) or set(published_assets) != expected - RECORD_FILES
                or any(by_name[name]["digest"] != "sha256:" + str(value) for name, value in published_assets.items())):
            raise ValueError(f"{tag}: published assets differ from the release build record")
        verified = True
    installers = {}
    if verified:
        for platform, target, *_ in PLATFORMS:
            name = standalone.artifact(version, target)
            if name in by_name:
                installers[platform] = {"url": by_name[name]["browser_download_url"],
                                        "manifest": by_name[standalone.manifest(version, target)]["browser_download_url"]}
    vsix = f"airk-{version}.vsix"
    extension = {"url": by_name[vsix]["browser_download_url"],
                 "manifest": by_name[vsix + "-manifest.json"]["browser_download_url"]} if verified else None
    return {"tag": tag, "version": version, "published": published, "installers": installers,
            "channel": channel, "split": True, "verified": verified, "extension": extension}


def published_record(release):
    """The release's own release.json bytes, downloaded through the authenticated GitHub CLI."""
    asset = next(asset for asset in release["assets"] if asset["name"] == "release.json")
    if not isinstance(asset.get("id"), int) or not 0 < asset.get("size", 0) <= RECORD_BYTES:
        raise ValueError(f"{release['tag_name']}: invalid release.json asset")
    return subprocess.run(["gh", "api", "-H", "Accept: application/octet-stream",
                           f"repos/{REPOSITORY}/releases/assets/{asset['id']}"],
                          check=True, capture_output=True, timeout=120).stdout


def select_releases(catalog, fetch_record=None):
    """Use the most recently published versioned release in each channel."""
    selected = {}
    for release in catalog:
        if release.get("draft") is not False or not TAG.fullmatch(release.get("tag_name", "")):
            continue
        # An incomplete versioned publication is an error, never a silent fallback.
        split = receipts.split_version(TAG.fullmatch(release["tag_name"])[1])
        record = release_metadata(release, record=fetch_record(release) if split and fetch_record else None)
        channel = record["channel"]
        if channel not in selected or record["published"] > selected[channel]["published"]:
            selected[channel] = record
    return [selected[channel] for channel in ("stable", "preview") if channel in selected]


def render_downloads(records):
    if not records:
        return f"""<div class="pending-release">
        <h3>The first download is on its way.</h3>
        <p>Packaged downloads will appear here after the first GitHub release is published. You can explore the source and build instructions now.</p>
        <a class="text-link" href="{GITHUB}">Explore AIRK on GitHub <span aria-hidden="true">↗</span></a>
      </div>"""
    sections = []
    for record in records:
        if record.get("split"):
            sections.append(render_split(record))
            continue
        tag, version, channel = (record[key] for key in ("tag", "version", "channel"))
        base = f"{GITHUB}/releases/download/{tag}"
        cards = []
        for platform, target, system, architecture, mark in PLATFORMS:
            installer = record["installers"].get(platform)
            signing = "Unsigned · extract and run install.sh" if system == "Linux" else (
                "Signed and notarized · disk image" if system == "macOS" else "Signed · Windows installer")
            if installer:
                label = escape(f"Download {channel} CLI and MCP installer for {system} {architecture}")
                download = f'<a class="button primary" aria-label="{label}" href="{escape(installer["url"], quote=True)}">Download installer <span aria-hidden="true">↓</span></a>'
                provenance = f'<a class="native-link" href="{escape(installer["manifest"], quote=True)}">Installer build and signing record</a>'
                if system == "Linux":
                    signing = 'Unsigned · <a href="https://github.com/iokaio/airk/blob/main/docs/release-verification.md#artifact-attestations">Verify build provenance</a>'
            else:
                download = '<span class="button unavailable-download" aria-disabled="true">Installer not published</span>'
                provenance = ''
            cards.append(f"""<article class="download-card">
            <span class="os-mark" aria-hidden="true">{mark}</span>
            <h4>{system}</h4><p>{architecture}</p>
            <p class="installer-signing">{signing}</p>
            {download}
            {provenance}
            <a class="native-link" aria-label="Download {channel} portable runtime ZIP for {system} {architecture}" href="{base}/airk-{version}-{target}.zip">Portable runtime ZIP</a>
          </article>""")
        date = record["published"].date().isoformat()
        note = ("Preview builds are for early testing. " if channel == "preview" else "")
        sections.append(f"""<section class="release-block" aria-labelledby="{channel}-title">
        <div class="release-heading">
          <h3 id="{channel}-title"><span class="badge {channel}">{channel.capitalize()}</span> {tag}</h3>
          <p>Package {version} · Published <time datetime="{date}">{date}</time></p>
        </div>
        <div class="download-grid">{''.join(cards)}</div>
        <div class="release-links">
          <a href="{GITHUB}/releases/tag/{tag}">Release notes ↗</a>
          <a href="{base}/SHA256SUMS">SHA-256 checksums ↓</a>
          <a href="{base}/release.json">Build record ↓</a>
        </div>
        <p class="release-note">{note}Standalone installers provide the native CLI and MCP runtime without a VSIX. “Installer not published” means this release has no standalone installer for that platform; its portable runtime ZIP remains available. Package version: {version}.</p>
      </section>""")
    return "\n".join(sections)


def render_split(record):
    """Two steps: Install AIRK Service (per platform), then Install VS Code Extension (universal)."""
    tag, version, channel = (record[key] for key in ("tag", "version", "channel"))
    base = f"{GITHUB}/releases/download/{tag}"
    cards = []
    for platform, target, system, architecture, mark in PLATFORMS:
        installer = record["installers"].get(platform)
        signing = "Unsigned · extract and run install.sh" if system == "Linux" else (
            "Signed and notarized · disk image" if system == "macOS" else "Signed · Windows installer")
        if installer:
            label = escape(f"Download {channel} AIRK Service installer for {system} {architecture}")
            download = f'<a class="button primary" aria-label="{label}" href="{escape(installer["url"], quote=True)}">Download installer <span aria-hidden="true">↓</span></a>'
            provenance = f'<a class="native-link" href="{escape(installer["manifest"], quote=True)}">Installer build and signing record</a>'
            if system == "Linux":
                signing = 'Unsigned · <a href="https://github.com/iokaio/airk/blob/main/docs/release-verification.md#artifact-attestations">Verify build provenance</a>'
        else:
            state = "Installer not published" if record["verified"] else "Installer not verified"
            download = f'<span class="button unavailable-download" aria-disabled="true">{state}</span>'
            provenance = ''
        pair = (f'<a class="native-link" aria-label="Download {channel} AIRK Service and airkctl ZIP for {system} {architecture}" '
                f'href="{base}/airk-{version}-{target}.zip">Service and airkctl ZIP</a>') if record["verified"] else ''
        cards.append(f"""<article class="download-card">
            <span class="os-mark" aria-hidden="true">{mark}</span>
            <h4>{system}</h4><p>{architecture}</p>
            <p class="installer-signing">{signing}</p>
            {download}
            {provenance}
            {pair}
          </article>""")
    extension = record["extension"]
    if extension:
        vsix = (f'<a class="button primary" aria-label="Download {channel} VS Code extension (universal VSIX)" '
                f'href="{escape(extension["url"], quote=True)}">Download VSIX <span aria-hidden="true">↓</span></a>'
                f'<a class="native-link" href="{escape(extension["manifest"], quote=True)}">Extension build record</a>')
    else:
        vsix = '<span class="button unavailable-download" aria-disabled="true">Extension not verified</span>'
    date = record["published"].date().isoformat()
    note = ("Preview builds are for early testing. " if channel == "preview" else "")
    return f"""<section class="release-block" aria-labelledby="{channel}-title">
        <div class="release-heading">
          <h3 id="{channel}-title"><span class="badge {channel}">{channel.capitalize()}</span> {tag}</h3>
          <p>Package {version} · Published <time datetime="{date}">{date}</time></p>
        </div>
        <h4 class="step-label">Step 1 · Install AIRK Service</h4>
        <div class="download-grid">{''.join(cards)}</div>
        <h4 class="step-label">Step 2 · Install VS Code Extension</h4>
        <div class="download-grid"><article class="download-card">
            <h4>VS Code</h4><p>All platforms</p>
            <p class="installer-signing">One universal VSIX with no native code; it connects to the AIRK Service from step 1.</p>
            {vsix}
          </article></div>
        <div class="release-links">
          <a href="{GITHUB}/releases/tag/{tag}">Release notes ↗</a>
          <a href="{base}/SHA256SUMS">SHA-256 checksums ↓</a>
          <a href="{base}/release.json">Build record ↓</a>
        </div>
        <p class="release-note">{note}Downloads appear only after their published digests match this release's build record. “Installer not published” means this release has no AIRK Service installer for that platform. Package version: {version}.</p>
      </section>"""


def build(catalog, output, fetch_record=None):
    # Deliberate allowlist: never upload the checkout, release archives or local state.
    rendered = render_downloads(select_releases(catalog, fetch_record))
    template = (ROOT / "site/index.html").read_text(encoding="utf-8")
    marker = "<!-- RELEASE_DOWNLOADS -->"
    if template.count(marker) != 1:
        raise ValueError("Expected exactly one download section marker")
    output.mkdir(parents=True, exist_ok=False)
    (output / "index.html").write_text(template.replace(marker, rendered), encoding="utf-8")
    for name in ("style.css", "favicon.svg"):
        shutil.copyfile(ROOT / "site" / name, output / name)
    (output / "images").mkdir()
    for name, component in PRODUCT_SCREENSHOTS.items():
        source = ROOT / "docs" / component / "images" / name
        if source.is_symlink() or not source.is_file() or source.stat().st_size > 2_097_152:
            raise ValueError("Invalid or oversized product screenshot: " + name)
        shutil.copyfile(source, output / "images" / name)



def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--catalog", type=Path, help="Local GitHub API release array for preview/testing")
    parser.add_argument("--output", type=Path, default=ROOT / "artifacts/downloads-site")
    args = parser.parse_args()
    if args.catalog:
        catalog = json.loads(args.catalog.read_text(encoding="utf-8"))
    else:
        result = subprocess.run(
            ["gh", "api", "--paginate", "--slurp", f"repos/{REPOSITORY}/releases?per_page=100"],
            check=True, text=True, capture_output=True, encoding="utf-8")
        catalog = [release for page in json.loads(result.stdout) for release in page]
    # A local catalog has no downloadable release.json: its split assets stay unverified.
    build(catalog, args.output, None if args.catalog else published_record)
    print(f"Built downloads page: {args.output}")


if __name__ == "__main__":
    main()
