/* SPDX-License-Identifier: Apache-2.0 */
/* AIRK UI pilot runtime source-sha256: 25bd3fc20188ecaf3b41c9f4397192b8943dc940f29cc3198acd25cb4f70d53e */
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

// ../ui/src/nativePilot.ts
var nativePilot_exports = {};
__export(nativePilot_exports, {
  createNativePilot: () => createNativePilot
});
module.exports = __toCommonJS(nativePilot_exports);

// src/uiPilotContract.ts
var PILOT_VERSION = 1;
function object(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value) ? value : null;
}
function fields(value, expected) {
  return Object.keys(value).length === expected.length && expected.every((key) => Object.hasOwn(value, key));
}
function integer(value, min, max) {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= min && value <= max;
}
function parsePilotRequest(value) {
  const request = object(value);
  if (!request || !fields(request, ["v", "type", "requestId", "epoch", "read"]) || request.v !== PILOT_VERSION || request.type !== "pilot.read" || typeof request.requestId !== "string" || !/^[a-zA-Z0-9_-]{1,64}$/.test(request.requestId) || !integer(request.epoch, 0, Number.MAX_SAFE_INTEGER)) return null;
  const read = object(request.read);
  if (!read) return null;
  switch (read.kind) {
    case "usage-json":
      if (!fields(read, ["kind", "binding", "days"]) || typeof read.binding !== "string" || !/^[0-9a-f]{64}$/.test(read.binding) || !integer(read.days, 1, 365)) return null;
      return Object.freeze({
        v: PILOT_VERSION,
        type: "pilot.read",
        requestId: request.requestId,
        epoch: request.epoch,
        read: Object.freeze({ kind: "usage-json", binding: read.binding, days: read.days })
      });
    case "help-topic":
      if (!fields(read, ["kind", "topic", "startLine"]) || typeof read.topic !== "string" || read.topic !== "" && !/^[a-z][a-z0-9-]{0,63}$/.test(read.topic) || !integer(read.startLine, 1, 1e5)) return null;
      return Object.freeze({
        v: PILOT_VERSION,
        type: "pilot.read",
        requestId: request.requestId,
        epoch: request.epoch,
        read: Object.freeze({ kind: "help-topic", topic: read.topic, startLine: read.startLine })
      });
    default:
      return null;
  }
}
function admitPilotRead(request, context) {
  if (!context.ready) return "pilot_not_ready";
  if (!context.trusted) return "pilot_untrusted";
  if (!context.nativeEnabled) return "pilot_native_disabled";
  if (request.epoch !== context.epoch) return "pilot_context_changed";
  if (request.read.kind === "usage-json" && request.read.binding !== context.usageBinding) return "pilot_binding_mismatch";
  return null;
}

// ../ui/src/nativePilot.ts
function argvFor(request) {
  switch (request.read.kind) {
    case "usage-json":
      return [
        "usage",
        "report",
        "--days",
        String(request.read.days),
        "--format",
        "json",
        "--service-project",
        request.read.binding,
        "--output",
        "json"
      ];
    case "help-topic":
      return ["tools", "call", "airk_help", "--args", JSON.stringify({
        ...request.read.topic === "" ? {} : { topic: request.read.topic },
        start_line: request.read.startLine,
        max_bytes: 32768
      }), "--output", "json"];
  }
}
function createNativePilot(host) {
  let sequence = 0;
  return async (input, signal) => {
    const request = parsePilotRequest(input);
    if (request === null) return null;
    const ticket = ++sequence;
    const session = host.session();
    const refusal = admitPilotRead(request, host.context());
    if (refusal !== null || session === null) return { request, result: { kind: "refused", code: refusal ?? "pilot_not_ready" }, applicability: "current" };
    let result;
    if (signal?.aborted) {
      result = { kind: "transport", status: null, message: "The read was canceled before dispatch." };
    } else {
      try {
        result = { kind: "i2", outcome: await host.invoke(argvFor(request), { deadlineMs: 6e4, ...signal ? { signal } : {} }) };
      } catch {
        result = { kind: "transport", status: null, message: "The native read did not return a complete outcome." };
      }
    }
    const current = ticket === sequence && session === host.session() && admitPilotRead(request, host.context()) === null;
    return { request, result, applicability: current ? "current" : "superseded" };
  };
}
// Annotate the CommonJS export names for ESM import in node:
0 && (module.exports = {
  createNativePilot
});
