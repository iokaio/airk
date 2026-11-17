// SPDX-License-Identifier: Apache-2.0
/*! AIRK uninstall hook 1.8.207: removes editor-owned storage only. No third-party code. */
"use strict";
var __create = Object.create;
var __defProp = Object.defineProperty;
var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __getProtoOf = Object.getPrototypeOf;
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
var __toESM = (mod, isNodeMode, target) => (target = mod != null ? __create(__getProtoOf(mod)) : {}, __copyProps(
  // If the importer is in node compatibility mode or this is not an ESM
  // file that has been converted to a CommonJS file using a Babel-
  // compatible transform (i.e. "__esModule" has not been set), then set
  // "default" to the CommonJS "module.exports" for node compatibility.
  isNodeMode || !mod || !mod.__esModule ? __defProp(target, "default", { value: mod, enumerable: true }) : target,
  mod
));
var __toCommonJS = (mod) => __copyProps(__defProp({}, "__esModule", { value: true }), mod);

// src/uninstallHook.ts
var uninstallHook_exports = {};
__export(uninstallHook_exports, {
  EXTENSION_ID: () => EXTENSION_ID,
  editorUserData: () => editorUserData,
  removeEditorStorage: () => removeEditorStorage
});
module.exports = __toCommonJS(uninstallHook_exports);
var fs = __toESM(require("node:fs/promises"));
var os = __toESM(require("node:os"));
var path = __toESM(require("node:path"));
var EXTENSION_ID = "iokaio.airk";
function editorUserData(extensionsDir, home, platform, appData) {
  const api = platform === "win32" ? path.win32 : path.posix;
  const products = { ".vscode": "Code", ".vscode-insiders": "Code - Insiders", ".vscode-oss": "VSCodium" };
  if (api.basename(extensionsDir) !== "extensions") {
    return void 0;
  }
  const marker = api.dirname(extensionsDir);
  const same = (a, b) => platform === "win32" ? a.toLowerCase() === b.toLowerCase() : a === b;
  if (!same(api.dirname(marker), api.normalize(home))) {
    return void 0;
  }
  const product = products[api.basename(marker)];
  if (!product) {
    return void 0;
  }
  switch (platform) {
    case "win32":
      return appData ? api.join(appData, product) : void 0;
    case "darwin":
      return api.join(home, "Library", "Application Support", product);
    default:
      return api.join(home, ".config", product);
  }
}
async function removeEditorStorage(userData) {
  const storage = path.join(userData, "User", "globalStorage", EXTENSION_ID);
  let info;
  try {
    info = await fs.lstat(storage);
  } catch (error) {
    if (error.code === "ENOENT") {
      return "absent";
    }
    throw error;
  }
  if (!info.isDirectory() || info.isSymbolicLink()) {
    return "refused";
  }
  await fs.rm(storage, { recursive: true, force: false });
  return "removed";
}
async function main() {
  const extensionsDir = path.resolve(__dirname, "..", "..");
  const platform = process.platform === "win32" || process.platform === "darwin" ? process.platform : "linux";
  const userData = editorUserData(extensionsDir, os.userInfo().homedir, platform, process.env.APPDATA);
  if (!userData) {
    process.stdout.write("AIRK: editor storage location not recognized; nothing was removed.\n");
    return;
  }
  const result = await removeEditorStorage(userData);
  process.stdout.write(`AIRK: editor storage ${result}. AIRK Service, its indexes and agent configuration were not changed.
`);
}
if (require.main === module) {
  void main().catch((error) => {
    process.stderr.write(String(error) + "\n");
    process.exitCode = 1;
  });
}
// Annotate the CommonJS export names for ESM import in node:
0 && (module.exports = {
  EXTENSION_ID,
  editorUserData,
  removeEditorStorage
});
