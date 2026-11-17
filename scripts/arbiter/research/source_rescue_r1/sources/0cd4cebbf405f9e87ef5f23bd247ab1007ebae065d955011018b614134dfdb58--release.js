/* SPDX-License-Identifier: Apache-2.0 */
/* AIRK bootstrap release source-sha256: 4ca915116fa87e598d7d640eb988315eedc4ebb3433df2b807c97f21df5c7eb8 */

"use strict";
var __defProp = Object.defineProperty;
var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __hasOwnProp = Object.prototype.hasOwnProperty;
var __export = (target, all) => {
  for (var name in all)
    __defProp(target, name, { get: all[name], enumerable: true });
};
var __copyProps = (to, from, except, desc) => {
  if (from && typeof from === "object" || typeof from === "function") {
    for (let key of __getOwnPropNames(from))
      if (!__hasOwnProp.call(to, key) && key !== except)
        __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
  }
  return to;
};
var __toCommonJS = (mod) => __copyProps(__defProp({}, "__esModule", { value: true }), mod);

// src/airkctl/bootstrap/release.ts
var release_exports = {};
__export(release_exports, {
  RELEASE_API: () => RELEASE_API,
  approvedAttestationBundleUrl: () => approvedAttestationBundleUrl,
  approvedRedirect: () => approvedRedirect,
  releaseAsset: () => releaseAsset,
  selectInstaller: () => selectInstaller,
  selectRelease: () => selectRelease,
  targetOf: () => targetOf,
  validatePublication: () => validatePublication
});
module.exports = __toCommonJS(release_exports);
var optionalPublication = false ? void 0 : null;
var RELEASE_API = "https://api.github.com/repos/iokaio/airk/releases";
function targetOf(platform, arch) {
  if (platform === "win32" && arch === "x64") {
    return "x86_64-pc-windows-msvc";
  }
  if (platform === "linux" && arch === "x64") {
    return "x86_64-unknown-linux-gnu";
  }
  if (platform === "darwin" && arch === "arm64") {
    return "aarch64-apple-darwin";
  }
  if (platform === "darwin" && arch === "x64") {
    return "x86_64-apple-darwin";
  }
  return void 0;
}
function record(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("Invalid official release metadata.");
  }
  return value;
}
function assetUrl(value, tag, name) {
  const expected = `https://github.com/iokaio/airk/releases/download/${tag}/${name}`;
  if (value !== expected) {
    throw new Error("Installer URL is outside the selected official release.");
  }
  return expected;
}
function selectRelease(value, version) {
  if (!/^\d+\.\d+\.\d+$/.test(version) || !Array.isArray(value) || value.length > 20) {
    throw new Error("Invalid release catalog.");
  }
  const candidates = value.map(record).filter((row) => row.tag_name === `v${version}` && row.draft === false && row.prerelease === false);
  if (candidates.length !== 1 || !Array.isArray(candidates[0].assets) || candidates[0].assets.length > 128) {
    throw new Error("No unique compatible stable AIRK release was found. Use manual instructions.");
  }
  return { tag: `v${version}`, assets: candidates[0].assets.map(record) };
}
function releaseAsset(release, name, maximum) {
  const found = release.assets.filter((row) => row.name === name);
  if (found.length !== 1 || !Number.isSafeInteger(found[0].size) || Number(found[0].size) < 1 || Number(found[0].size) > maximum) {
    throw new Error("Missing, duplicated or oversized release asset.");
  }
  const digest = found[0].digest;
  if (digest !== void 0 && digest !== null && (typeof digest !== "string" || !/^sha256:[a-f0-9]{64}$/.test(digest))) {
    throw new Error("Invalid GitHub asset digest.");
  }
  return { url: assetUrl(found[0].browser_download_url, release.tag, name), bytes: Number(found[0].size), ...typeof digest === "string" ? { digest: digest.slice(7) } : {} };
}
var TARGET_SUFFIX = {
  "aarch64-apple-darwin": "dmg",
  "x86_64-apple-darwin": "dmg",
  "x86_64-pc-windows-msvc": "exe",
  "x86_64-unknown-linux-gnu": "tar.gz"
};
function exactKeys(value, expected) {
  return Object.keys(value).sort().join("|") === [...expected].sort().join("|");
}
function canonicalOpaque(value) {
  if (Array.isArray(value)) {
    return "[" + value.map(canonicalOpaque).join(",") + "]";
  }
  if (value !== null && typeof value === "object") {
    const row = record(value);
    return "{" + Object.keys(row).sort().map((key) => JSON.stringify(key) + ":" + canonicalOpaque(row[key])).join(",") + "}";
  }
  const result = JSON.stringify(value);
  if (result === void 0) {
    throw new Error("Invalid opaque publication value.");
  }
  return result;
}
function validatePublication(value, expectedOptional = optionalPublication) {
  const row = record(value);
  if (row.schema === "airk-github-release/3") {
    if (!exactKeys(row, ["schema", "tag", "package_version", "commit", "tree", "channel", "run_id", "attempt", "assets", "producer", "installers", "optional_models"]) || !expectedOptional || canonicalOpaque(row.optional_models) !== canonicalOpaque(expectedOptional)) {
      throw new Error("Optional publication differs from the sealed native projection or is unavailable.");
    }
    const native = record(expectedOptional);
    if (native.tag !== row.tag) {
      throw new Error("Optional native publication tag differs.");
    }
    const assets2 = { ...record(row.assets) };
    for (const [name, part] of Object.entries(record(native.assets))) {
      if (assets2[name] !== record(part).sha256) {
        throw new Error("Optional published part digest differs.");
      }
      delete assets2[name];
    }
    const base = { ...row, schema: "airk-github-release/2", assets: assets2 };
    delete base.optional_models;
    validatePublication(base);
    return;
  }
  if (!exactKeys(row, ["schema", "tag", "package_version", "commit", "tree", "channel", "run_id", "attempt", "assets", "producer", "installers"]) || row.schema !== "airk-github-release/2") {
    throw new Error("Invalid publication fields.");
  }
  const version = row.package_version;
  if (typeof version !== "string" || !/^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/.test(version)) {
    throw new Error("Invalid publication version.");
  }
  const parts = version.split(".").map(BigInt);
  if (parts[0] < 1n || parts[0] === 1n && (parts[1] < 8n || parts[1] === 8n && parts[2] < 35n)) {
    throw new Error("Publication predates the headless contract.");
  }
  if (row.channel !== "stable" && row.channel !== "preview") {
    throw new Error("Invalid publication channel.");
  }
  const preview = row.channel === "preview";
  if (typeof row.tag !== "string" || !new RegExp("^v" + version.replace(/\./g, "\\.") + (preview ? "(?:-build\\.[1-9][0-9]*)?" : "") + "$").test(row.tag)) {
    throw new Error("Publication tag differs.");
  }
  for (const key of ["commit", "tree"]) {
    if (typeof row[key] !== "string" || !/^[a-f0-9]{40}$/.test(row[key])) {
      throw new Error("Publication revision differs.");
    }
  }
  for (const key of ["run_id", "attempt"]) {
    if (typeof row[key] !== "string" || !/^[1-9][0-9]*$/.test(row[key])) {
      throw new Error("Publication run differs.");
    }
  }
  if (record(row.producer).commit !== row.commit) {
    throw new Error("Publication producer commit differs.");
  }
  const expectedAssets = [`airk-${version}.vsix`, `airk-${version}.vsix-manifest.json`];
  const installers = record(row.installers);
  if (!exactKeys(installers, preview ? [] : Object.keys(TARGET_SUFFIX))) {
    throw new Error("Publication installer targets differ.");
  }
  for (const [target, suffix] of Object.entries(TARGET_SUFFIX)) {
    expectedAssets.push(
      `airk-${version}-${target}.zip`,
      `airk-${version}-${target}.native-manifest.json`,
      `airk-${version}-${target}-portable.zip`,
      `airk-${version}-${target}-portable.manifest.json`
    );
    if (!preview) {
      const name = `airk-${version}-${target}-installer.${suffix}`, manifest = `airk-${version}-${target}-installer.manifest.json`;
      expectedAssets.push(name, manifest);
      const item = record(installers[target]), headless = record(item.headless);
      if (!exactKeys(item, ["artifact", "manifest", "headless"]) || item.artifact !== name || item.manifest !== manifest || !exactKeys(headless, ["arguments", "stdin", "prompts", "certificate_trust", "requires_reset_acknowledgement", "success_requires"]) || JSON.stringify(headless.arguments) !== '["--silent"]' || headless.stdin !== "closed" || headless.prompts !== false || headless.certificate_trust !== "skip" || headless.requires_reset_acknowledgement !== true || headless.success_requires !== "native_setup_and_readiness") {
        throw new Error("Publication installer capability differs.");
      }
    }
  }
  const assets = record(row.assets);
  if (!exactKeys(assets, expectedAssets) || Object.values(assets).some((digest) => typeof digest !== "string" || !/^[a-f0-9]{64}$/.test(digest))) {
    throw new Error("Publication asset roster differs.");
  }
}
function selectInstaller(release, value, target, expectedOptional = optionalPublication) {
  validatePublication(value, expectedOptional);
  const row = record(value);
  if (!["airk-github-release/2", "airk-github-release/3"].includes(String(row.schema)) || row.tag !== release.tag || row.package_version !== release.tag.slice(1) || row.channel !== "stable") {
    throw new Error("This release does not declare verified headless installers.");
  }
  const installers = record(row.installers), hashes = record(row.assets), item = record(installers[target]);
  const suffix = target.endsWith("windows-msvc") ? "exe" : target.endsWith("apple-darwin") ? "dmg" : target === "x86_64-unknown-linux-gnu" ? "tar.gz" : void 0;
  if (!suffix) {
    throw new Error("Unsupported installer target.");
  }
  const name = `airk-${row.package_version}-${target}-installer.${suffix}`;
  const manifest = `airk-${row.package_version}-${target}-installer.manifest.json`;
  const headless = record(item.headless);
  if (item.artifact !== name || item.manifest !== manifest || JSON.stringify(headless.arguments) !== '["--silent"]' || headless.stdin !== "closed" || headless.prompts !== false || headless.certificate_trust !== "skip" || headless.requires_reset_acknowledgement !== true || headless.success_requires !== "native_setup_and_readiness") {
    throw new Error("The installer does not declare the approved headless behavior.");
  }
  if (typeof hashes[name] !== "string" || !/^[a-f0-9]{64}$/.test(hashes[name]) || typeof hashes[manifest] !== "string" || !/^[a-f0-9]{64}$/.test(hashes[manifest])) {
    throw new Error("Installer/manifest digest is absent.");
  }
  const asset = releaseAsset(release, name, 2 * 1024 * 1024 * 1024);
  if (asset.digest && asset.digest !== hashes[name]) {
    throw new Error("GitHub and release installer digests disagree.");
  }
  return {
    version: String(row.package_version),
    target,
    name,
    url: asset.url,
    sha256: hashes[name],
    bytes: asset.bytes,
    signature: suffix === "exe" ? "authenticode" : suffix === "dmg" ? "developer_id" : "unsigned_digest",
    headlessArguments: ["--silent"]
  };
}
function approvedRedirect(initial, candidate) {
  let url, first;
  try {
    url = new URL(candidate);
    first = new URL(initial);
  } catch {
    return false;
  }
  const api = first.origin === "https://api.github.com" && first.pathname === "/repos/iokaio/airk/releases" && first.searchParams.get("per_page") === "20" && /^[1-3]$/.test(first.searchParams.get("page") ?? "") && [...first.searchParams.keys()].every((key) => key === "per_page" || key === "page");
  const attestation = first.origin === "https://api.github.com" && /^\/repos\/iokaio\/airk\/attestations\/sha256:[a-f0-9]{64}$/.test(first.pathname) && first.searchParams.get("per_page") === "5" && first.searchParams.get("predicate_type") === "https://slsa.dev/provenance/v1" && [...first.searchParams.keys()].every((key) => key === "per_page" || key === "predicate_type");
  const storage = approvedAttestationBundleUrl(initial);
  const asset = first.origin === "https://github.com" && /^\/iokaio\/airk\/releases\/download\/v[0-9]+\.[0-9]+\.[0-9]+\/[A-Za-z0-9_.-]+$/.test(first.pathname) && !first.search;
  if (!api && !asset && !attestation && !storage) {
    return false;
  }
  if (url.protocol !== "https:" || url.username || url.password || url.port && url.port !== "443" || url.hash) {
    return false;
  }
  if (url.origin === first.origin && url.pathname === first.pathname && url.search === first.search) {
    return true;
  }
  if (first.origin !== "https://github.com" || !first.pathname.startsWith("/iokaio/airk/releases/download/")) {
    return false;
  }
  return url.hostname === "release-assets.githubusercontent.com" && url.pathname.startsWith("/github-production-release-asset/") || url.hostname === "objects.githubusercontent.com" && url.pathname.startsWith("/github-production-release-asset-2e65be/");
}
function approvedAttestationBundleUrl(value) {
  let url;
  try {
    url = new URL(value);
  } catch {
    return false;
  }
  return url.protocol === "https:" && url.hostname === "tmaproduction.blob.core.windows.net" && !url.port && !url.username && !url.password && !url.hash && /^\/attestations\/1418150478\/[0-9]{4}\/[0-9]{2}\/[0-9]{2}\/[0-9]+\.json\.sn$/.test(url.pathname);
}
// Annotate the CommonJS export names for ESM import in node:
0 && (module.exports = {
  RELEASE_API,
  approvedAttestationBundleUrl,
  approvedRedirect,
  releaseAsset,
  selectInstaller,
  selectRelease,
  targetOf,
  validatePublication
});
