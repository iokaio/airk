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
import tempfile
import zipfile
import stat

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
_spec = importlib.util.spec_from_file_location("airk_help_site", ROOT / "scripts/help-site.py")
help_site = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(help_site)
_spec = importlib.util.spec_from_file_location("publication_contract", ROOT / "scripts/publication-contract.py")
publication = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(publication)
_spec = importlib.util.spec_from_file_location("optional_publication", ROOT / "scripts/optional-publication.py")
optional = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(optional)
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
    expected = publication.release_assets(version, channel) | RECORD_FILES
    owner = optional.projection(tag) if optional.current(version) else None
    if owner is not None:
        expected |= set(owner["assets"])
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
        if (not isinstance(document, dict) or document.get("schema") != (optional.RELEASE if owner is not None else publication.RELEASE if publication.current(version, channel) else "airk-github-release/1") or document.get("tag") != tag
                or document.get("package_version") != version or document.get("channel") != channel
                or not isinstance(published_assets, dict) or set(published_assets) != expected - RECORD_FILES
                or any(by_name[name]["digest"] != "sha256:" + str(value) for name, value in published_assets.items())):
            raise ValueError(f"{tag}: published assets differ from the release build record")
        if owner is not None:
            optional.validate_release(document, owner)
            if any(by_name[name]["size"] != asset["bytes"] for name, asset in owner["assets"].items()):
                raise ValueError(f"{tag}: optional published part size differs from native pins")
        elif publication.current(version, channel):
            publication.validate_release(document)
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
            "channel": channel, "split": True, "verified": verified, "extension": extension,
            "optional_models": {name: {"url": by_name[name]["browser_download_url"], "bytes": asset["bytes"], "sha256": asset["sha256"]}
                                for name, asset in owner["assets"].items()} if owner is not None and verified else {}}


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
        if record["verified"] and publication.current(version, channel):
            pair = (f'<a class="native-link" href="{base}/{standalone.portable_artifact(version, target)}">Portable AIRK Service ZIP (manual included)</a>'
                    f'<a class="native-link" href="{base}/{standalone.portable_manifest(version, target)}">Portable build record</a>'
                    f'<a class="native-link" href="{base}/airk-{version}-{target}.zip">Developer native payload</a>')
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
    optional_links = ''.join(f'<li><a href="{escape(asset["url"], quote=True)}">{escape(name)}</a> · {asset["bytes"]:,} bytes · SHA-256 <code>{escape(asset["sha256"])}</code></li>'
                             for name, asset in sorted(record.get("optional_models", {}).items()))
    optional_section = ('<details><summary>Optional local model assets</summary><p>Choose and provision optional models in AIRK. Native compiled catalog pins validate every part and the complete model; these downloads do not activate a model.</p><ul>' + optional_links + '</ul></details>') if optional_links else ''
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
        {optional_section}
        <div class="release-links">
          <a href="{GITHUB}/releases/tag/{tag}">Release notes ↗</a>
          <a href="{base}/SHA256SUMS">SHA-256 checksums ↓</a>
          <a href="{base}/release.json">Build record ↓</a>
        </div>
        <p class="release-note">{note}Downloads appear only after their published digests match this release's build record. “Installer not published” means this release has no AIRK Service installer for that platform. Package version: {version}.</p>
      </section>"""


def help_from_vsix(archive, asset, version):
    """Read one fixed data entry; never extract code or arbitrary archive paths."""
    size = archive.stat().st_size
    if (archive.is_symlink() or not 0 < size <= 67_108_864 or size != asset["size"]
            or "sha256:" + hashlib.sha256(archive.read_bytes()).hexdigest() != asset["digest"]):
        raise ValueError("Published Help VSIX differs from its immutable release asset")
    with zipfile.ZipFile(archive) as package:
        infos = package.infolist(); names = [info.filename for info in infos]
        if len(infos) > 4096 or len(names) != len(set(names)):
            raise ValueError("Published Help VSIX has ambiguous entries")
        if help_site.MANIFEST_ENTRY not in names:
            if tuple(map(int, version.split('.'))) < (1, 8, 35):
                return None
            raise ValueError("Published product lacks its required shipping Help manifest")
        info = package.getinfo(help_site.MANIFEST_ENTRY)
        if (not 0 < info.file_size <= help_site.MAX_MANIFEST
                or stat.S_ISLNK(info.external_attr >> 16) or info.is_dir()):
            raise ValueError("Published Help manifest is invalid or oversized")
        with package.open(info) as stream:
            encoded = stream.read(help_site.MAX_MANIFEST + 1)
        if len(encoded) != info.file_size:
            raise ValueError("Published Help manifest length differs")
        help_site.manifest_bytes(encoded, version)
        return encoded


def published_help(release, metadata):
    """Acquire the release's verified universal VSIX, never current checkout topics."""
    if not metadata.get("split") or not metadata.get("verified"):
        return None
    name = f"airk-{metadata['version']}.vsix"
    asset = next(asset for asset in release["assets"] if asset["name"] == name)
    if (type(asset.get("id")) is not int or asset["id"] <= 0
            or type(asset.get("size")) is not int or not 0 < asset["size"] <= 67_108_864):
        raise ValueError("Published Help VSIX asset is invalid or oversized")
    with tempfile.TemporaryDirectory(prefix="airk-published-help-") as temporary:
        archive = Path(temporary) / "published.vsix"
        with archive.open("wb") as output:
            subprocess.run(["gh", "api", "-H", "Accept: application/octet-stream",
                            f"repos/{REPOSITORY}/releases/assets/{asset['id']}"],
                           check=True, stdout=output, timeout=120)
        return help_from_vsix(archive, asset, metadata['version'])


def build_help(records, catalog, output, fetch_help):
    selected = release = encoded = None
    for candidate in sorted((record for record in records if record.get('verified')),
                            key=lambda record: record['channel'] != 'stable'):
        if fetch_help is None:
            break
        source = next(release for release in catalog if release.get('tag_name') == candidate['tag'] and release.get('draft') is False)
        encoded = fetch_help(source, candidate)
        if encoded is not None:
            selected, release = candidate, source
            break
        if publication.current(candidate['version'], candidate['channel']):
            raise ValueError('Current published product lacks its required Help manifest')
    if encoded is not None:
        receipt = help_site.build(encoded, output / 'help', selected['version'], selected['channel'])
        receipt['release_tag'] = selected['tag']
        asset = next((asset for asset in release.get('assets', []) if asset.get('name') == f"airk-{selected['version']}.vsix"), None)
        if asset is not None:
            receipt['published_vsix'] = {key: asset[key] for key in ('id', 'name', 'digest', 'size')}
        (output / 'help/site-receipt.json').write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')
        return
    # Historical downloads remain available without inventing new product Help.
    help_root = output / 'help'; (help_root / 'html.index').mkdir(parents=True)
    page = '<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>AIRK Help</title><h1>AIRK Help</h1><p>The published product does not yet contain the shared Help manifest. No unreleased checkout content is shown.</p><a href="/index.html">Published AIRK downloads</a></html>'
    (help_root / 'index.html').write_text(page, encoding='utf-8')
    (help_root / 'html.index/index.html').write_text(page, encoding='utf-8')


def build(catalog, output, fetch_record=None, fetch_help=None):
    # Deliberate allowlist: never upload the checkout, release archives or local state.
    records = select_releases(catalog, fetch_record)
    rendered = render_downloads(records)
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
    build_help(records, catalog, output, fetch_help)



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
    build(catalog, args.output, None if args.catalog else published_record, None if args.catalog else published_help)
    print(f"Built downloads page: {args.output}")


if __name__ == "__main__":
    main()
