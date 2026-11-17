// SPDX-License-Identifier: Apache-2.0
/*! AIRK Explorer webview. Includes Preact 10.29.8 (MIT).
The MIT License (MIT)

Copyright (c) 2015-present Jason Miller

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
*/
"use strict";
(() => {
  // src/generated/arbiterBudget.ts
  var DEFAULT_CONTEXT_BUDGET = { "calls": 8, "rounds": 3, "fanout": 8, "input_tokens": 8192, "candidates": 32, "max_bytes": 262144, "deadline_ms": 12e4 };

  // ../ui/src/nativeMetadataCodec.ts
  var WORKBENCH_MAX_STEP_REPORTS = 72;
  var object = (v3) => v3 && typeof v3 === "object" && !Array.isArray(v3) ? v3 : {};
  function readResultBody(raw) {
    const value = object(raw);
    if (Array.isArray(value.content) && Object.keys(object(value.structuredContent)).length) return object(value.structuredContent);
    if (value.isError === true && Array.isArray(value.content)) {
      if (Object.keys(object(value.structuredContent)).length) return object(value.structuredContent);
      const text = object(value.content[0]).text;
      if (typeof text === "string" && text.length <= 524288) {
        try {
          return object(JSON.parse(text));
        } catch {
        }
      }
    }
    return value;
  }
  var SHARED_CAPABILITY_REFERENCE = "$airk_shared_capability_fields";
  var INLINE_CAPABILITY_FIELDS = /* @__PURE__ */ new Set(["request_admission", "runtime", "work", "scope_observation", "publication", "freshness", "collection", "generation", "observed_unix_ms", "cache_age_ms", "cached", "status_observation", "request_id", "query_concurrency", "source_verification", "error", "requested_scope"]);
  function shareableCapabilityField(key) {
    return key.length > 0 && key.length <= 128 && /^[\x00-\x7f]+$/.test(key) && !["__proto__", "constructor", "prototype"].includes(key) && !INLINE_CAPABILITY_FIELDS.has(key) && !key.startsWith("runtime") && !key.endsWith("_observation") && key !== SHARED_CAPABILITY_REFERENCE;
  }
  function materializeCapabilityMember(raw, display) {
    const body = readResultBody(raw);
    if (!Object.hasOwn(body, SHARED_CAPABILITY_REFERENCE)) return body;
    if (display?.state !== "verified") throw new Error(display?.state === "unavailable" ? display.error : "Shared capability data has not been validated.");
    const reference = body[SHARED_CAPABILITY_REFERENCE];
    if (typeof reference !== "string" || !/^[0-9a-f]{64}$/.test(reference) || !Object.hasOwn(display.values, reference)) throw new Error("Shared capability reference is unavailable.");
    const fields = display.values[reference], inline = { ...body };
    delete inline[SHARED_CAPABILITY_REFERENCE];
    if (Object.keys(fields).some((key) => !shareableCapabilityField(key) || Object.hasOwn(inline, key))) throw new Error("Shared capability fields conflict with native member fields.");
    return { ...fields, ...inline };
  }

  // src/workbenchContracts.ts
  var WORKBENCH_CONTEXT_BUDGET = Object.freeze({ ...DEFAULT_CONTEXT_BUDGET });
  var OPEN_SELECTION_MAX = 32;
  var OPEN_SELECTION_NAME_BYTES = 256;
  function validateOpenSelection(raw) {
    const value = raw && typeof raw === "object" && !Array.isArray(raw) ? raw : {};
    if (Object.keys(value).some((key) => key !== "indexes" && key !== "sets")) throw new Error("Name only indexes and index sets.");
    const list = (key, noun) => {
      const items = value[key] ?? [];
      if (!Array.isArray(items)) throw new Error("Name the ".concat(noun, "es as a list."));
      const out = items.map((item) => typeof item === "string" ? item.trim() : "");
      for (const item of out) {
        if (!item) throw new Error("An ".concat(noun, " name is empty."));
        if (new TextEncoder().encode(item).length > OPEN_SELECTION_NAME_BYTES) throw new Error("An ".concat(noun, " name exceeds ").concat(OPEN_SELECTION_NAME_BYTES, " bytes."));
        if (/[\u0000-\u001f\u007f-\u009f]/.test(item)) throw new Error("An ".concat(noun, " name contains a control character."));
        if (item.startsWith("-")) throw new Error('"'.concat(item, '" starts with "-"; name that ').concat(noun, " by its id instead."));
      }
      if (new Set(out).size !== out.length) throw new Error("An ".concat(noun, " is named twice."));
      if (out.length > OPEN_SELECTION_MAX) throw new Error("Name at most ".concat(OPEN_SELECTION_MAX, " ").concat(noun).concat(noun === "index" ? "es" : "s", "."));
      return out;
    };
    const selection = { indexes: list("indexes", "index"), sets: list("sets", "index set") };
    if (!selection.indexes.length && !selection.sets.length) throw new Error("Name at least one index or index set.");
    return selection;
  }
  var openSelectionLines = (text) => text.split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
  var FORM_TOOLS = ["search", "symbols", "references", "read_file", "outline", "list_files", "file_info", "imports", "code_facts", "document_facts", "changes", "context", "relationships", "status", "diagnostics", "capabilities", "query_scope", "help"];
  var FORM_RECIPES = ["", "definition", "documents", "impact", "tests", "component", "readiness"];
  var validFormText = (value, max = 32768) => typeof value === "string" && value.length <= max && !/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(value);
  var defaultWorkbenchFormDraft = () => ({ activeTool: "search", values: {}, revision: 0, scope: "", recipe: "" });
  function validWorkbenchToolFormValues(raw) {
    if (!raw || typeof raw !== "object" || Array.isArray(raw) || ![Object.prototype, null].includes(Object.getPrototypeOf(raw))) return false;
    const entries = Object.entries(raw);
    return entries.length <= 32 && entries.every(([key, value]) => /^[a-z][a-z0-9_]{0,63}$/.test(key) && !["constructor", "prototype"].includes(key) && validFormText(value));
  }
  function normalizeWorkbenchFormDraft(raw) {
    if (raw === void 0) return defaultWorkbenchFormDraft();
    if (!raw || typeof raw !== "object" || Array.isArray(raw)) return;
    const draft = raw, keys = Object.keys(draft);
    if (keys.some((key) => !["activeTool", "values", "revision", "scope", "recipe"].includes(key)) || typeof draft.activeTool !== "string" || !FORM_TOOLS.includes(draft.activeTool) || !Number.isSafeInteger(draft.revision) || Number(draft.revision) < 0 || Number(draft.revision) > 2147483647 || !draft.values || typeof draft.values !== "object" || Array.isArray(draft.values) || ![Object.prototype, null].includes(Object.getPrototypeOf(draft.values)) || Object.keys(draft.values).length > FORM_TOOLS.length) return;
    const scope = draft.scope ?? "", recipe = draft.recipe ?? "";
    if (!validFormText(scope, 1024) || typeof recipe !== "string" || !FORM_RECIPES.includes(recipe)) return;
    const values = {};
    for (const [tool, fields] of Object.entries(draft.values)) {
      if (!FORM_TOOLS.includes(tool) || !validWorkbenchToolFormValues(fields)) return;
      values[tool] = { ...fields };
    }
    const normalized = { activeTool: draft.activeTool, values, revision: Number(draft.revision), scope, recipe };
    if (new TextEncoder().encode(JSON.stringify(normalized)).length > 131072) return;
    return normalized;
  }
  function applyWorkbenchUpdate(previous, message) {
    if (message.type === "state") {
      const next = message.state;
      if (typeof previous.sessionEpoch === "number" && typeof next.sessionEpoch === "number" && next.sessionEpoch < previous.sessionEpoch) return previous;
      if (previous.session?.id === next.session?.id && previous.session?.project === next.session?.project && previous.sessionEpoch === next.sessionEpoch && previous.session && next.session) {
        const before = normalizeWorkbenchFormDraft(previous.session.formDraft), incoming = normalizeWorkbenchFormDraft(next.session.formDraft);
        if (before && incoming && before.revision > incoming.revision) return { ...next, session: { ...next.session, formDraft: before } };
      }
      return next;
    }
    if (message.sessionId !== previous.session?.id || message.project !== previous.session?.project) return previous;
    if (message.type === "status") return { ...previous, status: message.status, publicationRelations: message.publicationRelations };
    if (!previous.session) return previous;
    if (message.type === "formDraft") {
      const current = normalizeWorkbenchFormDraft(previous.session.formDraft), incoming = normalizeWorkbenchFormDraft(message.formDraft);
      if (message.epoch !== previous.sessionEpoch || !incoming || current && incoming.revision < current.revision) return previous;
      return { ...previous, session: { ...previous.session, formDraft: incoming }, save: message.save };
    }
    return { ...previous, session: { ...previous.session, revision: message.revision }, recent: message.recent, save: message.save };
  }
  var fieldLabels = { limit_per_index: "Max results per index", project_ids: "Project IDs (JSON array)", index_ids: "Index subset (JSON array)", member_availability: "Member availability", path_prefix: "Path prefix", exclude_path_prefix: "Exclude path prefix", evidence_kind: "Evidence type", name_match: "Name matching", reply_detail: "Reply detail", verification: "Saved-source verification", verification_max_sources: "Maximum sources to check", verification_max_bytes: "Raw-source byte budget", verification_deadline_ms: "Verification deadline (ms)", compact: "Compact results", include_profile_families: "Include effective profile families", include_count: "Show count precision", include_files: "Summarize files", max_bytes: "Response byte budget" };
  function fieldSchema(raw, root, depth = 0) {
    const schema = object2(raw);
    if (depth > 4) return {};
    if (typeof schema.$ref === "string" && schema.$ref.startsWith("#/$defs/")) {
      const name = schema.$ref.slice("#/$defs/".length);
      return { ...fieldSchema(object2(root.$defs)[name], root, depth + 1), ...schema };
    }
    const variants = Array.isArray(schema.anyOf) ? schema.anyOf : Array.isArray(schema.oneOf) ? schema.oneOf : [];
    const selected = variants.map((value) => fieldSchema(value, root, depth + 1)).find((value) => value.type !== "null" && (value.type !== void 0 || Array.isArray(value.enum)));
    return selected ? { ...schema, ...selected } : schema;
  }
  function toolFormFields(raw) {
    const schema = object2(raw), required = Array.isArray(schema.required) ? schema.required : [];
    return Object.entries(object2(schema.properties)).map(([name, value]) => {
      const spec = fieldSchema(value, schema), types = Array.isArray(spec.type) ? spec.type : [spec.type];
      const type = types.includes("boolean") ? "boolean" : types.includes("integer") ? "integer" : types.includes("array") || types.includes("object") ? "json" : "string";
      return {
        name,
        ...["string", "number", "boolean"].includes(typeof spec.default) ? { defaultValue: spec.default } : {},
        label: fieldLabels[name] ?? name.replaceAll("_", " "),
        type,
        required: required.includes(name),
        ...Array.isArray(spec.enum) ? { values: spec.enum.filter((item) => ["string", "number", "boolean"].includes(typeof item)).map(String) } : {},
        ...typeof spec.description === "string" ? { description: spec.description } : {},
        ...typeof spec.minLength === "number" ? { minLength: spec.minLength } : {},
        ...typeof spec.maxLength === "number" ? { maxLength: Math.min(32768, spec.maxLength) } : type === "json" ? { maxLength: 32768 } : {},
        ...typeof spec.minimum === "number" ? { minimum: spec.minimum } : {},
        ...typeof spec.maximum === "number" ? { maximum: spec.maximum } : {}
      };
    });
  }
  function limitPerIndexField(schema, combinedControls) {
    const properties = object2(object2(schema).properties), declared = object2(combinedControls).limit_per_index;
    if (!Object.hasOwn(properties, "limit") || Object.hasOwn(properties, "limit_per_index") || !declared || typeof declared !== "object" || Array.isArray(declared)) return;
    const spec = object2(declared), minimum = typeof spec.minimum === "number" ? Math.max(1, spec.minimum) : 1, maximum = typeof spec.maximum === "number" ? Math.min(100, spec.maximum) : 100;
    return {
      name: "limit_per_index",
      label: fieldLabels.limit_per_index,
      type: "integer",
      required: false,
      minimum,
      maximum,
      description: "Each index returns at most this many results; Limit stays the overall cap."
    };
  }
  function workbenchToolFields(schema, combinedControls) {
    const fields = toolFormFields(schema), extra = limitPerIndexField(schema, combinedControls);
    if (!extra) return fields;
    const at = fields.findIndex((field) => field.name === "limit");
    return [...fields.slice(0, at + 1), extra, ...fields.slice(at + 1)];
  }
  function toolFormArguments(schema, values, combinedControls) {
    const fields = workbenchToolFields(schema, combinedControls), known = new Set(fields.map((field) => field.name)), entries = [];
    for (const [name, value] of Object.entries(values)) if (value !== "" && !known.has(name)) throw new Error("Choose a field from the current tool form.");
    for (const field of fields) {
      const value = values[field.name] ?? "";
      if (value === "") {
        if (field.required) throw new Error("".concat(field.label, " is required."));
        continue;
      }
      if (value.length > (field.maxLength ?? 4096) || field.minLength !== void 0 && value.length < field.minLength) throw new Error("".concat(field.label, " is outside the allowed input length."));
      if (field.values && !field.values.includes(value)) throw new Error("Choose a supported ".concat(field.label, "."));
      if (field.type === "boolean") {
        if (value !== "true" && value !== "false") throw new Error("Choose Yes, No or Default for ".concat(field.label, "."));
        entries.push([field.name, value === "true"]);
      } else if (field.type === "integer") {
        const number = Number(value);
        if (!Number.isSafeInteger(number) || field.minimum !== void 0 && number < field.minimum || field.maximum !== void 0 && number > field.maximum) throw new Error("Enter an allowed whole number for ".concat(field.label, "."));
        entries.push([field.name, number]);
      } else if (field.type === "json") {
        let parsed2;
        try {
          parsed2 = JSON.parse(value);
        } catch {
          throw new Error("Enter valid JSON for ".concat(field.label, "."));
        }
        if (field.name === "selectors" && (!Array.isArray(parsed2) || parsed2.length > 64)) throw new Error("Enter at most 64 selectors as a JSON array.");
        entries.push([field.name, parsed2]);
      } else entries.push([field.name, value]);
    }
    return Object.fromEntries(entries);
  }
  var object2 = (v3) => v3 && typeof v3 === "object" && !Array.isArray(v3) ? v3 : {};
  function parseWorkbenchMessage(raw) {
    const v3 = object2(raw), type = v3.type;
    const str = (k3, max = 128) => typeof v3[k3] === "string" && v3[k3].length <= max && !/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(v3[k3]);
    const exact = (keys) => Object.keys(v3).every((k3) => keys.includes(k3));
    if (type === "answer" && exact(["type", "question", "budget"]) && str("question", 16384) && new TextEncoder().encode(String(v3.question)).length <= 16384 && String(v3.question).trim() && (v3.budget === void 0 || Object.keys(object2(v3.budget)).length === Object.keys(WORKBENCH_CONTEXT_BUDGET).length && Object.keys(WORKBENCH_CONTEXT_BUDGET).every((key) => Number.isSafeInteger(object2(v3.budget)[key]) && Number(object2(v3.budget)[key]) >= 0))) return v3;
    if (typeof type !== "string" || JSON.stringify(raw).length > (type === "formDraft" ? 132096 : 4e4)) {
      return;
    }
    const formFence = () => str("sessionId") && str("project", 64) && /^[0-9a-f]{64}$/.test(String(v3.project)) && Number.isSafeInteger(v3.epoch) && Number(v3.epoch) >= 0 && Number.isSafeInteger(v3.revision) && Number(v3.revision) > 0 && Number(v3.revision) <= 2147483647 && typeof v3.tool === "string" && FORM_TOOLS.includes(v3.tool);
    if (type === "exportReferenceRefresh" && exact(["type", "id", "format", "action"]) && str("id", 32) && /^[0-9a-f]{32}$/.test(String(v3.id)) && ["json", "markdown"].includes(String(v3.format)) && ["copy", "save"].includes(String(v3.action))) return v3;
    if (type === "refreshReferences" && exact(["type", "turn", "cards", "mode"]) && str("turn") && ["selected_only", "full_reconcile_reviewed"].includes(String(v3.mode)) && Array.isArray(v3.cards) && v3.cards.length > 0 && v3.cards.length <= 64 && new Set(v3.cards).size === v3.cards.length && v3.cards.every((id) => typeof id === "string" && /^record-(0|[1-9][0-9]*)$/.test(id))) return v3;
    if (["checkReferenceRefresh", "requeryReferenceRefresh"].includes(String(type)) && exact(["type", "id"]) && str("id", 32) && /^[0-9a-f]{32}$/.test(String(v3.id))) return v3;
    if (type === "formDraft" && exact(["type", "sessionId", "project", "epoch", "revision", "tool", "values", "scope", "recipe"]) && formFence() && (v3.values === void 0 || validWorkbenchToolFormValues(v3.values)) && (v3.scope === void 0 || validFormText(v3.scope, 1024)) && (v3.recipe === void 0 || typeof v3.recipe === "string" && FORM_RECIPES.includes(v3.recipe))) {
      return v3;
    }
    if (type === "resetForm" && exact(["type", "sessionId", "project", "epoch", "revision", "tool"]) && formFence()) {
      return v3;
    }
    if (type === "exportSourceOpen" && exact(["type", "id", "format"]) && str("id", 32) && /^[0-9a-f]{32}$/.test(String(v3.id)) && ["copy", "json"].includes(String(v3.format))) return v3;
    if (["ready", "stop", "new", "clear", "manage", "help", "refresh", "retrySave", "fork", "forms", "scopeChoices"].includes(type) && exact(["type"])) {
      return v3;
    }
    if (type === "openSelection" && exact(["type", "indexes", "sets"]) && Array.isArray(v3.indexes) && Array.isArray(v3.sets)) {
      try {
        const selection = validateOpenSelection({ indexes: v3.indexes, sets: v3.sets });
        if (JSON.stringify(selection) === JSON.stringify({ indexes: v3.indexes, sets: v3.sets })) return v3;
      } catch {
        return;
      }
    }
    if (["project", "recent", "delete"].includes(type) && exact(["type", "id"]) && str("id")) {
      return v3;
    }
    if (type === "rename" && exact(["type", "title"]) && str("title", 80)) {
      return v3;
    }
    if (type === "run" && exact(["type", "input", "prepare", "recipe", "plan", "pathPrefix"]) && str("input", 4096) && typeof v3.prepare === "boolean" && (v3.pathPrefix === void 0 || str("pathPrefix", 1024)) && (v3.recipe === void 0 || str("recipe")) && (v3.plan === void 0 || Object.keys(object2(v3.plan)).length > 0)) {
      return v3;
    }
    if (["rerun", "saveStrategy", "remember"].includes(type) && exact(["type", "turn"]) && str("turn")) {
      return v3;
    }
    if (type === "openAnswerCitation" && exact(["type", "turn", "record", "hit", "member"]) && str("turn") && typeof v3.record === "string" && /^[0-9a-f]{64}$/.test(v3.record) && Number.isSafeInteger(v3.hit) && Number(v3.hit) >= 0 && (v3.member === void 0 || Number.isSafeInteger(v3.member) && Number(v3.member) >= 0)) return v3;
    if (["open", "context"].includes(type) && exact(["type", "turn", "card"]) && str("turn") && str("card")) {
      return v3;
    }
    if (type === "continue" && exact(["type", "turn", "step"]) && str("turn") && str("step")) {
      return v3;
    }
    if (type === "followTarget" && exact(["type", "turn", "step", "target"]) && str("turn") && str("step") && str("target", 4096)) {
      return v3;
    }
    if (type === "followDefinition" && exact(["type", "turn", "step", "name", "path"]) && str("turn") && str("step") && str("name", 1024) && str("path", 4096)) {
      return v3;
    }
    if (["copy", "export"].includes(type) && exact(["type", "turn", "format", "cards"]) && ["json", "markdown"].includes(String(v3.format)) && (v3.turn === void 0 || str("turn")) && (v3.cards === void 0 || Array.isArray(v3.cards) && v3.cards.length <= 1e3 && v3.cards.every((id) => typeof id === "string" && id.length <= 128))) {
      return v3;
    }
  }

  // src/sourceVerification.ts
  function nativeVerification(raw) {
    const value = object2(raw);
    return value.schema === "airk.saved-source-verification/1" && value.method === "sha256-full-saved-source/1" && Array.isArray(value.sources) && value.sources.length <= 256 && Array.isArray(value.references) && value.references.length <= 4096 && Array.isArray(value.origins) && value.origins.length <= 32 ? value : void 0;
  }
  function verificationLabel(raw) {
    const value = nativeVerification(raw);
    if (!value) return "Saved-source verification unavailable.";
    const counts = object2(value.counts), sources = Array.isArray(value.sources) ? value.sources.map(object2) : [];
    const at = value.observed_to_unix_ms;
    const dated = Number.isSafeInteger(at) && Number(at) >= 0 && Number(at) <= 864e13 && Number.isSafeInteger(value.observed_from_unix_ms) && Number(value.observed_from_unix_ms) >= 0 && Number(value.observed_from_unix_ms) <= Number(at);
    const date = dated ? new Date(Number(at)).toISOString() : "unknown time";
    const references = Array.isArray(value.references) ? value.references.map(object2) : [];
    const origins = Array.isArray(value.origins) ? value.origins.map(object2) : [];
    const referenceIds = new Set(references.map((reference) => reference.id));
    const matched = Number.isSafeInteger(counts.matched_sources) && Number(counts.matched_sources) >= 0 ? Number(counts.matched_sources) : 0;
    const total = Number.isSafeInteger(counts.requested_sources) && Number(counts.requested_sources) >= 0 ? String(counts.requested_sources) : "unknown";
    const validMatch = dated && value.status === "verified" && value.requirements_satisfied === true && value.inventory_complete === true && value.all_references_accounted === true && value.source_rows_omitted === 0 && value.publication_fence === "current" && matched > 0 && matched === sources.length && matched === counts.requested_sources && counts.unchecked_sources === 0 && counts.checked_sources === matched && counts.mismatched_sources === 0 && counts.references === references.length && referenceIds.size === references.length && new Set(sources.map((source) => source.id)).size === sources.length && references.every((reference) => sources.some((source) => source.id === reference.source_id && Array.isArray(source.reference_ids) && source.reference_ids.includes(reference.id))) && new Set(origins.map((origin) => origin.id)).size === origins.length && sources.every((source) => source.state === "matched" && origins.some((origin) => origin.id === source.origin_id && typeof origin.publication === "string" && origin.publication.startsWith("airk-r1:")) && Number.isSafeInteger(source.observed_from_unix_ms) && Number.isSafeInteger(source.observed_to_unix_ms) && Number(source.observed_from_unix_ms) >= Number(value.observed_from_unix_ms) && Number(source.observed_to_unix_ms) >= Number(source.observed_from_unix_ms) && Number(source.observed_to_unix_ms) <= Number(at) && typeof source.original_root === "string" && /^(?:\/|[A-Za-z]:[\\/]|\\\\)/.test(source.original_root) && typeof source.source_relative_path === "string" && source.source_relative_path.length > 0 && !/[\\:\u0000]/.test(source.source_relative_path) && source.source_relative_path.split("/").every((part) => !["", ".", ".."].includes(part)) && typeof source.expected_hash === "string" && /^[0-9a-f]{64}$/.test(source.expected_hash) && source.expected_hash === source.observed_hash && Array.isArray(source.reference_ids) && source.reference_ids.length > 0 && source.reference_ids.every((id) => referenceIds.has(id)));
    if (validMatch) return "Saved sources verified at ".concat(date, ": ").concat(matched, " referenced source").concat(matched === 1 ? "" : "s", " matched the indexed bytes.");
    if (value.status === "not_applicable" && counts.references === 0 && counts.requested_sources === 0 && references.length === 0 && sources.length === 0 && value.inventory_complete === true) return "No returned saved-source references to verify (".concat(date, ").");
    return "Saved-source check at ".concat(date, ": ").concat(matched, " of ").concat(total, " referenced sources matched; verification incomplete.");
  }
  function jsonPointer(root, path) {
    if (typeof path !== "string" || path.length > 8192 || path !== "" && !path.startsWith("/")) return;
    let value = root;
    for (const escaped of path === "" ? [] : path.slice(1).split("/")) {
      if (/~[^01]|~$/.test(escaped)) return;
      const key = escaped.replaceAll("~1", "/").replaceAll("~0", "~");
      if (Array.isArray(value)) {
        if (!/^(0|[1-9][0-9]*)$/.test(key) || Number(key) >= value.length) return;
        value = value[Number(key)];
      } else {
        const row = object2(value);
        if (!Object.hasOwn(row, key)) return;
        value = row[key];
      }
    }
    return value;
  }
  function sameJson(left, right, depth = 0) {
    if (left === right) return true;
    if (depth > 32) return false;
    if (Array.isArray(left) || Array.isArray(right)) return Array.isArray(left) && Array.isArray(right) && left.length === right.length && left.length <= 4096 && left.every((item, index) => sameJson(item, right[index], depth + 1));
    if (!left || !right || typeof left !== "object" || typeof right !== "object") return false;
    const a3 = object2(left), b2 = object2(right), keys = Object.keys(a3);
    return keys.length <= 4096 && keys.length === Object.keys(b2).length && keys.every((key) => Object.hasOwn(b2, key) && sameJson(a3[key], b2[key], depth + 1));
  }
  function explicitIndexAt(projection, pointer) {
    let current = pointer;
    for (let depth = 0; depth <= 32; depth++) {
      const row = object2(jsonPointer(projection, current)), index = object2(row.origin).index_id ?? row.index_id;
      if (typeof index === "string") return index;
      if (!current) return;
      const slash = current.lastIndexOf("/");
      if (slash < 0) return;
      current = current.slice(0, slash);
    }
    return;
  }
  function selectedVerification(envelope, cards) {
    const candidates = [];
    const add = (body, step) => {
      candidates.push({ step, raw: body.source_verification });
      if ((body.schema === "airk.combined-read/1" || body.schema === "airk.combined-verification/1") && Array.isArray(body.members)) {
        const final = body.schema === "airk.combined-verification/1";
        for (const raw of body.members.slice(0, 32)) {
          const member = object2(raw), own = readResultBody(member.result);
          candidates.push({
            step,
            index: typeof member.index_id === "string" ? member.index_id : void 0,
            raw: own.source_verification,
            ...final ? { body: own, bindings: Array.isArray(member.reference_bindings) && member.reference_bindings.length <= 4096 ? member.reference_bindings.map(object2) : [], scope: body.scope_identity } : {}
          });
        }
      }
    };
    add(envelope);
    for (const raw of Array.isArray(envelope.steps) ? envelope.steps.slice(0, WORKBENCH_MAX_STEP_REPORTS) : []) {
      const report = object2(raw);
      add(readResultBody(report.result), typeof report.id === "string" ? report.id : void 0);
    }
    add(readResultBody(envelope.member_verification));
    const requested = envelope.member_verification !== void 0 || candidates.some((candidate) => candidate.raw !== void 0);
    const accounted = /* @__PURE__ */ new Set(), observations = [];
    let associationWork = 131072, scanComplete = true;
    for (const { step, index: memberIndex, raw, body, bindings, scope } of candidates) {
      const receipt = nativeVerification(raw);
      if (!receipt) continue;
      if (bindings !== void 0 && (typeof memberIndex !== "string" || !/^idx_[0-9a-f]{32}$/.test(memberIndex) || typeof scope !== "string" || !/^[0-9a-f]{64}$/.test(scope) || scope !== envelope.scope_identity)) continue;
      const originalSources = Array.isArray(receipt.sources) ? receipt.sources.map(object2) : [], originalOrigins = Array.isArray(receipt.origins) ? receipt.origins.map(object2) : [];
      const sources = [], references = [], originalAssociations = [];
      for (let index = 0; index < cards.length; index++) {
        const card = cards[index];
        if (step !== void 0 && card.step !== step || memberIndex !== void 0 && card.indexId !== memberIndex) continue;
        const cost = originalSources.length * (originalOrigins.length + 1) + (bindings === void 0 ? 0 : receipt.references.length * (bindings.length + 1));
        if (cost > associationWork) {
          scanComplete = false;
          break;
        }
        associationWork -= cost;
        const publication = card.publication ?? (typeof envelope.publication === "string" ? envelope.publication : void 0);
        const matches = originalSources.filter((source2) => source2.path === card.path && source2.expected_hash === card.hash && (source2.source_root_id == null || card.sourceRootId === void 0 || source2.source_root_id === card.sourceRootId) && (bindings === void 0 || card.sourceRootId !== void 0 && source2.source_root_id === card.sourceRootId) && publication !== void 0 && originalOrigins.some((origin) => origin.id === source2.origin_id && origin.publication === publication && (origin.index_id == null || memberIndex === void 0 || origin.index_id === memberIndex) && (bindings === void 0 || origin.index_id === memberIndex)));
        if (matches.length !== 1) continue;
        const source = matches[0], id = "ref-".concat(references.length);
        if (bindings !== void 0) {
          const recorded = receipt.references.map(object2).find((reference) => {
            if (reference.source_id !== source.id || !Array.isArray(source.reference_ids) || !source.reference_ids.includes(reference.id) || typeof reference.result_pointer !== "string") return false;
            const pointer = reference.result_pointer;
            let binding;
            for (const entry of bindings) {
              if (typeof entry.member_pointer === "string" && typeof entry.result_pointer === "string" && entry.member_pointer.length <= 4096 && entry.result_pointer.length <= 4096 && pointer.startsWith(entry.member_pointer) && (pointer.length === entry.member_pointer.length || pointer[entry.member_pointer.length] === "/") && (!binding || entry.member_pointer.length > String(binding.member_pointer).length)) binding = entry;
            }
            if (!binding) return false;
            const outer = String(binding.result_pointer) + pointer.slice(String(binding.member_pointer).length);
            const target = jsonPointer(envelope, outer), checked = jsonPointer(body, pointer), citation = object2(target);
            if (target == null || checked == null || !sameJson(target, checked) || explicitIndexAt(envelope, outer) !== memberIndex || citation.path !== card.path || citation.content_hash !== card.hash || citation.start_line !== card.start || citation.end_line !== card.end) return false;
            originalAssociations.push({ selected_reference_id: id, reference_id: reference.id, member_pointer: pointer, result_pointer: outer });
            return true;
          });
          if (!recorded) continue;
        }
        let retained = sources.find((retained2) => retained2.id === source.id);
        if (!retained) {
          retained = { ...source, reference_ids: [] };
          sources.push(retained);
        }
        retained.reference_ids.push(id);
        references.push({ id, source_id: source.id, result_pointer: "/evidence/".concat(index, "/citation"), kind: "citation" });
        accounted.add(card.id);
      }
      if (!sources.length) continue;
      const originIds = new Set(sources.map((source) => source.origin_id));
      observations.push({
        schema: "airk.saved-source-selected-observations/1",
        selection: true,
        verification_performed: false,
        ...memberIndex !== void 0 ? { index_id: memberIndex } : {},
        ...bindings !== void 0 ? { original_member_scope_identity: scope, original_reference_associations: originalAssociations } : {},
        original_status: receipt.status,
        original_counts: receipt.counts,
        original_requirements_satisfied: receipt.requirements_satisfied,
        original_inventory_complete: receipt.inventory_complete,
        original_all_references_accounted: receipt.all_references_accounted,
        original_source_rows_omitted: receipt.source_rows_omitted,
        original_scope_digest: receipt.scope_digest,
        original_selection_digest: receipt.selection_digest,
        observed_from_unix_ms: receipt.observed_from_unix_ms,
        observed_to_unix_ms: receipt.observed_to_unix_ms,
        method: receipt.method,
        limits: receipt.limits,
        publication_fence: receipt.publication_fence,
        origins: originalOrigins.filter((origin) => originIds.has(origin.id)),
        sources,
        references,
        qualifications: receipt.qualifications,
        ...receipt.error ? { original_error: receipt.error } : {}
      });
    }
    return { requested, observations, accounting: {
      scope: "selected_evidence_only",
      verification_performed: false,
      observation_scan_complete: scanComplete,
      ...!scanComplete ? { limits_hit: ["selected_observation_associations"] } : {},
      selected_citations: cards.length,
      with_retained_observations: accounted.size,
      without_retained_observations: cards.filter((card) => !accounted.has(card.id)).map((card) => card.id)
    } };
  }

  // src/publications.ts
  var U64_MAX = 18446744073709551615n;
  function parsed(publication, collection) {
    const envelope = publication !== null && typeof publication === "object" && !Array.isArray(publication) ? publication : void 0;
    const token = envelope ? envelope.token : publication;
    if (typeof token !== "string" || token.length > 126) return;
    const fields = /^airk-r1:([0-9a-f]{32}):(0|[1-9][0-9]{0,19}):([0-9a-f]{64})$/.exec(token);
    if (!fields || fields[0] !== token) return;
    const generation = BigInt(fields[2]);
    if (generation > U64_MAX) return;
    if (envelope) {
      if (envelope.incarnation !== void 0 && envelope.incarnation !== fields[1]) return;
      if (envelope.collection !== void 0 && envelope.collection !== collection) return;
      if (envelope.revision !== void 0 && envelope.revision !== fields[3]) return;
      if (envelope.generation !== void 0) {
        const supplied = envelope.generation;
        if (typeof supplied === "number") {
          if (!Number.isSafeInteger(supplied) || supplied < 0 || BigInt(supplied) !== generation) return;
        } else if (typeof supplied !== "string" || supplied !== fields[2]) return;
      }
    }
    return { incarnation: fields[1], generation, revision: fields[3] };
  }
  function comparePublication(result, observed) {
    if (!result || !observed || typeof result.authority !== "string" || !result.authority || typeof observed.authority !== "string" || !observed.authority || typeof result.collection !== "string" || !result.collection || typeof observed.collection !== "string" || !observed.collection) return "unknown";
    const a3 = parsed(result.publication, result.collection), b2 = parsed(observed.publication, observed.collection);
    if (!a3 || !b2) return "unknown";
    if (result.authority !== observed.authority || result.collection !== observed.collection || a3.incarnation !== b2.incarnation) return "different";
    if (a3.generation === b2.generation) return a3.revision === b2.revision ? "equal" : "inconsistent";
    return a3.generation < b2.generation ? "older" : "newer";
  }
  function publicationRelationLabel(relation) {
    switch (relation) {
      case "older":
        return "Result generation is older than the observed index generation. This does not establish ancestry or live-source freshness.";
      case "newer":
        return "Result generation is newer than the observed index generation. Readiness remains the separately observed status.";
      case "different":
        return "Result and observed index have different publication scopes; generation order is unavailable.";
      case "inconsistent":
        return "Result and observed index disagree at the same publication generation.";
      case "mixed":
        return "Some result members are older and others newer than the observed index publications. The combined observations cannot be ordered.";
      default:
        return;
    }
  }

  // src/workbenchAnswerNavigation.ts
  var digest = (v3) => typeof v3 === "string" && /^[0-9a-f]{64}$/.test(v3);
  var indexId = (v3) => typeof v3 === "string" && /^idx_[0-9a-f]{32}$/.test(v3);
  var projectIds = (v3) => Array.isArray(v3) && new Set(v3).size === v3.length && v3.every((id) => typeof id === "string" && /^prj_[0-9a-f]{32}$/.test(id));
  var sourceHitOperation = {
    search: true,
    context: true,
    read_evidence: true,
    definitions: false,
    references: false,
    document_sections: false
  };
  var equalPublication = (a3, b2, collection) => typeof collection === "string" && comparePublication({ authority: "native-answer", collection, publication: a3 }, { authority: "native-answer", collection, publication: b2 }) === "equal";
  function sameNativeJson(left, right) {
    if (left === right) return true;
    if (left === null || right === null || typeof left !== "object" || typeof right !== "object") return false;
    if (Array.isArray(left) || Array.isArray(right)) return Array.isArray(left) && Array.isArray(right) && left.length === right.length && left.every((value, index) => sameNativeJson(value, right[index]));
    const a3 = left, b2 = right, keys = Object.keys(a3);
    return keys.length === Object.keys(b2).length && keys.every((key) => Object.hasOwn(b2, key) && sameNativeJson(a3[key], b2[key]));
  }
  function contextPacket(raw) {
    const body = readResultBody(raw);
    if (body.schema !== "airk.arbiter.context/2" || !Array.isArray(body.records) || !Array.isArray(body.paragraphs) || !Array.isArray(body.assessment_history) || !Array.isArray(body.gaps) || !Object.keys(object2(body.assessment)).length) return;
    return body;
  }
  function isAnswerEnvelope(envelope) {
    return envelope.schema === "airk.arbiter.context/2" || envelope.schema === "airk.combined-read/1" && object2(envelope.context).schema === "airk.combined-context/1";
  }
  function answerPartitions(envelope) {
    if (envelope.schema === "airk.arbiter.context/2") return [{ outcome: "local", packet: contextPacket(envelope), raw: envelope }];
    if (!isAnswerEnvelope(envelope) || !Array.isArray(envelope.members)) return [];
    const members = envelope.members.map(object2), ids = members.map((member) => member.index_id);
    const vector = object2(envelope.publication_vector), publications = Array.isArray(vector.members) ? vector.members.map(object2) : [];
    return members.map((raw, member) => {
      const packet = contextPacket(raw.result), index = raw.index_id;
      const heads = publications.filter((head) => head.index_id === index), scope = object2(object2(packet?.assessment).scope), publication = object2(scope.publication);
      const bound = indexId(index) && ids.filter((id) => id === index).length === 1 && scope.index_id === index && heads.length === 1 && equalPublication(scope.publication, heads[0].publication, publication.collection) && packet?.records.every((record) => sameNativeJson(record.scope, scope) && object2(record.scope).index_id === index && equalPublication(object2(record.scope).publication, heads[0].publication, publication.collection));
      return {
        member,
        indexId: typeof index === "string" ? index : void 0,
        outcome: String(raw.outcome ?? "unknown"),
        ...bound ? { packet } : {},
        raw
      };
    });
  }
  function sourceAtPointer(root, pointer) {
    if (!pointer.startsWith("/")) return;
    let current = root;
    for (const token of pointer.slice(1).split("/")) {
      if (/~[^01]|~$/.test(token)) return;
      const key = token.replaceAll("~1", "/").replaceAll("~0", "~");
      if (Array.isArray(current)) {
        if (!/^(0|[1-9][0-9]*)$/.test(key) || Number(key) >= current.length) return;
        current = current[Number(key)];
      } else {
        const row = object2(current);
        if (!Object.hasOwn(row, key)) return;
        current = row[key];
      }
    }
    return current !== null && typeof current === "object" && !Array.isArray(current) ? current : void 0;
  }
  function answerSourceBinding(body, path, hash, index, publication, pointer, receiptOwner = body) {
    const receipt = nativeVerification(receiptOwner.source_verification);
    if (!receipt) return;
    if (pointer !== void 0) {
      const actual = sourceAtPointer(receiptOwner, pointer);
      if (actual?.path !== path || actual.content_hash !== hash) return;
    }
    const references = receipt.references.map(object2);
    const sources = receipt.sources.map(object2);
    const origins = receipt.origins.map(object2);
    if (new Set(sources.map((source2) => source2.id)).size !== sources.length || new Set(origins.map((origin2) => origin2.id)).size !== origins.length || new Set(references.map((reference) => reference.id)).size !== references.length || !equalPublication(body.publication, publication, body.collection)) return;
    const matching = sources.filter((source2) => source2.path === path && source2.expected_hash === hash && typeof source2.source_root_id === "string" && /^src_[0-9a-f]{32}$/.test(source2.source_root_id) && typeof source2.source_relative_path === "string" && source2.source_relative_path.length > 0 && !/[\\:\u0000]/.test(source2.source_relative_path) && source2.source_relative_path.split("/").every((part) => !["", ".", ".."].includes(part)) && origins.filter((origin2) => origin2.id === source2.origin_id && origin2.index_id === index && equalPublication(origin2.publication, publication, body.collection) && projectIds(origin2.project_ids)).length === 1 && (pointer === void 0 || references.filter((reference) => reference.result_pointer === pointer && reference.source_id === source2.id && Array.isArray(source2.reference_ids) && source2.reference_ids.includes(reference.id)).length === 1));
    if (matching.length !== 1) return;
    const source = matching[0], origin = origins.find((origin2) => origin2.id === source.origin_id);
    return {
      index_id: index,
      project_ids: origin.project_ids,
      source_root_id: source.source_root_id,
      source_relative_path: source.source_relative_path,
      publication
    };
  }
  function answerEvidenceRows(envelope) {
    return answerEvidenceRowsAt(envelope);
  }
  function structuredBodyPrefix(raw) {
    const value = object2(raw);
    if (Array.isArray(value.content) && Object.keys(object2(value.structuredContent)).length) return "/structuredContent";
    return readResultBody(raw) === value ? "" : void 0;
  }
  function answerEvidenceRowsAt(envelope, inherited) {
    const custody = inherited ?? (Object.hasOwn(envelope, "source_verification") ? { owner: envelope, prefix: "" } : void 0);
    if (envelope.schema === "airk.combined-read/1") return answerPartitions(envelope).flatMap((partition) => {
      if (!partition.packet || partition.outcome !== "complete" || partition.member === void 0) return [];
      const result = partition.raw.result, prefix = structuredBodyPrefix(result);
      if (prefix === void 0) return [];
      const child = custody ? { owner: custody.owner, prefix: "".concat(custody.prefix, "/members/").concat(partition.member, "/result").concat(prefix) } : void 0;
      return answerEvidenceRowsAt(partition.packet, child).map((row) => ({ ...row, answer_member: partition.member }));
    });
    if (envelope.schema !== "airk.arbiter.context/2" || !Array.isArray(envelope.records)) return [];
    const records = envelope.records.map(object2), ids = records.map((record) => record.id);
    const scope = object2(object2(envelope.assessment).scope);
    if (records.some((record) => !sameNativeJson(record.scope, scope))) return [];
    if (ids.some((id) => !digest(id)) || new Set(ids).size !== ids.length) return [];
    return records.flatMap((record, recordPosition) => {
      if (!Object.hasOwn(sourceHitOperation, String(record.operation)) || !sourceHitOperation[record.operation]) return [];
      const scope2 = object2(record.scope), body = object2(record.canonical_result), publication = object2(scope2.publication);
      const observation = object2(body.scope_observation), members = Array.isArray(observation.members) ? observation.members.map(object2) : [];
      const own = members.filter((member) => member.index_id === scope2.index_id);
      if (!indexId(scope2.index_id) || !digest(scope2.authorization) || observation.fingerprint !== scope2.authorization || own.length !== 1 || !equalPublication(body.publication, publication, body.collection) || !Array.isArray(body.hits)) return [];
      return body.hits.flatMap((raw, hit) => {
        const row = object2(raw), citation = object2(row.citation);
        if (typeof citation.path !== "string" || !digest(citation.content_hash)) return [];
        const origin = answerSourceBinding(
          body,
          citation.path,
          citation.content_hash,
          String(scope2.index_id),
          publication,
          custody ? "".concat(custody.prefix, "/records/").concat(recordPosition, "/canonical_result/hits/").concat(hit, "/citation") : "/hits/".concat(hit, "/citation"),
          custody?.owner ?? body
        );
        const memberships = Array.isArray(own[0].memberships) ? own[0].memberships.map(object2) : [];
        if (!origin || !Array.isArray(own[0].source_root_ids) || !own[0].source_root_ids.includes(origin.source_root_id) || !origin.project_ids.every((id) => memberships.some((member) => member.project_id === id))) return [];
        return [{ ...row, origin, publication: publication.token, step: record.id, result_pointer: "/hits/".concat(hit) }];
      });
    });
  }

  // node_modules/preact/dist/preact.module.js
  var n;
  var l;
  var u;
  var t;
  var i;
  var r;
  var o;
  var e;
  var f;
  var c;
  var a;
  var s;
  var h;
  var p;
  var v;
  var y;
  var d = {};
  var w = [];
  var _ = /acit|ex(?:s|g|n|p|$)|rph|grid|ows|mnc|ntw|ine[ch]|zoo|^ord|itera/i;
  var g = Array.isArray;
  function m(n2, l3) {
    for (var u4 in l3) n2[u4] = l3[u4];
    return n2;
  }
  function b(n2) {
    n2 && n2.parentNode && n2.parentNode.removeChild(n2);
  }
  function k(l3, u4, t3) {
    var i3, r3, o3, e3 = {};
    for (o3 in u4) "key" == o3 ? i3 = u4[o3] : "ref" == o3 ? r3 = u4[o3] : e3[o3] = u4[o3];
    if (arguments.length > 2 && (e3.children = arguments.length > 3 ? n.call(arguments, 2) : t3), "function" == typeof l3 && null != l3.defaultProps) for (o3 in l3.defaultProps) void 0 === e3[o3] && (e3[o3] = l3.defaultProps[o3]);
    return x(l3, e3, i3, r3, null);
  }
  function x(n2, t3, i3, r3, o3) {
    var e3 = { type: n2, props: t3, key: i3, ref: r3, __k: null, __: null, __b: 0, __e: null, __c: null, constructor: void 0, __v: null == o3 ? ++u : o3, __i: -1, __u: 0 };
    return null == o3 && null != l.vnode && l.vnode(e3), e3;
  }
  function S(n2) {
    return n2.children;
  }
  function C(n2, l3) {
    this.props = n2, this.context = l3;
  }
  function $(n2, l3) {
    if (null == l3) return n2.__ ? $(n2.__, n2.__i + 1) : null;
    for (var u4; l3 < n2.__k.length; l3++) if (null != (u4 = n2.__k[l3]) && null != u4.__e) return u4.__e;
    return "function" == typeof n2.type ? $(n2) : null;
  }
  function I(n2) {
    if (n2.__P && n2.__d) {
      var u4 = n2.__v, t3 = u4.__e, i3 = [], r3 = [], o3 = m({}, u4);
      o3.__v = u4.__v + 1, l.vnode && l.vnode(o3), q(n2.__P, o3, u4, n2.__n, n2.__P.namespaceURI, 32 & u4.__u ? [t3] : null, i3, null == t3 ? $(u4) : t3, !!(32 & u4.__u), r3), o3.__v = u4.__v, o3.__.__k[o3.__i] = o3, D(i3, o3, r3), u4.__e = u4.__ = null, o3.__e != t3 && P(o3);
    }
  }
  function P(n2) {
    if (null != (n2 = n2.__) && null != n2.__c) return n2.__e = n2.__c.base = null, n2.__k.some(function(l3) {
      if (null != l3 && null != l3.__e) return n2.__e = n2.__c.base = l3.__e;
    }), P(n2);
  }
  function A(n2) {
    (!n2.__d && (n2.__d = true) && i.push(n2) && !H.__r++ || r != l.debounceRendering) && ((r = l.debounceRendering) || o)(H);
  }
  function H() {
    try {
      for (var n2, l3 = 1; i.length; ) i.length > l3 && i.sort(e), n2 = i.shift(), l3 = i.length, I(n2);
    } finally {
      i.length = H.__r = 0;
    }
  }
  function L(n2, l3, u4, t3, i3, r3, o3, e3, f4, c3, a3) {
    var s3, h3, p3, v3, y3, _3, g2 = t3 && t3.__k || w, m3 = l3.length;
    for (f4 = T(u4, l3, g2, f4, m3), s3 = 0; s3 < m3; s3++) null != (p3 = u4.__k[s3]) && (h3 = -1 != p3.__i && g2[p3.__i] || d, p3.__i = s3, _3 = q(n2, p3, h3, i3, r3, o3, e3, f4, c3, a3), v3 = p3.__e, p3.ref && h3.ref != p3.ref && (h3.ref && J(h3.ref, null, p3), a3.push(p3.ref, p3.__c || v3, p3)), null == y3 && null != v3 && (y3 = v3), 4 & p3.__u ? (f4 = j(p3, f4, n2), h3.__e && (h3.__e = null)) : "function" == typeof p3.type && void 0 !== _3 ? f4 = _3 : v3 && (f4 = v3.nextSibling), p3.__u &= -7);
    return u4.__e = y3, f4;
  }
  function T(n2, l3, u4, t3, i3) {
    var r3, o3, e3, f4, c3, a3 = u4.length, s3 = a3, h3 = 0;
    for (n2.__k = new Array(i3), r3 = 0; r3 < i3; r3++) null != (o3 = l3[r3]) && "boolean" != typeof o3 && "function" != typeof o3 ? ("string" == typeof o3 || "number" == typeof o3 || "bigint" == typeof o3 || o3.constructor == String ? o3 = n2.__k[r3] = x(null, o3, null, null, null) : g(o3) ? o3 = n2.__k[r3] = x(S, { children: o3 }, null, null, null) : void 0 === o3.constructor && o3.__b > 0 ? o3 = n2.__k[r3] = x(o3.type, o3.props, o3.key, o3.ref ? o3.ref : null, o3.__v) : n2.__k[r3] = o3, f4 = r3 + h3, o3.__ = n2, o3.__b = n2.__b + 1, e3 = null, -1 != (c3 = o3.__i = O(o3, u4, f4, s3)) && (s3--, (e3 = u4[c3]) && (e3.__u |= 2)), null == e3 || null == e3.__v ? (-1 == c3 && (i3 > a3 ? h3-- : i3 < a3 && h3++), "function" != typeof o3.type && (o3.__u |= 4)) : c3 != f4 && (c3 == f4 - 1 ? h3-- : c3 == f4 + 1 ? h3++ : (c3 > f4 ? h3-- : h3++, o3.__u |= 4))) : n2.__k[r3] = null;
    if (s3) for (r3 = 0; r3 < a3; r3++) null != (e3 = u4[r3]) && 0 == (2 & e3.__u) && (e3.__e == t3 && (t3 = $(e3)), K(e3, e3));
    return t3;
  }
  function j(n2, l3, u4) {
    var t3, i3;
    if ("function" == typeof n2.type) {
      for (t3 = n2.__k, i3 = 0; t3 && i3 < t3.length; i3++) t3[i3] && (t3[i3].__ = n2, l3 = j(t3[i3], l3, u4));
      return l3;
    }
    n2.__e != l3 && (l3 && n2.type && !l3.parentNode && (l3 = $(n2)), l3 = u4.insertBefore(n2.__e, l3 || null));
    do {
      l3 = l3 && l3.nextSibling;
    } while (null != l3 && 8 == l3.nodeType);
    return l3;
  }
  function O(n2, l3, u4, t3) {
    var i3, r3, o3, e3 = n2.key, f4 = n2.type, c3 = l3[u4], a3 = null != c3 && 0 == (2 & c3.__u);
    if (null === c3 && null == e3 || a3 && e3 == c3.key && f4 == c3.type) return u4;
    if (t3 > (a3 ? 1 : 0)) {
      for (i3 = u4 - 1, r3 = u4 + 1; i3 >= 0 || r3 < l3.length; ) if (null != (c3 = l3[o3 = i3 >= 0 ? i3-- : r3++]) && 0 == (2 & c3.__u) && e3 == c3.key && f4 == c3.type) return o3;
    }
    return -1;
  }
  function z(n2, l3, u4) {
    "-" == l3[0] ? n2.setProperty(l3, null == u4 ? "" : u4) : n2[l3] = null == u4 ? "" : "number" != typeof u4 || _.test(l3) ? u4 : u4 + "px";
  }
  function N(n2, l3, u4, t3, i3) {
    var r3, o3;
    n: if ("style" == l3) if ("string" == typeof u4) n2.style.cssText = u4;
    else {
      if ("string" == typeof t3 && (n2.style.cssText = t3 = ""), t3) for (l3 in t3) u4 && l3 in u4 || z(n2.style, l3, "");
      if (u4) for (l3 in u4) t3 && u4[l3] == t3[l3] || z(n2.style, l3, u4[l3]);
    }
    else if ("o" == l3[0] && "n" == l3[1]) r3 = l3 != (l3 = l3.replace(s, "$1")), o3 = l3.toLowerCase(), l3 = o3 in n2 || "onFocusOut" == l3 || "onFocusIn" == l3 ? o3.slice(2) : l3.slice(2), n2.l || (n2.l = {}), n2.l[l3 + r3] = u4, u4 ? t3 ? u4[a] = t3[a] : (u4[a] = h, n2.addEventListener(l3, r3 ? v : p, r3)) : n2.removeEventListener(l3, r3 ? v : p, r3);
    else {
      if ("http://www.w3.org/2000/svg" == i3) l3 = l3.replace(/xlink(H|:h)/, "h").replace(/sName$/, "s");
      else if ("width" != l3 && "height" != l3 && "href" != l3 && "list" != l3 && "form" != l3 && "tabIndex" != l3 && "download" != l3 && "rowSpan" != l3 && "colSpan" != l3 && "role" != l3 && "popover" != l3 && l3 in n2) try {
        n2[l3] = null == u4 ? "" : u4;
        break n;
      } catch (n3) {
      }
      "function" == typeof u4 || (null == u4 || false === u4 && "-" != l3[4] ? n2.removeAttribute(l3) : n2.setAttribute(l3, "popover" == l3 && 1 == u4 ? "" : u4));
    }
  }
  function V(n2) {
    return function(u4) {
      if (this.l) {
        var t3 = this.l[u4.type + n2];
        if (null == u4[c]) u4[c] = h++;
        else if (u4[c] < t3[a]) return;
        return t3(l.event ? l.event(u4) : u4);
      }
    };
  }
  function q(n2, u4, t3, i3, r3, o3, e3, f4, c3, a3) {
    var s3, h3, p3, v3, y3, d3, _3, k3, x2, M, I2, P2, A3, H2, T3, j3, F = u4.type;
    if (void 0 !== u4.constructor) return null;
    128 & t3.__u && (c3 = !!(32 & t3.__u), o3 = [f4 = u4.__e = t3.__e]), (s3 = l.__b) && s3(u4);
    n: if ("function" == typeof F) {
      h3 = e3.length;
      try {
        if (x2 = u4.props, M = F.prototype && F.prototype.render, I2 = (s3 = F.contextType) && i3[s3.__c], P2 = s3 ? I2 ? I2.props.value : s3.__ : i3, t3.__c ? k3 = (p3 = u4.__c = t3.__c).__ = p3.__E : (M ? u4.__c = p3 = new F(x2, P2) : (u4.__c = p3 = new C(x2, P2), p3.constructor = F, p3.render = Q), I2 && I2.sub(p3), p3.state || (p3.state = {}), p3.__n = i3, v3 = p3.__d = true, p3.__h = [], p3._sb = []), M && null == p3.__s && (p3.__s = p3.state), M && null != F.getDerivedStateFromProps && (p3.__s == p3.state && (p3.__s = m({}, p3.__s)), m(p3.__s, F.getDerivedStateFromProps(x2, p3.__s))), y3 = p3.props, d3 = p3.state, p3.__v = u4, v3) M && null == F.getDerivedStateFromProps && null != p3.componentWillMount && p3.componentWillMount(), M && null != p3.componentDidMount && p3.__h.push(p3.componentDidMount);
        else {
          if (M && null == F.getDerivedStateFromProps && x2 !== y3 && null != p3.componentWillReceiveProps && p3.componentWillReceiveProps(x2, P2), u4.__v == t3.__v || !p3.__e && null != p3.shouldComponentUpdate && false === p3.shouldComponentUpdate(x2, p3.__s, P2)) {
            u4.__v != t3.__v && (p3.props = x2, p3.state = p3.__s, p3.__d = false), u4.__e = t3.__e, u4.__k = t3.__k, u4.__k.some(function(n3) {
              n3 && (n3.__ = u4);
            }), w.push.apply(p3.__h, p3._sb), p3._sb = [], p3.__h.length && e3.push(p3), f4 = $(t3);
            break n;
          }
          null != p3.componentWillUpdate && p3.componentWillUpdate(x2, p3.__s, P2), M && null != p3.componentDidUpdate && p3.__h.push(function() {
            p3.componentDidUpdate(y3, d3, _3);
          });
        }
        if (p3.context = P2, p3.props = x2, p3.__P = n2, p3.__e = false, A3 = l.__r, H2 = 0, M) p3.state = p3.__s, p3.__d = false, A3 && A3(u4), s3 = p3.render(p3.props, p3.state, p3.context), w.push.apply(p3.__h, p3._sb), p3._sb = [];
        else do {
          p3.__d = false, A3 && A3(u4), s3 = p3.render(p3.props, p3.state, p3.context), p3.state = p3.__s;
        } while (p3.__d && ++H2 < 25);
        p3.state = p3.__s, null != p3.getChildContext && (i3 = m(m({}, i3), p3.getChildContext())), M && !v3 && null != p3.getSnapshotBeforeUpdate && (_3 = p3.getSnapshotBeforeUpdate(y3, d3)), T3 = null != s3 && s3.type === S && null == s3.key ? E(s3.props.children) : s3, f4 = L(n2, g(T3) ? T3 : [T3], u4, t3, i3, r3, o3, e3, f4, c3, a3), p3.base = u4.__e, u4.__u &= -161, p3.__h.length && e3.push(p3), k3 && (p3.__E = p3.__ = null);
      } catch (n3) {
        if (e3.length = h3, u4.__v = null, c3 || null != o3) {
          if (n3.then) {
            for (u4.__u |= c3 ? 160 : 128; f4 && 8 == f4.nodeType && f4.nextSibling; ) f4 = f4.nextSibling;
            null != o3 && (o3[o3.indexOf(f4)] = null), u4.__e = f4;
          } else if (null != o3) for (j3 = o3.length; j3--; ) b(o3[j3]);
        } else u4.__e = t3.__e;
        null == u4.__k && (u4.__k = t3.__k || []), n3.then || B(u4), l.__e(n3, u4, t3);
      }
    } else null == o3 && u4.__v == t3.__v ? (u4.__k = t3.__k, u4.__e = t3.__e) : f4 = u4.__e = G(t3.__e, u4, t3, i3, r3, o3, e3, c3, a3);
    return (s3 = l.diffed) && s3(u4), 128 & u4.__u ? void 0 : f4;
  }
  function B(n2) {
    n2 && (n2.__c && (n2.__c.__e = true), n2.__k && n2.__k.some(B));
  }
  function D(n2, u4, t3) {
    for (var i3 = 0; i3 < t3.length; i3++) J(t3[i3], t3[++i3], t3[++i3]);
    l.__c && l.__c(u4, n2), n2.some(function(u5) {
      try {
        n2 = u5.__h, u5.__h = [], n2.some(function(n3) {
          n3.call(u5);
        });
      } catch (n3) {
        l.__e(n3, u5.__v);
      }
    });
  }
  function E(n2) {
    return "object" != typeof n2 || null == n2 || n2.__b > 0 ? n2 : g(n2) ? n2.map(E) : void 0 !== n2.constructor ? null : m({}, n2);
  }
  function G(u4, t3, i3, r3, o3, e3, f4, c3, a3) {
    var s3, h3, p3, v3, y3, w3, _3, m3 = i3.props || d, k3 = t3.props, x2 = t3.type;
    if ("svg" == x2 ? o3 = "http://www.w3.org/2000/svg" : "math" == x2 ? o3 = "http://www.w3.org/1998/Math/MathML" : o3 || (o3 = "http://www.w3.org/1999/xhtml"), null != e3) {
      for (s3 = 0; s3 < e3.length; s3++) if ((y3 = e3[s3]) && "setAttribute" in y3 == !!x2 && (x2 ? y3.localName == x2 : 3 == y3.nodeType)) {
        u4 = y3, e3[s3] = null;
        break;
      }
    }
    if (null == u4) {
      if (null == x2) return document.createTextNode(k3);
      u4 = document.createElementNS(o3, x2, k3.is && k3), c3 && (l.__m && l.__m(t3, e3), c3 = false), e3 = null;
    }
    if (null == x2) m3 === k3 || c3 && u4.data == k3 || (u4.data = k3);
    else {
      if (e3 = "textarea" == x2 && null != k3.defaultValue ? null : e3 && n.call(u4.childNodes), !c3 && null != e3) for (m3 = {}, s3 = 0; s3 < u4.attributes.length; s3++) m3[(y3 = u4.attributes[s3]).name] = y3.value;
      for (s3 in m3) y3 = m3[s3], "dangerouslySetInnerHTML" == s3 ? p3 = y3 : "children" == s3 || s3 in k3 || "value" == s3 && "defaultValue" in k3 || "checked" == s3 && "defaultChecked" in k3 || N(u4, s3, null, y3, o3);
      for (s3 in k3) y3 = k3[s3], "children" == s3 ? v3 = y3 : "dangerouslySetInnerHTML" == s3 ? h3 = y3 : "value" == s3 ? w3 = y3 : "checked" == s3 ? _3 = y3 : c3 && "function" != typeof y3 || m3[s3] === y3 || N(u4, s3, y3, m3[s3], o3);
      if (h3) c3 || p3 && (h3.__html == p3.__html || h3.__html == u4.innerHTML) || (u4.innerHTML = h3.__html), t3.__k = [];
      else if (p3 && (u4.innerHTML = ""), L("template" == t3.type ? u4.content : u4, g(v3) ? v3 : [v3], t3, i3, r3, "foreignObject" == x2 ? "http://www.w3.org/1999/xhtml" : o3, e3, f4, e3 ? e3[0] : i3.__k && $(i3, 0), c3, a3), null != e3) for (s3 = e3.length; s3--; ) b(e3[s3]);
      c3 && "textarea" != x2 || (s3 = "value", "progress" == x2 && null == w3 ? u4.removeAttribute("value") : null != w3 && (w3 !== u4[s3] || "progress" == x2 && !w3 || "option" == x2 && w3 != m3[s3]) && N(u4, s3, w3, m3[s3], o3), s3 = "checked", null != _3 && _3 != u4[s3] && N(u4, s3, _3, m3[s3], o3));
    }
    return u4;
  }
  function J(n2, u4, t3) {
    try {
      if ("function" == typeof n2) {
        var i3 = "function" == typeof n2.__u;
        i3 && n2.__u(), i3 && null == u4 || (n2.__u = n2(u4));
      } else n2.current = u4;
    } catch (n3) {
      l.__e(n3, t3);
    }
  }
  function K(n2, u4, t3) {
    var i3, r3;
    if (l.unmount && l.unmount(n2), (i3 = n2.ref) && (i3.current && i3.current != n2.__e || J(i3, null, u4)), null != (i3 = n2.__c)) {
      if (i3.componentWillUnmount) try {
        i3.componentWillUnmount();
      } catch (n3) {
        l.__e(n3, u4);
      }
      i3.base = i3.__P = i3.__n = null;
    }
    if (i3 = n2.__k) for (r3 = 0; r3 < i3.length; r3++) i3[r3] && K(i3[r3], u4, t3 || "function" != typeof n2.type);
    t3 || b(n2.__e), n2.__c = n2.__ = n2.__e = void 0;
  }
  function Q(n2, l3, u4) {
    return this.constructor(n2, u4);
  }
  function R(u4, t3, i3) {
    var r3, o3, e3, f4;
    t3 == document && (t3 = document.documentElement), l.__ && l.__(u4, t3), o3 = (r3 = "function" == typeof i3) ? null : i3 && i3.__k || t3.__k, e3 = [], f4 = [], q(t3, u4 = (!r3 && i3 || t3).__k = k(S, null, [u4]), o3 || d, d, t3.namespaceURI, !r3 && i3 ? [i3] : o3 ? null : t3.firstChild ? n.call(t3.childNodes) : null, e3, !r3 && i3 ? i3 : o3 ? o3.__e : t3.firstChild, r3, f4), D(e3, u4, f4), u4.props.children = null;
  }
  n = w.slice, l = { __e: function(n2, l3, u4, t3) {
    for (var i3, r3, o3; l3 = l3.__; ) if ((i3 = l3.__c) && !i3.__) try {
      if ((r3 = i3.constructor) && null != r3.getDerivedStateFromError && (i3.setState(r3.getDerivedStateFromError(n2)), o3 = i3.__d), null != i3.componentDidCatch && (i3.componentDidCatch(n2, t3 || {}), o3 = i3.__d), o3) return i3.__E = i3;
    } catch (l4) {
      n2 = l4;
    }
    throw n2;
  } }, u = 0, t = function(n2) {
    return null != n2 && void 0 === n2.constructor;
  }, C.prototype.setState = function(n2, l3) {
    var u4;
    u4 = null != this.__s && this.__s != this.state ? this.__s : this.__s = m({}, this.state), "function" == typeof n2 && (n2 = n2(m({}, u4), this.props)), n2 && m(u4, n2), null != n2 && this.__v && (l3 && this._sb.push(l3), A(this));
  }, C.prototype.forceUpdate = function(n2) {
    this.__v && (this.__e = true, n2 && this.__h.push(n2), A(this));
  }, C.prototype.render = S, i = [], o = "function" == typeof Promise ? Promise.prototype.then.bind(Promise.resolve()) : setTimeout, e = function(n2, l3) {
    return n2.__v.__b - l3.__v.__b;
  }, H.__r = 0, f = Math.random().toString(8), c = "__d" + f, a = "__a" + f, s = /(PointerCapture)$|Capture$/i, h = 0, p = V(false), v = V(true), y = 0;

  // ../ui/src/answerPresentation.ts
  var node = (tag, ...children) => ({ tag, children });
  var words = (value) => value.replaceAll("_", " ");
  var gapLabel = (gap) => gap === "unsupported" ? "Unclassified need (native unsupported)" : words(gap);
  var assessmentApplicability = {
    current: "Current local assessment",
    historical_after_read: "Historical after a later read; no current coverage claim",
    historical_after_configuration_change: "Historical after configuration changed; no current coverage claim",
    basis_omitted: "Basis omitted from this packet; no current coverage claim"
  };
  function answerPresentation(answer, identity) {
    const ids = new Set(answer.records.map((record) => record.id));
    const target = (id) => "answer-".concat(identity, "-").concat(id);
    const citation = (id) => ids.has(id) ? { ...node("button", "Read evidence ".concat(id.slice(0, 12))), citationTarget: target(id) } : node("span", "Unresolved native evidence citation: ".concat(id));
    const unit = (reference) => node(
      "div",
      citation(reference.record),
      node("span", "Native ".concat(reference.kind, " ").concat(reference.result_pointer, "; UTF-8 bytes ").concat(reference.utf8_start, "–").concat(reference.utf8_end, "."))
    );
    const measuredWork = (work) => node(
      "div",
      node("p", "Recorded input-token lower bound: ".concat(work.input_tokens, ". Nested work belongs to this assessment attempt and is already accounted in the native response totals.")),
      node("p", "Measured phase times (microseconds): queue ".concat(work.queue_us == null ? "unobserved" : work.queue_us, "; compile ").concat(work.compile_us == null ? "unobserved" : work.compile_us, "; inference ").concat(work.inference_us == null ? "unobserved" : work.inference_us, "; policy ").concat(work.policy_us == null ? "unobserved" : work.policy_us, "."))
    );
    const adviceReceipt = (advice) => node(
      "details",
      node("summary", "Native advice receipt · reserved ".concat(advice.reserved_input_tokens, " tokens · observed ").concat(advice.observed_input_tokens === null ? "unknown" : advice.observed_input_tokens)),
      node("p", "Reserved input tokens: ".concat(advice.reserved_input_tokens, ". Observed input tokens: ").concat(advice.observed_input_tokens === null ? "unknown; the known lower bound does not establish zero actual use" : advice.observed_input_tokens, ".")),
      measuredWork(advice.work),
      node("p", "Profile policy: ".concat(advice.policy, ". Task policy: ").concat(advice.task_policy, ". Compiler: ").concat(advice.compiler, ". Calibration: ").concat(advice.calibration, ". Input: ").concat(advice.input_digest, ".")),
      node("pre", JSON.stringify(advice, null, 2))
    );
    const event = (observed) => {
      const result = observed.result;
      const advice = result.advice;
      const unknownWork = result.status === "unavailable" && result.work_unknown || advice?.observed_input_tokens === null;
      const applicability = assessmentApplicability[observed.applicability];
      const summary = result.status === "selected" ? "Selected ".concat(words(result.card.action.kind), " · gap ").concat(gapLabel(result.card.gap)) : result.status === "unavailable" ? "Unavailable: ".concat(words(result.reason)).concat(unknownWork ? " · work unknown" : "") : "Abstained · no selected card";
      return node(
        "details",
        node("summary", "Assessment ".concat(observed.ordinal, " · ").concat(applicability, " · ").concat(summary)),
        node("p", "Partition ".concat(observed.partition.index_id, "; publication ").concat(observed.partition.publication.token, ".")),
        node("p", "Source units: ".concat(observed.basis.source_units_presented.length, " presented of ").concat(observed.basis.source_units_available, "; ").concat(observed.basis.source_units_partial, " partial; ").concat(observed.basis.source_units_omitted, " omitted. Read choices: ").concat(observed.basis.read_choices_scored.length, " scored of ").concat(observed.basis.read_choices_available, "; ").concat(observed.basis.read_choices_omitted, " omitted.")),
        node("p", "These counts describe this bounded native recipe, not all facts in the index."),
        ...result.status === "selected" ? [
          node("p", "Recorded action: ".concat(words(result.card.action.kind)).concat(result.card.action.kind === "read" ? " · native choice ".concat(result.card.action.choice) : "", ". Gap: ").concat(gapLabel(result.card.gap), ". Reason: ").concat(words(result.card.reason), ".")),
          node("p", "Roles and relevance below are whole-card hypotheses about the question, not new source facts or exhaustive coverage."),
          ...result.card.assessments.map((assessment) => node("div", node("p", "Role hypothesis: ".concat(words(assessment.role), ". Relevance hypothesis: ").concat(words(assessment.relevance), ".")), unit(assessment.evidence)))
        ] : result.status === "unavailable" ? [
          node("p", "Assessment not applied: ".concat(words(result.reason), ". ").concat(unknownWork ? "Actual optional work may be unknown; no zero-cost or replay claim." : "Known measured optional work is retained below."))
        ] : [node("p", "Native assessment abstained; it supplied no selected whole-card claim.")],
        ...advice !== null ? [adviceReceipt(advice)] : [
          node("p", "No actual native advice reply was retained. Token reservation and observed-token receipt were not returned; no learning result is claimed."),
          ...result.status === "unavailable" ? [measuredWork(result.work)] : []
        ],
        node("details", node("summary", "Complete assessment and provenance"), node("pre", JSON.stringify(observed, null, 2)))
      );
    };
    const coverage = answer.assessment.coverage;
    return node(
      "section",
      node("h3", "Evidence-grounded context"),
      node("p", "Local stop: ".concat(words(answer.stop), ". Candidates: ").concat(answer.candidate_count, "; omitted: ").concat(answer.omitted_count, ". Evidence remains bounded by the original request.")),
      node("h4", "Local partition assessment"),
      node("p", "Index ".concat(answer.assessment.scope.index_id, "; ").concat(answer.assessment.scope.publication.collection, "; publication ").concat(answer.assessment.scope.publication.token, ".")),
      node("p", "Termination: ".concat(words(answer.assessment.termination), ". ").concat(coverage.status === "assessed" ? "Native coverage assessment refers to local ordinal ".concat(coverage.ordinal, ".") : "Coverage unassessed: ".concat(words(coverage.reason), "."))),
      node("p", "This local partition does not establish coverage for other authorized indexes. A sufficiency assessment is fallible, not exhaustive coverage."),
      ...answer.paragraphs.map((paragraph) => node(
        "div",
        node("p", paragraph.text),
        node("p", paragraph.interpretation ? "Interpretation of the cited evidence." : "Evidence-grounded statement."),
        node("div", ...paragraph.evidence.map(citation))
      )),
      node("h4", "Gaps and limits"),
      answer.gaps.length ? node("ul", ...answer.gaps.map((gap) => node("li", gapLabel(gap)))) : node("p", "No additional gap was reported. This does not establish exhaustive coverage."),
      node("h4", "Assessment history"),
      ...answer.assessment_history.map(event),
      node("h4", "Original evidence records"),
      ...answer.records.map((record) => node(
        "details",
        node("summary", "".concat(record.operation, " · ").concat(record.role, " · ").concat(record.id.slice(0, 12))),
        { ...node("article", node("h5", record.id), node("pre", JSON.stringify(record, null, 2))), id: target(record.id) }
      )),
      node(
        "details",
        node("summary", "Native work, application and assessment observations"),
        node("pre", JSON.stringify({ work: answer.work, applications: answer.applications, assessment: answer.assessment, assessment_history: answer.assessment_history }, null, 2))
      )
    );
  }

  // node_modules/preact/hooks/dist/hooks.module.js
  var t2;
  var r2;
  var u2;
  var i2;
  var o2 = 0;
  var f2 = [];
  var c2 = l;
  var e2 = c2.__b;
  var a2 = c2.__r;
  var v2 = c2.diffed;
  var l2 = c2.__c;
  var m2 = c2.unmount;
  var p2 = c2.__;
  function s2(n2, t3) {
    c2.__h && c2.__h(r2, n2, o2 || t3), o2 = 0;
    var u4 = r2.__H || (r2.__H = { __: [], __h: [] });
    return n2 >= u4.__.length && u4.__.push({}), u4.__[n2];
  }
  function d2(n2) {
    return o2 = 1, y2(D2, n2);
  }
  function y2(n2, u4, i3) {
    var o3 = s2(t2++, 2);
    if (o3.t = n2, !o3.__c && (o3.__ = [i3 ? i3(u4) : D2(void 0, u4), function(n3) {
      var t3 = o3.__N ? o3.__N[0] : o3.__[0], r3 = o3.t(t3, n3);
      t3 !== r3 && (o3.__N = [r3, o3.__[1]], o3.__c.setState({}));
    }], o3.__c = r2, !r2.__f)) {
      var f4 = function(n3, t3, r3) {
        if (!o3.__c.__H) return true;
        var u5 = false, i4 = o3.__c.props !== n3;
        if (o3.__c.__H.__.some(function(n4) {
          if (n4.__N) {
            u5 = true;
            var t4 = n4.__[0];
            n4.__ = n4.__N, n4.__N = void 0, t4 !== n4.__[0] && (i4 = true);
          }
        }), c3) {
          var f5 = c3.call(this, n3, t3, r3);
          return u5 ? f5 || i4 : f5;
        }
        return !u5 || i4;
      };
      r2.__f = true;
      var c3 = r2.shouldComponentUpdate, e3 = r2.componentWillUpdate;
      r2.componentWillUpdate = function(n3, t3, r3) {
        if (this.__e) {
          var u5 = c3;
          c3 = void 0, f4(n3, t3, r3), c3 = u5;
        }
        e3 && e3.call(this, n3, t3, r3);
      }, r2.shouldComponentUpdate = f4;
    }
    return o3.__N || o3.__;
  }
  function h2(n2, u4) {
    var i3 = s2(t2++, 3);
    !c2.__s && C2(i3.__H, u4) && (i3.__ = n2, i3.u = u4, r2.__H.__h.push(i3));
  }
  function _2(n2, u4) {
    var i3 = s2(t2++, 4);
    !c2.__s && C2(i3.__H, u4) && (i3.__ = n2, i3.u = u4, r2.__h.push(i3));
  }
  function A2(n2) {
    return o2 = 5, T2(function() {
      return { current: n2 };
    }, []);
  }
  function T2(n2, r3) {
    var u4 = s2(t2++, 7);
    return C2(u4.__H, r3) && (u4.__ = n2(), u4.__H = r3, u4.__h = n2), u4.__;
  }
  function j2() {
    for (var n2; n2 = f2.shift(); ) {
      var t3 = n2.__H;
      if (n2.__P && t3) try {
        t3.__h.some(z2), t3.__h.some(B2), t3.__h = [];
      } catch (r3) {
        t3.__h = [], c2.__e(r3, n2.__v);
      }
    }
  }
  c2.__b = function(n2) {
    r2 = null, e2 && e2(n2);
  }, c2.__ = function(n2, t3) {
    n2 && t3.__k && t3.__k.__m && (n2.__m = t3.__k.__m), p2 && p2(n2, t3);
  }, c2.__r = function(n2) {
    a2 && a2(n2), t2 = 0;
    var i3 = (r2 = n2.__c).__H;
    i3 && (u2 === r2 ? (i3.__h = [], r2.__h = [], i3.__.some(function(n3) {
      n3.__N && (n3.__ = n3.__N), n3.u = n3.__N = void 0;
    })) : (i3.__h.some(z2), i3.__h.some(B2), i3.__h = [], t2 = 0)), u2 = r2;
  }, c2.diffed = function(n2) {
    v2 && v2(n2);
    var t3 = n2.__c;
    t3 && t3.__H && (t3.__H.__h.length && (1 !== f2.push(t3) && i2 === c2.requestAnimationFrame || ((i2 = c2.requestAnimationFrame) || w2)(j2)), t3.__H.__.some(function(n3) {
      n3.u && (n3.__H = n3.u, n3.u = void 0);
    })), u2 = r2 = null;
  }, c2.__c = function(n2, t3) {
    t3.some(function(n3) {
      try {
        n3.__h.some(z2), n3.__h = n3.__h.filter(function(n4) {
          return !n4.__ || B2(n4);
        });
      } catch (r3) {
        t3.some(function(n4) {
          n4.__h && (n4.__h = []);
        }), t3 = [], c2.__e(r3, n3.__v);
      }
    }), l2 && l2(n2, t3);
  }, c2.unmount = function(n2) {
    m2 && m2(n2);
    var t3, r3 = n2.__c;
    r3 && r3.__H && (r3.__H.__.some(function(n3) {
      try {
        z2(n3);
      } catch (n4) {
        t3 = n4;
      }
    }), r3.__H = void 0, t3 && c2.__e(t3, r3.__v));
  };
  var k2 = "function" == typeof requestAnimationFrame;
  function w2(n2) {
    var t3, r3 = function() {
      clearTimeout(u4), k2 && cancelAnimationFrame(t3), setTimeout(n2);
    }, u4 = setTimeout(r3, 35);
    k2 && (t3 = requestAnimationFrame(r3));
  }
  function z2(n2) {
    var t3 = r2, u4 = n2.__c;
    "function" == typeof u4 && (n2.__c = void 0, u4()), r2 = t3;
  }
  function B2(n2) {
    var t3 = r2;
    n2.__c = n2.__(), r2 = t3;
  }
  function C2(n2, t3) {
    return !n2 || n2.length !== t3.length || t3.some(function(t4, r3) {
      return t4 !== n2[r3];
    });
  }
  function D2(n2, t3) {
    return "function" == typeof t3 ? t3(n2) : t3;
  }

  // src/workbenchRender.ts
  function workbenchStepReportOmissions(envelope) {
    return Array.isArray(envelope.steps) ? Math.max(0, envelope.steps.length - WORKBENCH_MAX_STEP_REPORTS) : 0;
  }
  var stepReports = (envelope) => Array.isArray(envelope.steps) ? envelope.steps.slice(0, WORKBENCH_MAX_STEP_REPORTS) : [];
  function continuationCursor(raw) {
    const result = object2(raw), content = object2(result.content), evidence = object2(result.evidence);
    return [result.cursor, content.cursor, object2(result.page).cursor, object2(content.page).cursor, object2(evidence.page).cursor].find((value) => typeof value === "string" && value.length > 0);
  }
  function continuationRequiredRecordBytes(raw) {
    const bytes = resultMetadata(raw).required_record_bytes;
    return Number.isSafeInteger(bytes) && Number(bytes) > 0 ? Number(bytes) : void 0;
  }
  function guidanceMetadata(raw) {
    const value = object2(raw);
    if (value.version !== 1 || !Array.isArray(value.observations)) return void 0;
    let truncated = value.truncated === true || value.observations.length > 8;
    const observations = value.observations.slice(0, 8).flatMap((raw2) => {
      const row = object2(raw2), keys = ["reason", "subject", "basis", "message", "next_read"];
      if (keys.some((key) => typeof row[key] !== "string" || String(row[key]).length > 512)) {
        truncated = true;
        return [];
      }
      return [Object.fromEntries(keys.map((key) => [key, row[key]]))];
    });
    return { version: 1, observations, truncated };
  }
  function canFollowResult(envelope, report) {
    const result = object2(report.result), content = object2(result.content), evidence = object2(result.evidence);
    const publication = result.publication ?? content.publication ?? evidence.publication;
    const token = typeof publication === "string" ? publication : object2(publication).token;
    if (report.combined_origin !== void 0) {
      const origin = object2(report.combined_origin);
      return envelope.executed === true && ["complete", "partial"].includes(String(envelope.status)) && report.status === "complete" && typeof token === "string" && token === object2(origin.publication).token && typeof origin.index_id === "string" && /^idx_[0-9a-f]{32}$/.test(origin.index_id) && Array.isArray(origin.project_ids) && origin.project_ids.length > 0 && origin.project_ids.length <= 32 && origin.project_ids.every((id) => typeof id === "string" && /^prj_[0-9a-f]{32}$/.test(id));
    }
    return envelope.executed === true && ["complete", "partial"].includes(String(envelope.status)) && report.status === "complete" && typeof envelope.publication === "string" && envelope.publication.length > 0 && token === envelope.publication;
  }
  function discoveryReports(envelope) {
    return stepReports(envelope).flatMap((raw, i3) => {
      const report = object2(raw), combined = readResultBody(report.result);
      if (combined.schema !== "airk.combined-read/1") return [{ ...report, id: String(report.id ?? "step-".concat(i3)) }];
      const selected = object2(combined.selection), selections = Array.isArray(selected.members) ? selected.members.slice(0, 32).map(object2) : [];
      const publications = Array.isArray(object2(combined.publication_vector).members) ? object2(combined.publication_vector).members.map(object2) : [];
      return (Array.isArray(combined.members) ? combined.members.slice(0, 32) : []).map((raw2, j3) => {
        const member = object2(raw2), body = readResultBody(member.result), observation = object2(body.scope_observation ?? object2(member.result).scope_observation), grant = object2(observation.grant);
        let projects = [];
        if (Array.isArray(selected.index_ids)) {
          const matching = Array.isArray(observation.members) ? observation.members.slice(0, 32).map(object2).filter((row) => row.index_id === member.index_id) : [];
          if (selected.index_ids.includes(member.index_id) && observation.schema === 1 && observation.qualifier === "selected_subset" && grant.id === selected.grant_id && grant.revision === selected.grant_revision && matching.length === 1 && Array.isArray(matching[0].memberships) && matching[0].memberships.length <= 32) {
            const ids = matching[0].memberships.map((raw3) => object2(raw3).project_id);
            if (ids.length > 0 && new Set(ids).size === ids.length && ids.every((id) => typeof id === "string" && /^prj_[0-9a-f]{32}$/.test(id) && Array.isArray(selected.project_ids) && selected.project_ids.includes(id))) projects = ids.sort();
          }
        } else {
          const selection = selections.find((row) => row.index_id === member.index_id), frozen = Array.isArray(selection?.memberships) ? selection.memberships.slice(0, 32) : [];
          projects = frozen.map((raw3) => object2(object2(raw3).membership).project_id).filter((value) => typeof value === "string");
        }
        return {
          ...report,
          id: "member-".concat(i3, "-").concat(j3),
          result: body,
          status: report.status === "complete" && member.outcome === "complete" ? "complete" : "failed",
          combined_origin: { index_id: member.index_id, project_ids: projects, publication: publications.find((row) => row.index_id === member.index_id)?.publication }
        };
      });
    });
  }
  function resultMetadata(raw) {
    const result = readResultBody(raw), content = object2(result.content), evidence = object2(result.evidence);
    const page = Object.keys(object2(result.page)).length ? object2(result.page) : Object.keys(object2(content.page)).length ? object2(content.page) : object2(evidence.page);
    const source = { ...result, ...content };
    return Object.fromEntries(Object.entries({
      cursor: continuationCursor(raw),
      truncated: source.truncated ?? evidence.truncated,
      coverage: page.coverage ?? source.coverage ?? evidence.coverage,
      resolution: source.resolution ?? evidence.resolution,
      match_count: source.match_count ?? page.match_count,
      files: page.files ?? source.files,
      files_truncated: page.files_truncated ?? source.files_truncated,
      more: page.more,
      incomplete_reason: page.incomplete_reason,
      required_record_bytes: page.required_record_bytes ?? source.required_record_bytes,
      count_unit: page.count_unit,
      definitions: source.definitions,
      suggested_targets: source.suggested_targets,
      reply_detail: ["lean", "rich"].includes(String(source.reply_detail)) ? source.reply_detail : void 0,
      guidance: guidanceMetadata(source.guidance ?? evidence.guidance),
      read_guidance: Object.keys(object2(source.read_guidance)).length ? source.read_guidance : void 0,
      profile_families: Object.keys(object2(source.profile_families)).length ? source.profile_families : void 0,
      recommendations: source.schema === "airk.recommendations/1" ? source : void 0,
      source_verification: nativeVerification(source.source_verification),
      execution: source.execution ?? evidence.execution,
      semantic_readiness: source.semantic_readiness ?? evidence.semantic_readiness,
      legacy_adoption: source.legacy_adoption ?? evidence.legacy_adoption,
      discovery_readiness: source.discovery_readiness ?? evidence.discovery_readiness,
      error: source.error ?? evidence.error,
      requested_mode: source.requested_mode ?? evidence.requested_mode,
      publication: source.publication ?? evidence.publication,
      scope_observation: source.scope_observation ?? evidence.scope_observation ?? object2(raw).scope_observation
    }).filter(([, value]) => value !== void 0));
  }
  function searchExecutionLabel(raw) {
    const value = object2(raw), requested = value.requested_mode, served = value.served_mode;
    if (value.version !== 1 || typeof requested !== "string" || !["default", "lexical", "vector", "hybrid"].includes(requested) || typeof served !== "string" || !["lexical", "vector", "hybrid"].includes(served) || typeof value.incomplete !== "boolean") return void 0;
    return "Search requested ".concat(requested, "; served ").concat(served, " evidence.").concat(value.incomplete ? " The semantic component is incomplete. Saved-source verification applies to returned file references and does not establish semantic completeness." : "");
  }
  function discoveryDetails(envelope) {
    return discoveryReports(envelope).flatMap((raw, i3) => {
      const report = object2(raw), metadata = resultMetadata(report.result);
      if (!Object.keys(metadata).length) return [];
      const files = Array.isArray(metadata.files) ? metadata.files : [], suggestions = Array.isArray(metadata.suggested_targets) ? metadata.suggested_targets : [], definitions = Array.isArray(metadata.definitions) ? metadata.definitions : [];
      const count = object2(metadata.match_count), coverage = object2(metadata.coverage);
      const origin = object2(report.combined_origin);
      return [{
        id: String(report.id ?? "step-".concat(i3)),
        tool: String(report.tool ?? ""),
        canFollow: canFollowResult(envelope, report),
        ...Object.keys(count).length ? { count } : {},
        ...typeof origin.index_id === "string" ? { indexId: origin.index_id } : {},
        ...Array.isArray(origin.project_ids) && origin.project_ids.length ? { projectIds: origin.project_ids.map(String) } : {},
        ...metadata.reply_detail === "lean" || metadata.reply_detail === "rich" ? { replyDetail: metadata.reply_detail } : {},
        ...metadata.guidance ? { guidance: object2(metadata.guidance) } : {},
        ...metadata.read_guidance ? { readGuidance: object2(metadata.read_guidance) } : {},
        ...metadata.profile_families ? { profileFamilies: object2(metadata.profile_families) } : {},
        ...metadata.recommendations ? { recommendations: object2(metadata.recommendations) } : {},
        ...metadata.source_verification ? { sourceVerification: object2(metadata.source_verification) } : {},
        ...Object.keys(object2(metadata.execution)).length ? { execution: object2(metadata.execution) } : {},
        ...Object.keys(object2(metadata.semantic_readiness)).length ? { semanticReadiness: object2(metadata.semantic_readiness) } : {},
        ...Object.keys(object2(metadata.legacy_adoption)).length ? { legacyAdoption: object2(metadata.legacy_adoption) } : {},
        ...Object.keys(object2(metadata.discovery_readiness)).length ? { discoveryReadiness: object2(metadata.discovery_readiness) } : {},
        ...Object.keys(object2(metadata.error)).length ? { readError: object2(metadata.error) } : {},
        ...Object.keys(object2(metadata.scope_observation)).length ? { scopeObservation: object2(metadata.scope_observation) } : {},
        ...typeof metadata.requested_mode === "string" ? { requestedMode: metadata.requested_mode } : {},
        ...metadata.publication !== void 0 ? { publication: metadata.publication } : {},
        ...Object.keys(coverage).length ? { coverage } : {},
        ...typeof metadata.count_unit === "string" ? { countUnit: metadata.count_unit } : {},
        files: files.slice(0, 100).flatMap((value) => {
          const file = object2(value);
          return typeof file.path === "string" && Number.isSafeInteger(file.count) && Number(file.count) >= 0 ? [{ path: file.path, count: Number(file.count) }] : [];
        }),
        filesTruncated: metadata.files_truncated === true,
        more: metadata.more === true || continuationCursor(report.result) !== void 0,
        ...typeof metadata.incomplete_reason === "string" ? { incompleteReason: metadata.incomplete_reason } : {},
        suggestions: suggestions.slice(0, 100).flatMap((value) => {
          const target = object2(value), citation = object2(target.citation);
          return typeof target.target === "string" ? [{ target: target.target, ...typeof target.symbol_id === "string" ? { symbolId: target.symbol_id } : {}, ...typeof citation.path === "string" ? { path: citation.path } : {}, ...typeof target.coverage === "string" ? { coverage: target.coverage } : {} }] : [];
        }),
        definitions: definitions.slice(0, 100).map(object2),
        omittedFiles: Math.max(0, files.length - 100),
        omittedSuggestions: Math.max(0, suggestions.length - 100),
        omittedDefinitions: Math.max(0, definitions.length - 100),
        ...continuationRequiredRecordBytes(report.result) !== void 0 ? { requiredRecordBytes: continuationRequiredRecordBytes(report.result) } : {}
      }];
    });
  }
  function evidenceRows(envelope) {
    if (isAnswerEnvelope(envelope)) return answerEvidenceRows(envelope);
    if (Array.isArray(envelope.evidence)) return envelope.evidence.map(object2);
    const pointer = (root, path) => {
      if (typeof path !== "string" || path.length > 4096 || !path.startsWith("/")) return;
      let value = root;
      for (const escaped of path.slice(1).split("/")) {
        if (/~[^01]|~$/.test(escaped)) return;
        const key = escaped.replaceAll("~1", "/").replaceAll("~0", "~"), row = object2(value);
        if (Array.isArray(value)) {
          if (!/^(0|[1-9][0-9]*)$/.test(key) || Number(key) >= value.length) return;
          value = value[Number(key)];
        } else {
          if (!Object.hasOwn(row, key)) return;
          value = row[key];
        }
      }
      return value;
    };
    const combined = (value, step) => {
      if (value.schema !== "airk.combined-read/1" || !Array.isArray(value.ranking) || value.ranking.length > 100) return [];
      return value.ranking.flatMap((raw) => {
        const rank = object2(raw), selected = pointer(value, rank.result_pointer), row = typeof selected === "string" ? { path: selected } : object2(selected), origin = object2(rank.origin);
        const memberPath = typeof rank.result_pointer === "string" ? /^\/members\/(0|[1-9][0-9]*)\/result(?:\/structuredContent)?(?:\/|$)/.exec(rank.result_pointer) : void 0;
        const member = memberPath && Array.isArray(value.members) ? object2(value.members[Number(memberPath[1])]) : void 0;
        if (!member || member.index_id !== rank.index_id || member.outcome !== "complete") return [];
        if (typeof origin.index_id !== "string" || !/^idx_[0-9a-f]{32}$/.test(origin.index_id) || origin.index_id !== rank.index_id || !Array.isArray(origin.project_ids) || origin.project_ids.length > 32 || origin.project_ids.some((id) => typeof id !== "string" || !/^prj_[0-9a-f]{32}$/.test(id))) return [];
        return [{ ...row, step, origin, publication: object2(origin.publication).token, result_pointer: rank.result_pointer }];
      });
    };
    if (envelope.schema === "airk.combined-read/1") return combined(envelope, "");
    return stepReports(envelope).flatMap((raw) => {
      const step = object2(raw);
      return combined(object2(step.result), String(step.id ?? ""));
    });
  }
  function combinedReadDetails(envelope) {
    const verification = readResultBody(envelope.member_verification), checks = ["airk.combined-verification/1", "airk.combined-read/1"].includes(String(verification.schema)) ? [{ step: "Returned-source verification", result: verification }] : [];
    if (envelope.schema === "airk.combined-read/1") return [{ step: "", result: envelope }, ...checks];
    return [...stepReports(envelope).flatMap((raw) => {
      const step = object2(raw), result = readResultBody(step.result);
      return result.schema === "airk.combined-read/1" ? [{ step: String(step.id ?? ""), result }] : [];
    }), ...checks];
  }
  function evidenceCards(envelope) {
    const rows = evidenceRows(envelope);
    return rows.flatMap((raw, i3) => {
      const row = object2(raw), citation = object2(row.citation), path = citation.path, hash = citation.content_hash;
      if (typeof path !== "string" || typeof hash !== "string" || !/^[0-9a-f]{64}$/.test(hash) || !Number.isSafeInteger(citation.start_line) || !Number.isSafeInteger(citation.end_line) || Number(citation.start_line) < 1 || Number(citation.end_line) < Number(citation.start_line)) {
        return [];
      }
      const origin = object2(row.origin);
      return [{
        id: "record-".concat(i3),
        path,
        hash,
        start: Number(citation.start_line),
        end: Number(citation.end_line),
        text: typeof row.text === "string" ? row.text : "",
        location: citation.location,
        step: String(row.step ?? ""),
        publication: typeof row.publication === "string" ? row.publication : void 0,
        ...typeof origin.index_id === "string" ? { indexId: origin.index_id } : {},
        ...Array.isArray(origin.project_ids) ? { projectIds: origin.project_ids.map(String) } : {},
        ...typeof origin.source_root_id === "string" ? { sourceRootId: origin.source_root_id } : {},
        ...typeof origin.source_relative_path === "string" ? { sourceRelativePath: origin.source_relative_path } : {},
        ...typeof row.result_pointer === "string" ? { resultPointer: row.result_pointer } : {},
        ...Number.isSafeInteger(row.answer_member) ? { answerMember: Number(row.answer_member) } : {}
      }];
    });
  }
  function canonical(value) {
    const sorted = (v3) => Array.isArray(v3) ? v3.map(sorted) : v3 && typeof v3 === "object" ? Object.fromEntries(Object.entries(v3).sort(([a3], [b2]) => a3 < b2 ? -1 : a3 > b2 ? 1 : 0).map(([k3, item]) => [k3, sorted(item)])) : v3;
    return JSON.stringify(sorted(value), null, 2);
  }
  function selectedSourceCoverage(turn, selected) {
    const steps = new Set(evidenceCards(turn.envelope ?? {}).filter((card) => selected.has(card.id)).map((card) => card.step));
    const combined = combinedReadDetails(turn.envelope ?? {}).filter((detail) => steps.has(detail.step)).map(({ step, result }) => ({
      step,
      scope: "original_combined_request",
      ...result.shared_values !== void 0 ? { shared_values: result.shared_values } : {},
      selection: result.selection,
      publication_vector: result.publication_vector,
      rank_policy: result.rank_policy,
      partial: result.partial,
      truncated: result.truncated,
      ...result.error !== void 0 ? { error: result.error } : {},
      members: (Array.isArray(result.members) ? result.members : []).map((raw) => {
        const member = object2(raw);
        return {
          index_id: member.index_id,
          ...member.index_name !== void 0 ? { index_name: member.index_name } : {},
          allocation: member.allocation,
          outcome: member.outcome,
          ...member.cutoff !== void 0 ? { cutoff: member.cutoff } : {},
          ...member.elapsed_ms !== void 0 ? { elapsed_ms: member.elapsed_ms } : {},
          result_metadata: resultMetadata(member.result),
          result_payload_omitted: true
        };
      })
    }));
    return [...combined, ...(Array.isArray(turn.envelope?.steps) ? turn.envelope.steps : []).flatMap((raw) => {
      const report = object2(raw);
      if (typeof report.id !== "string" || !steps.has(report.id)) return [];
      const metadata = resultMetadata(report.result), qualification = {};
      if (typeof metadata.coverage === "string") qualification.coverage = metadata.coverage;
      else {
        const coverage = object2(metadata.coverage), safe = {};
        for (const key of ["basis", "line_completeness"]) if (typeof coverage[key] === "string") safe[key] = coverage[key];
        for (const key of ["candidate_scan_complete", "query_complete", "token_boundary_complete"]) if (typeof coverage[key] === "boolean") safe[key] = coverage[key];
        if (coverage.query_total_matches === null) safe.query_total_matches = null;
        if (Object.keys(safe).length) qualification.coverage = safe;
      }
      if (typeof metadata.truncated === "boolean") qualification.truncated = metadata.truncated;
      if (typeof metadata.count_unit === "string") qualification.count_unit = metadata.count_unit;
      if (typeof metadata.resolution === "string") qualification.resolution = metadata.resolution;
      if (metadata.reply_detail === "lean" || metadata.reply_detail === "rich") qualification.reply_detail = metadata.reply_detail;
      if (metadata.guidance) qualification.guidance = metadata.guidance;
      for (const key of ["execution", "semantic_readiness", "legacy_adoption", "discovery_readiness", "error", "requested_mode", "publication", "read_guidance", "profile_families", "scope_observation"]) {
        if (metadata[key] !== void 0) qualification[key] = metadata[key];
      }
      return Object.keys(qualification).length ? [{ step: report.id, tool: typeof report.tool === "string" ? report.tool : "", scope: "original_source_request", ...qualification }] : [];
    })];
  }
  function selectedEnvelope(turn, selected, projection = turn.envelope) {
    const raw = turn.envelope;
    turn = { ...turn, envelope: projection };
    const sourceCoverage = selectedSourceCoverage(turn, selected);
    const originalEncoding = combinedReadDetails(raw).filter(({ result }) => result.shared_values !== void 0).map(({ step, result }) => ({ step, shared_values: result.shared_values, publication_vector: result.publication_vector, members: (Array.isArray(result.members) ? result.members : []).map((item) => {
      const member = object2(item), body = readResultBody(member.result);
      return { index_id: member.index_id, ...body.$airk_shared_native_header_fields !== void 0 ? { $airk_shared_native_header_fields: body.$airk_shared_native_header_fields } : {}, ...body.$airk_shared_capability_fields !== void 0 ? { $airk_shared_capability_fields: body.$airk_shared_capability_fields } : {} };
    }) }));
    const selectedCards = evidenceCards(turn.envelope).filter((card) => selected.has(card.id));
    const checks = selectedVerification(turn.envelope, selectedCards);
    const { source_verification: _originalReceipt, member_verification: _memberReceipt, delivery_observation: _delivery, ...original } = raw;
    const members = Array.isArray(original.members) ? original.members.map((raw2) => {
      const { result: _result, ...member } = object2(raw2);
      return { ...member, result_payload_omitted: true };
    }) : void 0;
    return {
      ...original,
      ...originalEncoding.length ? { original_native_metadata: originalEncoding } : {},
      steps: [],
      ...members ? { members, ranking: [], original_member_payloads_omitted: true } : {},
      evidence: selectedCards.map((card) => evidenceRows(turn.envelope)[Number(card.id.slice("record-".length))]),
      ...original.selection !== null && typeof original.selection === "object" && !Array.isArray(original.selection) ? { selected_subset: true } : { selection: true },
      ...checks.requested ? { source_verification_observations: checks.observations, selected_verification_accounting: checks.accounting } : {},
      ...sourceCoverage.length ? { source_coverage: sourceCoverage } : {}
    };
  }
  function openSelectionArgv(selection) {
    return [...selection.indexes.flatMap((name) => ["--index", name]), ...selection.sets.flatMap((name) => ["--set", name])];
  }
  function markdown(turn, selected, projection = turn.envelope) {
    const escape = (text) => text.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replace(/[\\`*_[\]]/g, "\\$&");
    const fenced = (text) => {
      const fence = "`".repeat(Math.max(3, ...[...text.matchAll(/`+/g)].map((m3) => m3[0].length + 1)));
      return "".concat(fence, "\n").concat(text, "\n").concat(fence);
    };
    const e3 = projection ?? {}, cards = evidenceCards(e3).filter((card) => !selected || selected.has(card.id));
    const content = cards.map((card) => {
      return "### ".concat(escape(card.path), ":").concat(card.start, "-").concat(card.end, "\n\nLocation: ").concat(escape(JSON.stringify(card.location ?? null)), "\n\nSource hash: ").concat(card.hash, "\n\n").concat(fenced(card.text));
    });
    const sourceCoverage = selected ? selectedSourceCoverage({ ...turn, envelope: e3 }, selected) : [];
    const selectedRows = selected ? cards.map((card) => evidenceRows(e3)[Number(card.id.slice("record-".length))]) : [];
    const selectedChecks = selected ? selectedVerification(e3, cards) : void 0;
    const metadata = selected ? sourceCoverage.length ? [...sourceCoverage.flatMap((value) => {
      const label = searchExecutionLabel(value.execution);
      return label ? [escape(label)] : [];
    }), "### Original source request coverage for selected evidence\n\n".concat(fenced(canonical(sourceCoverage)))] : [] : (Array.isArray(e3.steps) ? e3.steps : []).flatMap((raw) => {
      const report = object2(raw), details = resultMetadata(report.result);
      const label = searchExecutionLabel(details.execution);
      return Object.keys(details).length ? [...label ? [escape(label)] : [], "### ".concat(escape(String(report.tool ?? report.id ?? "Step")), " discovery details\n\n").concat(fenced(canonical(details)))] : [];
    });
    return [
      "## ".concat(escape(turn.input.replaceAll("\n", " "))),
      "Status: ".concat(escape(String(e3.status ?? e3.stop ?? object2(e3.context).termination ?? turn.error ?? "trimmed"))),
      "Publication: ".concat(escape(String(e3.publication ?? object2(object2(object2(e3.assessment).scope).publication).token ?? (isAnswerEnvelope(e3) && e3.schema === "airk.combined-read/1" ? "Independent member publication vector retained in complete response" : "not available")))),
      "Scope: ".concat(escape(String(object2(e3.plan).path_prefix ?? ""))),
      ...turn.selection ? ["Explicit selection: ".concat(escape(openSelectionArgv(turn.selection).join(" ")))] : [],
      ...turn.trimmed ? ["Saved result was trimmed; re-run to collect current evidence."] : [],
      ...content,
      ...e3.scope_observation !== void 0 ? ["### Recorded scope observation\n\n".concat(fenced(canonical(e3.scope_observation)))] : [],
      ...selected ? ["### ".concat(selectedChecks?.requested ? "Selected evidence and dated source observations" : "Selected evidence", "\n\n").concat(fenced(canonical({ evidence: selectedRows, ...selectedChecks?.requested ? { source_verification_observations: selectedChecks.observations, selected_verification_accounting: selectedChecks.accounting } : {} })))] : [],
      ...metadata,
      ...turn.request ? ["### Complete original request\n\n".concat(fenced(canonical(turn.request)))] : [],
      ...turn.envelope ? ["### ".concat(selected ? "Complete selected response" : "Complete response", "\n\n").concat(fenced(canonical(selected ? selectedEnvelope(turn, selected, e3) : turn.envelope)))] : [],
      ...turn.nativeFailure ? ["### Complete original native refusal\n\n".concat(fenced(canonical(turn.nativeFailure)))] : [],
      "Warnings: ".concat(escape(JSON.stringify(e3.warnings ?? []))),
      "Completeness: ".concat(escape(JSON.stringify(e3.completeness ?? []))),
      ...e3.source_verification ? [verificationLabel(e3.source_verification), "Saved-byte observations retain their original dates. Extraction, coverage, relevance and unsaved buffers remain unverified; recheck before later edits."] : ["Untrusted indexed evidence. Verify relevant live files before edits."]
    ].join("\n\n");
  }

  // node_modules/preact/jsx-runtime/dist/jsxRuntime.module.js
  var f3 = 0;
  function u3(e3, t3, n2, o3, i3, u4) {
    t3 || (t3 = {});
    var a3, c3, p3 = t3;
    if ("ref" in p3) for (c3 in p3 = {}, t3) "ref" == c3 ? a3 = t3[c3] : p3[c3] = t3[c3];
    var l3 = { type: e3, props: p3, key: n2, ref: a3, __k: null, __: null, __b: 0, __e: null, __c: null, constructor: void 0, __v: --f3, __i: -1, __u: 0, __source: i3, __self: u4 };
    if ("function" == typeof e3 && (a3 = e3.defaultProps)) for (c3 in a3) void 0 === p3[c3] && (p3[c3] = a3[c3]);
    return l.vnode && l.vnode(l3), l3;
  }

  // src/workbenchWebview/main.tsx
  var vscode = acquireVsCodeApi();
  var send = (message) => vscode.postMessage(message);
  var COMMANDS = ["/search", "/symbols", "/code-facts", "/refs", "/read", "/outline", "/files", "/info", "/imports", "/facts", "/changes --since", "/status", "/diagnostics", "/capabilities", "/context", "/relationships", "/help"];
  var RECIPES = [["definition", "Explore a definition"], ["documents", "Collect document evidence"], ["impact", "Find possible impact"], ["tests", "Find test mentions"], ["component", "Explore a component"], ["readiness", "Check retrieval readiness"]];
  function AnswerView(props) {
    const show = (value) => typeof value === "string" ? value : k(value.tag, {
      ...value.id ? { id: value.id, tabIndex: -1 } : {},
      ...value.citationTarget ? { type: "button", onClick: () => {
        const record = document.getElementById(value.citationTarget);
        record?.closest("details")?.setAttribute("open", "");
        record?.focus();
        record?.scrollIntoView({ block: "start" });
      } } : {}
    }, ...value.children.map(show));
    return /* @__PURE__ */ u3(S, { children: [
      show(answerPresentation(props.answer, props.member === void 0 ? props.identity : "".concat(props.identity, "-member-").concat(props.member))),
      /* @__PURE__ */ u3("section", { "aria-label": "Open cited answer sources", children: [
        /* @__PURE__ */ u3("h4", { children: "Open cited sources" }),
        /* @__PURE__ */ u3("p", { class: "note", children: "Source locations are resolved by the native host from the retained record and checked again before opening." }),
        props.cards.length ? props.cards.map((card) => /* @__PURE__ */ u3("button", { type: "button", disabled: props.disabled, onClick: () => send({ type: "openAnswerCitation", turn: props.identity, record: card.step, hit: Number(card.resultPointer?.slice("/hits/".length)), ...props.member === void 0 ? {} : { member: props.member } }), children: [
          "Open ",
          card.path,
          ":",
          card.start,
          " · record ",
          card.step.slice(0, 12)
        ] })) : /* @__PURE__ */ u3("p", { children: "No unambiguous native source locator was returned. Complete original evidence remains available above." })
      ] })
    ] });
  }
  function ToolForm({ state, issue, currentDraft, onSelect, onField, onReset, onIssue }) {
    const draft = normalizeWorkbenchFormDraft(state.session?.formDraft) ?? defaultWorkbenchFormDraft(), tool = draft.activeTool, values = draft.values[tool] ?? {};
    const schema = object2(state.operations?.[tool]), fields = workbenchToolFields(schema, state.combinedControls);
    const submit = (execute) => {
      try {
        const latest = currentDraft();
        if (!latest || latest.activeTool !== tool) throw new Error("Choose the tool form in the current session.");
        const args = toolFormArguments(schema, latest.values[tool] ?? {}, state.combinedControls), scope = latest.scope, queryScope = {};
        for (const key of ["project_ids", "index_ids", "member_availability"]) {
          if (args[key] !== void 0) {
            queryScope[key] = args[key];
            delete args[key];
          }
        }
        send({ type: "run", input: "".concat(tool, " form"), prepare: !execute, pathPrefix: scope, plan: { steps: [{ id: "form", tool, args }], path_prefix: scope, ...Object.keys(queryScope).length ? { query_scope: queryScope } : {} } });
        onIssue("");
      } catch (error) {
        onIssue(error instanceof Error ? error.message : "Review the form values.");
      }
    };
    return /* @__PURE__ */ u3("details", { class: "tool-form", open: true, children: [
      /* @__PURE__ */ u3("summary", { children: "Tool form" }),
      /* @__PURE__ */ u3("div", { class: "tool-form-toolbar", children: [
        /* @__PURE__ */ u3("label", { children: [
          "Operation",
          /* @__PURE__ */ u3("select", { "aria-label": "Operation", value: tool, onChange: (e3) => onSelect(e3.currentTarget.value), children: Object.keys(state.operations ?? {}).map((name) => /* @__PURE__ */ u3("option", { value: name, children: name })) })
        ] }),
        /* @__PURE__ */ u3("button", { class: "icon-button", type: "button", title: "Reset this tool form", "aria-label": "Reset this tool form", disabled: !state.session, onClick: () => onReset(tool), children: /* @__PURE__ */ u3(Icon, { name: "reset" }) })
      ] }),
      /* @__PURE__ */ u3("form", { onSubmit: (e3) => {
        e3.preventDefault();
        submit(false);
      }, children: [
        fields.map((field) => /* @__PURE__ */ u3("label", { children: [
          field.label,
          field.required ? " *" : "",
          field.type === "boolean" || field.values ? /* @__PURE__ */ u3("select", { "aria-label": field.label, required: field.required, value: values[field.name] ?? "", onChange: (e3) => onField(tool, field.name, e3.currentTarget.value), children: [
            /* @__PURE__ */ u3("option", { value: "", children: field.defaultValue !== void 0 ? "Default (".concat(String(field.defaultValue), ")") : "Default" }),
            field.type === "boolean" ? /* @__PURE__ */ u3(S, { children: [
              /* @__PURE__ */ u3("option", { value: "true", children: "Yes" }),
              /* @__PURE__ */ u3("option", { value: "false", children: "No" })
            ] }) : field.values?.map((value) => /* @__PURE__ */ u3("option", { value, children: value }))
          ] }) : /* @__PURE__ */ u3("input", { "aria-label": field.label, required: field.required, type: field.type === "integer" ? "number" : "text", placeholder: field.defaultValue !== void 0 ? "Default: ".concat(String(field.defaultValue)) : void 0, min: field.minimum, max: field.maximum, minLength: field.minLength, maxLength: field.maxLength ?? 4096, value: values[field.name] ?? "", onInput: (e3) => onField(tool, field.name, e3.currentTarget.value) }),
          field.description && /* @__PURE__ */ u3("small", { class: "form-hint", children: field.description })
        ] }, field.name)),
        issue && /* @__PURE__ */ u3("p", { role: "alert", children: issue }),
        /* @__PURE__ */ u3("button", { disabled: state.busy, type: "submit", children: "Prepare form" }),
        /* @__PURE__ */ u3("button", { disabled: state.busy, type: "button", onClick: (e3) => {
          if (e3.currentTarget.form?.reportValidity()) {
            submit(true);
          }
        }, children: "Run form" })
      ] })
    ] });
  }
  function countLabel(count, unit) {
    const scope = typeof count.scope === "string" ? count.scope.replaceAll("_", " ") : "verified scope";
    if (count.scope === "verified_lexical_candidates" && count.complete === true && typeof count.total_matches === "number") return "".concat(count.total_matches, " ").concat(unit, " in verified lexical candidates; whole-query total unknown");
    if (count.complete === true && typeof count.total_matches === "number") return "".concat(count.total_matches, " ").concat(unit, " (").concat(scope, ")");
    return "".concat(typeof count.lower_bound === "number" ? "At least ".concat(count.lower_bound, " verified ").concat(unit) : "Total unknown", " (").concat(scope, "; total unknown)");
  }
  function CombinedMemberDetails({ member, display }) {
    const [opened, setOpened] = d2(false), native = object2(object2(member.result).structuredContent ?? member.result);
    let materialized, issue;
    if (opened) {
      try {
        materialized = materializeCapabilityMember(member.result, display);
      } catch (error) {
        issue = error instanceof Error ? error.message : "Capability display unavailable.";
      }
    }
    const outcome = member.outcome ?? object2(native.source_verification).status ?? (object2(member.result).isError === true ? "failed" : "observed");
    return /* @__PURE__ */ u3("details", { onToggle: (event) => setOpened(event.currentTarget.open), children: [
      /* @__PURE__ */ u3("summary", { children: [
        typeof member.index_name === "string" ? "".concat(member.index_name, " (").concat(String(member.index_id), ")") : String(member.index_id),
        ": ",
        String(outcome),
        typeof member.elapsed_ms === "number" ? " · ".concat(member.elapsed_ms, " ms") : ""
      ] }),
      /* @__PURE__ */ u3("pre", { children: canonical({ index_id: member.index_id, index_name: member.index_name, elapsed_ms: member.elapsed_ms, allocation: member.allocation, cutoff: member.cutoff, reference_bindings: member.reference_bindings }) }),
      native.source_verification && /* @__PURE__ */ u3("p", { children: verificationLabel(native.source_verification) }),
      native.execution && /* @__PURE__ */ u3("p", { children: searchExecutionLabel(native.execution) }),
      issue && /* @__PURE__ */ u3("p", { class: "warning", children: [
        issue,
        " The complete original response remains available below."
      ] }),
      opened && /* @__PURE__ */ u3("pre", { children: canonical(materialized ?? member.result) })
    ] });
  }
  function DiscoveryDetails({ turn, state }) {
    const logical = state.metadataViews?.[turn.id] ?? turn.envelope ?? {};
    const details = T2(() => discoveryDetails(logical), [logical]);
    const combined = T2(() => combinedReadDetails(logical), [logical]);
    return /* @__PURE__ */ u3(S, { children: [
      state.metadataIssues?.[turn.id] && /* @__PURE__ */ u3("p", { class: "warning", children: state.metadataIssues[turn.id] }),
      workbenchStepReportOmissions(turn.envelope ?? {}) > 0 && /* @__PURE__ */ u3("p", { class: "warning", children: [
        workbenchStepReportOmissions(turn.envelope ?? {}),
        " step reports exceed the supported display limit. The complete original response remains in copies and exports."
      ] }),
      combined.map(({ step, result }) => /* @__PURE__ */ u3("section", { class: "discovery-details", children: [
        /* @__PURE__ */ u3("h3", { children: step ? "Combined members for " + step : "Combined members" }),
        result.schema === "airk.combined-verification/1" ? /* @__PURE__ */ u3("p", { class: "note", children: "Dated saved-source checks retain each member's complete receipt and original reference associations. Viewing them performs no new check." }) : /* @__PURE__ */ u3("p", { class: "note", children: "Each selected index runs once. Member publications describe an observation interval; they do not form one atomic snapshot." }),
        /* @__PURE__ */ u3("pre", { children: canonical({ selection: result.selection, publication_vector: result.publication_vector, rank_policy: result.rank_policy, partial: result.partial, truncated: result.truncated, error: result.error, scope_identity: result.scope_identity, binding_basis: result.binding_basis }) }),
        state.capabilityDetails?.[turn.id]?.[step]?.state === "unavailable" && /* @__PURE__ */ u3("p", { class: "warning", children: "Shared capability fields could not be validated. Inspect the complete original response." }),
        (Array.isArray(result.members) ? result.members : []).map((raw, i3) => /* @__PURE__ */ u3(CombinedMemberDetails, { member: object2(raw), display: state.capabilityDetails?.[turn.id]?.[step] }, i3))
      ] }, "combined-" + step)),
      turn.envelope?.scope_observation !== void 0 && /* @__PURE__ */ u3("details", { children: [
        /* @__PURE__ */ u3("summary", { children: "Recorded scope observation" }),
        /* @__PURE__ */ u3("p", { class: "note", children: "Authorization identity for the returned subset; current access and publication freshness remain separate." }),
        /* @__PURE__ */ u3("pre", { children: canonical(turn.envelope.scope_observation) })
      ] }),
      turn.envelope?.source_verification && /* @__PURE__ */ u3("details", { open: true, children: [
        /* @__PURE__ */ u3("summary", { children: verificationLabel(turn.envelope.source_verification) }),
        /* @__PURE__ */ u3("p", { class: "note", children: "Dated saved-byte agreement; viewing this receipt performs no new check. Unsaved editor buffers, extraction accuracy and search completeness are not certified." }),
        /* @__PURE__ */ u3("pre", { children: canonical(turn.envelope.source_verification) })
      ] }),
      details.map((step) => /* @__PURE__ */ u3("section", { class: "discovery-details", "aria-label": "".concat(step.tool, " discovery details"), children: [
        step.indexId && /* @__PURE__ */ u3("p", { class: "note", children: [
          "Index: ",
          /* @__PURE__ */ u3("code", { children: step.indexId }),
          " | Selected Projects: ",
          step.projectIds?.length ? step.projectIds.join(", ") : "not observed"
        ] }),
        step.sourceVerification && /* @__PURE__ */ u3("details", { open: true, children: [
          /* @__PURE__ */ u3("summary", { children: verificationLabel(step.sourceVerification) }),
          /* @__PURE__ */ u3("p", { class: "note", children: "Saved disk bytes only, at the original observation time. Extraction, coverage, relevance and unsaved buffers remain qualified." }),
          /* @__PURE__ */ u3("pre", { children: canonical(step.sourceVerification) })
        ] }),
        step.scopeObservation && /* @__PURE__ */ u3("details", { children: [
          /* @__PURE__ */ u3("summary", { children: "Recorded scope observation" }),
          /* @__PURE__ */ u3("p", { class: "note", children: "Authorization identity for this returned subset. This record does not establish current access or publication freshness." }),
          /* @__PURE__ */ u3("pre", { children: canonical({ scope_observation: step.scopeObservation }) })
        ] }),
        step.execution && /* @__PURE__ */ u3("details", { open: true, children: [
          /* @__PURE__ */ u3("summary", { children: "Search execution" }),
          searchExecutionLabel(step.execution) && /* @__PURE__ */ u3("p", { class: step.execution.incomplete === true ? "warning" : "note", children: searchExecutionLabel(step.execution) }),
          /* @__PURE__ */ u3("pre", { children: canonical(step.execution) })
        ] }),
        step.readError && /* @__PURE__ */ u3("details", { open: true, children: [
          /* @__PURE__ */ u3("summary", { children: "Read refusal" }),
          /* @__PURE__ */ u3("pre", { children: canonical({ error: step.readError, ...step.requestedMode !== void 0 ? { requested_mode: step.requestedMode } : {}, ...step.publication !== void 0 ? { publication: step.publication } : {} }) })
        ] }),
        (step.semanticReadiness || step.legacyAdoption || step.discoveryReadiness) && /* @__PURE__ */ u3("details", { children: [
          /* @__PURE__ */ u3("summary", { children: "Indexed coverage observations" }),
          /* @__PURE__ */ u3("p", { class: "note", children: "These observations describe the retained index and are independent of this search's execution outcome." }),
          /* @__PURE__ */ u3("pre", { children: canonical({ ...step.semanticReadiness ? { semantic_readiness: step.semanticReadiness } : {}, ...step.legacyAdoption ? { legacy_adoption: step.legacyAdoption } : {}, ...step.discoveryReadiness ? { discovery_readiness: step.discoveryReadiness } : {} }) })
        ] }),
        step.replyDetail && /* @__PURE__ */ u3("p", { class: "note", children: [
          "Reply detail: ",
          step.replyDetail === "lean" ? "Lean" : "Rich"
        ] }),
        step.readGuidance && /* @__PURE__ */ u3("details", { children: [
          /* @__PURE__ */ u3("summary", { children: "Guidance vocabulary and limits" }),
          /* @__PURE__ */ u3("pre", { children: canonical(step.readGuidance) })
        ] }),
        step.profileFamilies && /* @__PURE__ */ u3("details", { open: true, children: [
          /* @__PURE__ */ u3("summary", { children: "Effective profile families (metadata only)" }),
          /* @__PURE__ */ u3("p", { class: "note", children: "Support, retention eligibility and reply inclusion are separate observations. Unavailable collection settings and unknown pending state remain unknown. This report does not establish fact counts, completeness or live-source equality." }),
          /* @__PURE__ */ u3("pre", { children: canonical(step.profileFamilies) })
        ] }),
        step.recommendations && /* @__PURE__ */ u3("details", { open: true, children: [
          /* @__PURE__ */ u3("summary", { children: "Recommendations and scope instructions" }),
          /* @__PURE__ */ u3("pre", { children: canonical(step.recommendations) })
        ] }),
        step.guidance && /* @__PURE__ */ u3("details", { open: true, children: [
          /* @__PURE__ */ u3("summary", { children: "Evidence guidance" }),
          /* @__PURE__ */ u3("ul", { children: (Array.isArray(step.guidance.observations) ? step.guidance.observations : []).map((raw, i3) => {
            const row = object2(raw);
            return /* @__PURE__ */ u3("li", { children: [
              /* @__PURE__ */ u3("strong", { children: [
                String(row.reason).replaceAll("_", " "),
                " (",
                String(row.subject).replaceAll("_", " "),
                "): "
              ] }),
              String(row.message),
              " ",
              /* @__PURE__ */ u3("small", { children: [
                "Next read: ",
                String(row.next_read).replaceAll("_", " "),
                ". Basis: ",
                String(row.basis),
                "."
              ] })
            ] }, i3);
          }) }),
          step.guidance.truncated === true && /* @__PURE__ */ u3("p", { class: "note", children: "Additional guidance was omitted. Original status and coverage remain in the full response." }),
          /* @__PURE__ */ u3("p", { class: "note", children: "Suggestions do not run tools or change indexing. Keep the original authorized scope." })
        ] }),
        !step.canFollow && (step.definitions.length > 0 || step.suggestions.length > 0) && /* @__PURE__ */ u3("p", { class: "note", children: "Re-run this investigation to enable follow-up actions from completed, publication-matched evidence." }),
        step.count && /* @__PURE__ */ u3("p", { class: "counts", children: countLabel(step.count, step.tool === "references" ? (step.countUnit ?? "matching_lines").replaceAll("_", " ") : "records") }),
        step.coverage?.query_complete === false && /* @__PURE__ */ u3("p", { class: "warning", children: "Reference coverage is limited to verified lexical candidates. The whole-query total is unknown; identifier tokens split across indexed chunk boundaries can be missed." }),
        step.tool === "references" && step.coverage === void 0 && /* @__PURE__ */ u3("p", { class: "warning", children: "Textual candidate matches do not establish complete reference coverage; the whole-query total is unknown." }),
        step.coverage?.line_completeness === "includes_unverified_fragments" && /* @__PURE__ */ u3("p", { class: "warning", children: "Some extracted-text matches are fragments whose completeness as original source lines is unverified." }),
        step.incompleteReason && /* @__PURE__ */ u3("p", { class: "warning", children: [
          "Discovery is incomplete: ",
          step.incompleteReason.replaceAll("_", " "),
          "."
        ] }),
        step.more && /* @__PURE__ */ u3("p", { class: "note", children: "More scanning remains. Use Fetch next page when a continuation is available." }),
        step.requiredRecordBytes !== void 0 && /* @__PURE__ */ u3("p", { class: "note", children: [
          "The next complete record needs ",
          step.requiredRecordBytes,
          " bytes.",
          !step.more ? " No continuation is available; use a scoped source read or inspect the reported coverage limit." : ""
        ] }),
        step.files.length > 0 && /* @__PURE__ */ u3("details", { children: [
          /* @__PURE__ */ u3("summary", { children: "Files in this page" }),
          /* @__PURE__ */ u3("ul", { children: step.files.map((file) => /* @__PURE__ */ u3("li", { children: [
            file.path,
            ": ",
            file.count,
            " ",
            (step.countUnit ?? "matching_lines").replaceAll("_", " ")
          ] }, file.path)) }),
          (step.filesTruncated || step.omittedFiles > 0) && /* @__PURE__ */ u3("p", { class: "warning", children: "The file summary is incomplete. Inspect the full export and continuation coverage." })
        ] }),
        step.files.length === 0 && step.filesTruncated && /* @__PURE__ */ u3("p", { class: "warning", children: "The file summary is incomplete at this budget. Inspect the reported continuation and coverage." }),
        step.definitions.length > 0 && /* @__PURE__ */ u3("details", { open: true, children: [
          /* @__PURE__ */ u3("summary", { children: "Definition pointers" }),
          /* @__PURE__ */ u3("p", { class: "note", children: "Definitions are candidates from this same indexed publication. Only a bounded set of pointers is shown." }),
          step.definitions.map((hint, i3) => /* @__PURE__ */ u3("div", { children: [
            /* @__PURE__ */ u3("p", { children: [
              String(hint.name ?? "Identifier"),
              ": ",
              countLabel(hint, "definition records")
            ] }),
            (Array.isArray(hint.matches) ? hint.matches : []).map((raw, j3) => {
              const citation = object2(object2(raw).citation);
              return typeof hint.name === "string" && typeof citation.path === "string" && typeof object2(raw).symbol_id === "string" && String(object2(raw).symbol_id).length > 0 ? /* @__PURE__ */ u3("button", { disabled: state.busy || !step.canFollow, onClick: () => send({ type: "followDefinition", turn: turn.id, step: step.id, name: String(hint.name), path: String(citation.path) }), children: [
                "Inspect ",
                hint.name,
                " — ",
                citation.path
              ] }, j3) : null;
            })
          ] }, i3)),
          step.omittedDefinitions > 0 && /* @__PURE__ */ u3("p", { class: "note", children: "Additional pointers remain in the full export." })
        ] }),
        step.suggestions.length > 0 && /* @__PURE__ */ u3("details", { open: true, children: [
          /* @__PURE__ */ u3("summary", { children: "Written method targets" }),
          /* @__PURE__ */ u3("p", { class: "note", children: "Follow a target for textual call candidates. These are not compiler-resolved callers." }),
          step.suggestions.map((target, i3) => /* @__PURE__ */ u3("div", { children: [
            /* @__PURE__ */ u3("button", { disabled: state.busy || !step.canFollow, onClick: () => send({ type: "followTarget", turn: turn.id, step: step.id, target: target.target }), children: [
              "Find callers of ",
              target.target
            ] }),
            target.path && /* @__PURE__ */ u3("small", { children: target.path }),
            target.coverage && /* @__PURE__ */ u3("p", { class: "note", children: target.coverage })
          ] }, i3)),
          step.omittedSuggestions > 0 && /* @__PURE__ */ u3("p", { class: "note", children: "Additional targets remain in the full export." })
        ] })
      ] }, step.id))
    ] });
  }
  function SelectAll({ count, checked, mixed, onChange }) {
    const input = A2(null);
    _2(() => {
      if (input.current) {
        input.current.indeterminate = mixed;
      }
    }, [mixed]);
    return /* @__PURE__ */ u3("label", { class: "select-all", title: "Select all matching results in this response", children: [
      /* @__PURE__ */ u3("input", { ref: input, type: "checkbox", checked, "aria-checked": mixed ? "mixed" : checked, disabled: count === 0, onChange: (e3) => onChange(e3.currentTarget.checked) }),
      "Select all ",
      count
    ] });
  }
  function Turn({ turn, state }) {
    const [format, setFormat] = d2(turn.format), [show, setShow] = d2(20), [filter, setFilter] = d2(""), [selected, setSelected] = d2([]), [edit, setEdit] = d2(false), [draft, setDraft] = d2(""), [issue, setIssue] = d2(""), [group, setGroup] = d2(false);
    const envelope = turn.envelope ?? {}, plan = object2(envelope.plan), cards = state.cards[turn.id] ?? [];
    const rows = T2(() => {
      const matching = cards.filter((c3) => "".concat(c3.path, " ").concat(c3.text).toLowerCase().includes(filter.toLowerCase()));
      if (group) matching.sort((a3, b2) => a3.path.localeCompare(b2.path) || a3.start - b2.start);
      return matching;
    }, [cards, filter, group]);
    const matchingIds = new Set(rows.map((card) => card.id)), selectedMatches = rows.filter((card) => selected.includes(card.id)).length;
    const allSelected = rows.length > 0 && selectedMatches === rows.length, mixedSelection = selectedMatches > 0 && !allSelected;
    const selectMatching = (checked) => setSelected((previous) => checked ? [.../* @__PURE__ */ new Set([...previous, ...matchingIds])] : previous.filter((id) => !matchingIds.has(id)));
    const copyFormat = format === "json" ? "json" : "markdown", copySelection = { type: "copy", turn: turn.id, format: copyFormat, cards: selected };
    const copySelectionAllowed = selected.length > 0 && !!parseWorkbenchMessage(copySelection);
    const selectionTooLarge = selected.length > 0 && !copySelectionAllowed;
    const copySelectionLabel = "Copy ".concat(selected.length, " selected results as ").concat(copyFormat === "json" ? "JSON" : "Markdown");
    const copySelectionTitle = selectionTooLarge ? "Select fewer results or use Save export" : copySelectionLabel;
    const selectionUi = format === "structured" && cards.length > 0;
    const selectionRow = () => /* @__PURE__ */ u3("div", { class: "selection-toolbar", children: [
      /* @__PURE__ */ u3(SelectAll, { count: rows.length, checked: allSelected, mixed: mixedSelection, onChange: selectMatching }),
      /* @__PURE__ */ u3("button", { disabled: state.busy || selected.length === 0, onClick: () => send({ type: "refreshReferences", turn: turn.id, cards: selected, mode: "selected_only" }), children: "Review refresh selected files…" }),
      /* @__PURE__ */ u3("button", { disabled: state.busy || selected.length === 0, onClick: () => send({ type: "refreshReferences", turn: turn.id, cards: selected, mode: "full_reconcile_reviewed" }), children: "Review full reconciliation…" }),
      /* @__PURE__ */ u3("button", { title: copySelectionTitle, "aria-label": copySelectionLabel, disabled: !copySelectionAllowed, onClick: () => send(copySelection), children: [
        "Copy selected (",
        selected.length,
        ")"
      ] })
    ] });
    const planText = T2(() => canonical(plan), [turn.envelope]);
    const formatted = T2(() => format === "json" ? canonical(envelope) : format === "markdown" ? markdown(turn, void 0, state.metadataViews?.[turn.id] ?? turn.envelope) : "", [format, turn]);
    const publicationNotice = publicationRelationLabel(state.publicationRelations?.[turn.id] ?? "unknown");
    const runPlan = (prepare) => {
      try {
        const parsed2 = JSON.parse(draft);
        if (prepare) {
          parsed2.digest = "";
        }
        send({ type: "run", input: turn.input, prepare, plan: parsed2 });
        setIssue("");
      } catch {
        setIssue("Enter a valid JSON plan.");
      }
    };
    return /* @__PURE__ */ u3("article", { class: "turn", children: [
      /* @__PURE__ */ u3("div", { class: "input", children: turn.input }),
      /* @__PURE__ */ u3("time", { children: new Date(turn.at).toLocaleTimeString() }),
      turn.selection && /* @__PURE__ */ u3("p", { class: "note", children: [
        "Read named indexes and index sets: ",
        /* @__PURE__ */ u3("code", { children: openSelectionArgv(turn.selection).join(" ") })
      ] }),
      publicationNotice && /* @__PURE__ */ u3("p", { class: "warning", children: publicationNotice }),
      turn.error && /* @__PURE__ */ u3("p", { class: "error", role: "alert", children: turn.error }),
      turn.trimmed && /* @__PURE__ */ u3("p", { class: "warning", children: "Saved result trimmed. Re-run to collect current evidence." }),
      turn.request && /* @__PURE__ */ u3("details", { children: [
        /* @__PURE__ */ u3("summary", { children: "Complete original request" }),
        /* @__PURE__ */ u3("pre", { children: canonical(turn.request) })
      ] }),
      !!turn.nativeFailure && /* @__PURE__ */ u3("section", { class: "result", "aria-label": "Original native refusal", children: [
        /* @__PURE__ */ u3("details", { class: "complete-response", children: [
          /* @__PURE__ */ u3("summary", { children: "Complete original native refusal" }),
          /* @__PURE__ */ u3("p", { class: "note", children: "Captured native response from this attempt. It does not certify current authorization or publication freshness." }),
          /* @__PURE__ */ u3("pre", { children: canonical(turn.nativeFailure) })
        ] }),
        /* @__PURE__ */ u3("footer", { children: [
          /* @__PURE__ */ u3("button", { onClick: () => send({ type: "copy", turn: turn.id, format: "json" }), children: "Copy JSON" }),
          /* @__PURE__ */ u3("button", { onClick: () => send({ type: "copy", turn: turn.id, format: "markdown" }), children: "Copy Markdown" }),
          /* @__PURE__ */ u3("button", { onClick: () => send({ type: "export", turn: turn.id, format: "json" }), children: "Save JSON export..." }),
          /* @__PURE__ */ u3("button", { onClick: () => send({ type: "export", turn: turn.id, format: "markdown" }), children: "Save Markdown export..." })
        ] })
      ] }),
      !!turn.envelope && /* @__PURE__ */ u3(S, { children: [
        /* @__PURE__ */ u3("details", { children: [
          /* @__PURE__ */ u3("summary", { children: "Plan and evidence details" }),
          /* @__PURE__ */ u3("button", { disabled: state.busy, onClick: () => send({ type: "rerun", turn: turn.id }), children: "Re-run with current evidence" }),
          /* @__PURE__ */ u3("pre", { children: planText }),
          /* @__PURE__ */ u3("button", { onClick: () => {
            setEdit(!edit);
            setDraft(planText);
          }, children: "Edit plan" }),
          /* @__PURE__ */ u3("button", { onClick: () => send({ type: "remember", turn: turn.id }), children: "Remember accepted inputs" }),
          /* @__PURE__ */ u3("button", { onClick: () => send({ type: "saveStrategy", turn: turn.id }), children: "Save compatible inputs to Strategies…" })
        ] }),
        Array.isArray(plan.steps) && plan.steps.length > 0 && ["prepared", "needs_input"].includes(String(envelope.status)) && /* @__PURE__ */ u3("button", { disabled: state.busy, onClick: () => {
          setEdit(true);
          setDraft(planText);
        }, children: "Review proposed plan" }),
        edit && /* @__PURE__ */ u3("div", { class: "editor", children: [
          /* @__PURE__ */ u3("label", { children: [
            "Plan JSON",
            /* @__PURE__ */ u3("textarea", { value: draft, onInput: (e3) => setDraft(e3.currentTarget.value), spellcheck: false })
          ] }),
          /* @__PURE__ */ u3("button", { onClick: () => runPlan(true), children: "Prepare edited plan" }),
          /* @__PURE__ */ u3("button", { disabled: state.busy, onClick: () => runPlan(false), children: "Run plan" }),
          issue && /* @__PURE__ */ u3("p", { role: "alert", children: issue })
        ] }),
        Array.isArray(envelope.requires_input) && envelope.requires_input.length > 0 && /* @__PURE__ */ u3("div", { class: "warning", children: [
          /* @__PURE__ */ u3("p", { children: envelope.requires_input.map((r3) => String(object2(r3).reason ?? r3)).join(" · ") }),
          envelope.requires_input.flatMap((raw) => {
            const requirement = object2(raw);
            return Array.isArray(requirement.choices) && ["name", "path"].includes(String(requirement.field)) ? requirement.choices.map((choice) => /* @__PURE__ */ u3("button", { disabled: state.busy, onClick: () => {
              const revised = JSON.parse(planText);
              revised.digest = "";
              const target = revised.steps.find((s3) => s3.id === requirement.step);
              if (target) {
                if (requirement.field === "path") {
                  const source = revised.steps.find((s3) => s3.id === target.bind?.[0]?.from);
                  if (source) {
                    source.args.path = choice;
                  }
                } else {
                  target.args.name = choice;
                }
                send({ type: "run", input: turn.input, prepare: false, plan: revised });
              }
            }, children: [
              "Use ",
              String(choice)
            ] })) : [];
          }),
          /* @__PURE__ */ u3("button", { disabled: state.busy, onClick: () => send({ type: "run", input: "/search ".concat(JSON.stringify(turn.input)), prepare: true }), children: "Prepare text search" }),
          /* @__PURE__ */ u3("button", { disabled: state.busy, onClick: () => send({ type: "run", input: "/status", prepare: false }), children: "Index status" }),
          /* @__PURE__ */ u3("button", { disabled: state.busy, onClick: () => send({ type: "run", input: "/diagnostics", prepare: false }), children: "Diagnostics" })
        ] }),
        /* @__PURE__ */ u3("section", { class: "result", children: [
          /* @__PURE__ */ u3("header", { children: [
            /* @__PURE__ */ u3("span", { children: [
              String(envelope.status ?? envelope.stop ?? object2(envelope.context).termination ?? "native result"),
              " · ",
              envelope.schema === "airk.combined-read/1" && isAnswerEnvelope(envelope) ? "".concat(answerPartitions(envelope).length, " native members") : "".concat(envelope.schema === "airk.arbiter.context/2" && Array.isArray(envelope.records) ? envelope.records.length : cards.length, " retained records")
            ] }),
            /* @__PURE__ */ u3("div", { role: "group", "aria-label": "Output format", children: ["structured", "json", "markdown"].map((f4) => /* @__PURE__ */ u3("button", { "aria-pressed": format === f4, onClick: () => setFormat(f4), children: f4 })) })
          ] }),
          Array.isArray(envelope.warnings) && envelope.warnings.map((w3) => /* @__PURE__ */ u3("p", { class: "note", children: String(w3) })),
          Array.isArray(envelope.completeness) && envelope.completeness.length > 0 && /* @__PURE__ */ u3("p", { class: "warning", children: [
            "Incomplete: ",
            envelope.completeness.map(String).join("; ")
          ] }),
          format === "structured" ? /* @__PURE__ */ u3(S, { children: [
            isAnswerEnvelope(envelope) && /* @__PURE__ */ u3("section", { "aria-label": "Native Answer partitions", children: [
              envelope.schema === "airk.combined-read/1" && /* @__PURE__ */ u3(S, { children: [
                /* @__PURE__ */ u3("h3", { children: "Combined Answer" }),
                /* @__PURE__ */ u3("p", { children: "Every native member remains separate. Failed, exhausted and unqueried members do not establish coverage. The complete outer result, publication vector and whole-selection accounting remain in Complete response and every full export." }),
                /* @__PURE__ */ u3("details", { children: [
                  /* @__PURE__ */ u3("summary", { children: "Whole-selection context accounting" }),
                  /* @__PURE__ */ u3("pre", { children: canonical(envelope.context) })
                ] })
              ] }),
              answerPartitions(envelope).map((partition) => /* @__PURE__ */ u3("section", { "aria-label": "Answer partition ".concat(partition.member ?? "local"), children: [
                partition.member !== void 0 && /* @__PURE__ */ u3(S, { children: /* @__PURE__ */ u3("h4", { children: [
                  "Index ",
                  partition.indexId ?? "unavailable",
                  " · member ",
                  partition.member,
                  " · ",
                  partition.outcome
                ] }) }),
                partition.packet ? /* @__PURE__ */ u3(
                  AnswerView,
                  {
                    answer: partition.packet,
                    identity: turn.id,
                    member: partition.member,
                    cards: cards.filter((card) => card.answerMember === partition.member),
                    disabled: state.busy || partition.member !== void 0 && partition.outcome !== "complete"
                  }
                ) : /* @__PURE__ */ u3("p", { children: "No valid locally bound current Answer packet was returned for this member. No local assessment or source authority is inferred." }),
                partition.member !== void 0 && /* @__PURE__ */ u3("details", { children: [
                  /* @__PURE__ */ u3("summary", { children: "Complete original member" }),
                  /* @__PURE__ */ u3("pre", { children: canonical(partition.raw) })
                ] })
              ] }, partition.member ?? "local"))
            ] }),
            /* @__PURE__ */ u3(DiscoveryDetails, { turn, state }),
            cards.length > 0 && /* @__PURE__ */ u3("details", { class: "result-tools", children: [
              /* @__PURE__ */ u3("summary", { children: [
                "Filter and group evidence",
                filter ? " · ".concat(rows.length, " matches") : ""
              ] }),
              /* @__PURE__ */ u3("label", { class: "filter", children: [
                "Filter retained evidence",
                /* @__PURE__ */ u3("input", { value: filter, onInput: (e3) => setFilter(e3.currentTarget.value) })
              ] }),
              /* @__PURE__ */ u3("button", { "aria-pressed": group, onClick: () => setGroup(!group), children: "Group by file" }),
              /* @__PURE__ */ u3("p", { class: "counts", children: [
                "Showing ",
                Math.min(show, rows.length),
                " of ",
                rows.length,
                " matching retained records."
              ] })
            ] }),
            envelope.truncated === true && /* @__PURE__ */ u3("p", { class: "warning", children: "Upstream or output omissions reported." }),
            cards.length > 0 && selectionRow(),
            cards.length > 0 && selectionTooLarge && /* @__PURE__ */ u3("p", { class: "note", role: "status", children: "Too many selected results to copy together. Select fewer results or use Save export." }),
            rows.slice(0, show).map((card) => /* @__PURE__ */ u3("div", { class: "card", children: [
              /* @__PURE__ */ u3("div", { class: "card-title", children: [
                /* @__PURE__ */ u3("input", { type: "checkbox", "aria-label": "Select ".concat(card.path, ":").concat(card.start), checked: selected.includes(card.id), onChange: (e3) => {
                  const checked = e3.currentTarget.checked;
                  setSelected((previous) => checked ? [.../* @__PURE__ */ new Set([...previous, card.id])] : previous.filter((id) => id !== card.id));
                } }),
                /* @__PURE__ */ u3("button", { class: "source", onClick: () => send({ type: "open", turn: turn.id, card: card.id }), children: [
                  card.path,
                  ":",
                  card.start,
                  "–",
                  card.end
                ] })
              ] }),
              /* @__PURE__ */ u3("pre", { children: card.text }),
              /* @__PURE__ */ u3("details", { class: "source-details", children: [
                /* @__PURE__ */ u3("summary", { children: "Source details" }),
                /* @__PURE__ */ u3("small", { children: [
                  "Step: ",
                  card.step
                ] }),
                /* @__PURE__ */ u3("pre", { children: canonical({ index_id: card.indexId, project_ids: card.projectIds, source_root_id: card.sourceRootId, source_relative_path: card.sourceRelativePath, location: card.location, sourceHash: card.hash, publication: card.publication }) })
              ] }),
              /* @__PURE__ */ u3("button", { disabled: state.busy, onClick: () => send({ type: "context", turn: turn.id, card: card.id }), children: "Expand context" })
            ] }, card.id)),
            cards.length > 0 && selectionRow(),
            rows.length > show && /* @__PURE__ */ u3("button", { onClick: () => setShow(show + 20), children: "Show 20 more retained records" }),
            (state.continuations?.[turn.id] ?? []).map((page) => /* @__PURE__ */ u3("button", { disabled: state.busy, onClick: () => send({ type: "continue", turn: turn.id, step: page.id }), children: [
              "Fetch next ",
              page.tool,
              " page · ",
              page.callsRemaining,
              " calls remain",
              page.requiredRecordBytes !== void 0 ? " · needs ".concat(page.requiredRecordBytes, " bytes") : ""
            ] })),
            /* @__PURE__ */ u3("details", { class: "complete-response", open: !cards.length, children: [
              /* @__PURE__ */ u3("summary", { children: "Complete response" }),
              /* @__PURE__ */ u3("pre", { children: canonical(envelope) })
            ] })
          ] }) : /* @__PURE__ */ u3("pre", { children: formatted }),
          /* @__PURE__ */ u3("footer", { children: [
            !selectionUi && /* @__PURE__ */ u3("button", { onClick: () => send({ type: "copy", turn: turn.id, format: format === "json" ? "json" : "markdown" }), children: [
              "Copy ",
              format === "json" ? "JSON" : "Markdown"
            ] }),
            !selectionUi && selected.length > 0 && /* @__PURE__ */ u3("button", { title: copySelectionTitle, disabled: !copySelectionAllowed, onClick: () => send(copySelection), children: [
              "Copy selected (",
              selected.length,
              ")"
            ] }),
            /* @__PURE__ */ u3("button", { onClick: () => send({ type: "export", turn: turn.id, format: format === "json" ? "json" : "markdown" }), children: "Save export…" })
          ] })
        ] })
      ] }),
      !turn.envelope && (state.busy && state.session?.turns.at(-1)?.id === turn.id ? /* @__PURE__ */ u3("p", { role: "status", children: "Collecting evidence…" }) : /* @__PURE__ */ u3("button", { disabled: state.busy, onClick: () => send({ type: "rerun", turn: turn.id }), children: "Re-run with current evidence" }))
    ] });
  }
  function SelectionPicker({ state, onDone }) {
    const choices = state.scopeChoices, current = state.openSelection;
    const known = (kind, id) => !!(kind === "indexes" ? choices?.indexes : choices?.sets)?.some((row) => row.id === id);
    const [picked, setPicked] = d2(() => ({ indexes: (current?.indexes ?? []).filter((id) => known("indexes", id)), sets: (current?.sets ?? []).filter((id) => known("sets", id)) }));
    const [typedIndexes, setTypedIndexes] = d2(() => (current?.indexes ?? []).filter((id) => !known("indexes", id)).join("\n"));
    const [typedSets, setTypedSets] = d2(() => (current?.sets ?? []).filter((id) => !known("sets", id)).join("\n"));
    const [issue, setIssue] = d2("");
    const toggle = (kind, id, checked) => setPicked((prior) => ({ ...prior, [kind]: checked ? [.../* @__PURE__ */ new Set([...prior[kind], id])] : prior[kind].filter((value) => value !== id) }));
    const submit = () => {
      try {
        const selection = validateOpenSelection({ indexes: [...picked.indexes, ...openSelectionLines(typedIndexes)], sets: [...picked.sets, ...openSelectionLines(typedSets)] });
        send({ type: "openSelection", indexes: selection.indexes, sets: selection.sets });
        setIssue("");
        onDone();
      } catch (error) {
        setIssue(error instanceof Error ? error.message : "Review the named indexes and index sets.");
      }
    };
    const list = (kind, legend) => {
      const rows = kind === "indexes" ? choices?.indexes : choices?.sets;
      return rows?.length ? /* @__PURE__ */ u3("fieldset", { class: "choices", children: [
        /* @__PURE__ */ u3("legend", { children: legend }),
        rows.map((row) => /* @__PURE__ */ u3("label", { class: "check", title: row.id, children: [
          /* @__PURE__ */ u3("input", { type: "checkbox", checked: picked[kind].includes(row.id), onChange: (e3) => toggle(kind, row.id, e3.currentTarget.checked) }),
          row.name ?? row.id
        ] }, row.id))
      ] }) : null;
    };
    return /* @__PURE__ */ u3("section", { class: "popover", id: "search-selection", "aria-label": "What reads search", children: [
      /* @__PURE__ */ u3("h2", { children: "Search in" }),
      /* @__PURE__ */ u3("p", { class: "note", children: "Reads use the entry chosen in the list beside this button: a project, this window's search scope (the default), or named indexes and index sets. Name indexes and index sets here to read them whole, as a union, instead of the search scope." }),
      !choices?.loaded && /* @__PURE__ */ u3("button", { type: "button", disabled: state.busy, onClick: () => send({ type: "scopeChoices" }), children: "Load index and index set names" }),
      list("sets", "Index sets"),
      list("indexes", "Indexes"),
      /* @__PURE__ */ u3("label", { children: [
        "Other indexes (id or name, one per line)",
        /* @__PURE__ */ u3("textarea", { rows: 2, spellcheck: false, value: typedIndexes, onInput: (e3) => setTypedIndexes(e3.currentTarget.value) })
      ] }),
      /* @__PURE__ */ u3("label", { children: [
        "Other index sets (id or name, one per line)",
        /* @__PURE__ */ u3("textarea", { rows: 2, spellcheck: false, value: typedSets, onInput: (e3) => setTypedSets(e3.currentTarget.value) })
      ] }),
      issue && /* @__PURE__ */ u3("p", { role: "alert", children: issue }),
      /* @__PURE__ */ u3("button", { type: "button", disabled: state.busy, onClick: submit, children: "Read these indexes and index sets" })
    ] });
  }
  function Icon({ name }) {
    const paths = { "plus": "M12 4v16M4 12h16", "history": "M5 7 9 3h7l5 5v8l-5 5H8l-5-5M3 6v6h6M12 7v5l4 2", "settings": "M3 6h18M3 12h18M3 18h18M7 3l3 3-3 3-3-3zM17 9l3 3-3 3-3-3zM9 15l3 3-3 3-3-3z", "new": "M4 21V7l4-4h8l4 4v14zM9 12h6M12 9v6", "send": "m6 5 7 7-7 7m7-14 7 7-7 7", "stop": "M7 4h10l3 3v10l-3 3H7l-3-3V7z", "reset": "M4 11a8 8 0 1 1 2 7M4 5v6h6", "scope": "M3 5h18M6 12h12M9 19h6" };
    return /* @__PURE__ */ u3("svg", { viewBox: "0 0 24 24", width: "18", height: "18", fill: "none", stroke: "currentColor", "stroke-width": "1.5", "stroke-linecap": "square", "stroke-linejoin": "miter", "aria-hidden": "true", children: [
      /* @__PURE__ */ u3("path", { d: paths[name] }),
      /* @__PURE__ */ u3("path", { class: "icon-accent", d: name === "send" ? "m17 9 3 3-3 3" : "m19 3 2 2-2 2" })
    ] });
  }
  function App() {
    const [state, setState] = d2({ projects: [], recent: [], cards: {}, busy: false, save: "", persistence: true }), [input, setInput] = d2(""), [menu, setMenu] = d2(), [history, setHistory] = d2(-1), [title, setTitle] = d2(""), [formIssue, setFormIssue] = d2("");
    const [contextQuestion, setContextQuestion] = d2(""), [contextBudget, setContextBudget] = d2(() => JSON.stringify(WORKBENCH_CONTEXT_BUDGET, null, 2)), [contextIssue, setContextIssue] = d2("");
    const liveState = A2(state);
    liveState.current = state;
    const formDraft = normalizeWorkbenchFormDraft(state.session?.formDraft) ?? defaultWorkbenchFormDraft(), scope = formDraft.scope, recipe = formDraft.recipe;
    const currentDraft = () => {
      const current = liveState.current;
      if (!current.session || !state.session || current.session.id !== state.session.id || current.project !== state.project || current.session.project !== current.project || current.sessionEpoch !== state.sessionEpoch || !Number.isSafeInteger(current.sessionEpoch)) return;
      return normalizeWorkbenchFormDraft(current.session.formDraft);
    };
    const changeForm = (patch, reset = false) => {
      const current = currentDraft(), owner = liveState.current;
      if (!current || !owner.session || typeof owner.sessionEpoch !== "number") {
        setFormIssue("Choose the form in the current session.");
        return;
      }
      const tool = patch.tool ?? current.activeTool;
      if ((patch.field || reset) && tool !== current.activeTool) {
        setFormIssue("The active form changed. Use its current controls.");
        return;
      }
      const values = { ...current.values };
      if (reset) delete values[tool];
      else if (patch.field) values[tool] = { ...values[tool], [patch.field.name]: patch.field.value };
      const nextDraft = normalizeWorkbenchFormDraft({ ...current, activeTool: tool, values, revision: current.revision + 1, ...patch.scope !== void 0 ? { scope: patch.scope } : {}, ...patch.recipe !== void 0 ? { recipe: patch.recipe } : {} });
      if (!nextDraft) {
        setFormIssue("These form choices exceed the saved-input limits. Shorten a field or reset an unused form.");
        return;
      }
      const fence = { sessionId: owner.session.id, project: owner.session.project, epoch: owner.sessionEpoch, revision: nextDraft.revision, tool };
      const message = reset ? { type: "resetForm", ...fence } : { type: "formDraft", ...fence, ...patch.field ? { values: nextDraft.values[tool] } : {}, ...patch.scope !== void 0 ? { scope: patch.scope } : {}, ...patch.recipe !== void 0 ? { recipe: patch.recipe } : {} };
      if (!parseWorkbenchMessage(message)) {
        setFormIssue("This form setting is outside the supported input limits.");
        return;
      }
      const next = { ...owner, session: { ...owner.session, formDraft: nextDraft } };
      liveState.current = next;
      setState(next);
      setFormIssue("");
      send(message);
    };
    const setScope = (scope2) => changeForm({ scope: scope2 }), setRecipe = (recipe2) => changeForm({ recipe: recipe2 });
    const composer = A2(null), transcript = A2(null), menuTrigger = A2(), openMenu = A2();
    const latest = state.session?.turns.at(-1);
    _2(() => {
      const container = transcript.current, turn = container?.lastElementChild;
      if (!container || !latest || !turn) {
        return;
      }
      const target = turn.querySelector(".input") ?? turn;
      container.scrollTop += target.getBoundingClientRect().top - container.getBoundingClientRect().top;
    }, [state.session?.id, latest?.id, !!latest?.envelope, latest?.error]);
    openMenu.current = menu;
    const toggle = (next, event) => {
      menuTrigger.current = event.currentTarget.getAttribute("aria-controls") ?? void 0;
      setMenu(menu === next ? void 0 : next);
      setTitle(state.session?.title ?? "");
    };
    h2(() => {
      const listener = (event) => {
        if (["state", "status", "saved", "formDraft"].includes(event.data?.type)) {
          setState((previous) => {
            const next = applyWorkbenchUpdate(previous, event.data);
            liveState.current = next;
            return next;
          });
        } else if (event.data?.type === "prefillForm") {
          setMenu("add");
        } else if (event.data?.type === "prefill" && typeof event.data.input === "string") {
          setInput(event.data.input);
          composer.current?.focus();
        }
      };
      window.addEventListener("message", listener);
      send({ type: "ready" });
      return () => window.removeEventListener("message", listener);
    }, []);
    h2(() => {
      setHistory(-1);
      setFormIssue("");
    }, [state.session?.id]);
    h2(() => {
      const escape = (event) => {
        if (event.key === "Escape" && openMenu.current) {
          setMenu(void 0);
          requestAnimationFrame(() => {
            document.querySelector('button[aria-controls="'.concat(menuTrigger.current, '"]'))?.focus();
          });
        }
      }, outside = (event) => {
        if (!event.target.closest(".menu-anchor")) setMenu(void 0);
      };
      document.addEventListener("keydown", escape);
      document.addEventListener("pointerdown", outside);
      return () => {
        document.removeEventListener("keydown", escape);
        document.removeEventListener("pointerdown", outside);
      };
    }, []);
    const run = (prepare = false) => {
      const current = currentDraft();
      if (current && !state.busy && (input.trim() || current.recipe)) {
        send({ type: "run", input, prepare, pathPrefix: current.scope, ...current.recipe ? { recipe: current.recipe } : {} });
        setInput("");
        setHistory(-1);
        setMenu(void 0);
      }
    };
    const action = (message) => {
      send(message);
      if (["new", "clear", "fork"].includes(message.type)) setInput("");
      setMenu(void 0);
    };
    return /* @__PURE__ */ u3("main", { children: [
      /* @__PURE__ */ u3("header", { class: "toolbar", children: [
        /* @__PURE__ */ u3("select", { class: "project", "aria-label": "Project", title: "Select project", value: state.project ?? "", onChange: (e3) => action({ type: "project", id: e3.currentTarget.value }), children: [
          /* @__PURE__ */ u3("option", { value: "", disabled: true, children: "Choose project" }),
          state.projects.map((p3) => /* @__PURE__ */ u3("option", { value: p3.id, children: p3.name }))
        ] }),
        /* @__PURE__ */ u3("div", { class: "menu-anchor", children: [
          /* @__PURE__ */ u3("button", { class: "icon-button", title: "Search in: the search scope, or named indexes and index sets", "aria-label": "Choose what reads search", "aria-expanded": menu === "selection", "aria-controls": "search-selection", onClick: (e3) => toggle("selection", e3), children: /* @__PURE__ */ u3(Icon, { name: "scope" }) }),
          menu === "selection" && /* @__PURE__ */ u3(SelectionPicker, { state, onDone: () => setMenu(void 0) })
        ] }),
        /* @__PURE__ */ u3("div", { class: "menu-anchor", children: [
          /* @__PURE__ */ u3("button", { class: "icon-button", title: "Session history", "aria-label": "Session history", "aria-expanded": menu === "history", "aria-controls": "session-history", onClick: (e3) => toggle("history", e3), children: /* @__PURE__ */ u3(Icon, { name: "history" }) }),
          menu === "history" && /* @__PURE__ */ u3("section", { class: "popover", id: "session-history", "aria-label": "Session history", children: [
            /* @__PURE__ */ u3("h2", { children: "History" }),
            !state.recent.length && /* @__PURE__ */ u3("p", { children: "No saved sessions yet." }),
            state.recent.map((s3) => /* @__PURE__ */ u3("div", { class: "history-row", children: [
              /* @__PURE__ */ u3("button", { class: "history-item", onClick: () => action({ type: "recent", id: s3.id }), children: [
                s3.title,
                /* @__PURE__ */ u3("small", { children: [
                  s3.turns,
                  " turns · ",
                  new Date(s3.updatedAt).toLocaleDateString()
                ] })
              ] }),
              /* @__PURE__ */ u3("button", { class: "icon-button", "aria-label": "Delete ".concat(s3.title), title: "Delete session", onClick: () => action({ type: "delete", id: s3.id }), children: "×" })
            ] }))
          ] })
        ] }),
        /* @__PURE__ */ u3("div", { class: "menu-anchor", children: [
          /* @__PURE__ */ u3("button", { class: "icon-button", title: "Settings and session actions", "aria-label": "Settings and session actions", "aria-expanded": menu === "settings", "aria-controls": "session-settings", onClick: (e3) => toggle("settings", e3), children: /* @__PURE__ */ u3(Icon, { name: "settings" }) }),
          menu === "settings" && /* @__PURE__ */ u3("section", { class: "popover", id: "session-settings", "aria-label": "Settings and session actions", children: [
            /* @__PURE__ */ u3("h2", { children: "Session" }),
            /* @__PURE__ */ u3("label", { children: [
              "Title",
              /* @__PURE__ */ u3("input", { maxLength: 80, value: title, onInput: (e3) => setTitle(e3.currentTarget.value) })
            ] }),
            /* @__PURE__ */ u3("button", { onClick: () => action({ type: "rename", title }), children: "Rename" }),
            /* @__PURE__ */ u3("button", { onClick: () => action({ type: "fork" }), children: "Fork session" }),
            /* @__PURE__ */ u3("button", { onClick: () => action({ type: "copy", format: "markdown" }), children: "Copy session as Markdown" }),
            /* @__PURE__ */ u3("button", { onClick: () => action({ type: "copy", format: "json" }), children: "Copy session as JSON" }),
            /* @__PURE__ */ u3("button", { onClick: () => action({ type: "export", format: "json" }), children: "Export session…" }),
            state.save && /* @__PURE__ */ u3("p", { class: "note", children: state.save }),
            /* @__PURE__ */ u3("button", { onClick: () => action({ type: "retrySave" }), children: "Retry saving" }),
            /* @__PURE__ */ u3("details", { children: [
              /* @__PURE__ */ u3("summary", { children: "Index readiness" }),
              /* @__PURE__ */ u3("button", { onClick: () => send({ type: "refresh" }), children: "Refresh readiness" }),
              /* @__PURE__ */ u3("p", { class: "note", children: "Collection readiness does not verify live source files. Dated saved-source checks appear with individual results." }),
              /* @__PURE__ */ u3("pre", { children: state.status ? canonical(state.status) : "Readiness not checked" })
            ] }),
            /* @__PURE__ */ u3("button", { onClick: () => action({ type: "manage" }), children: "AIRK Management" }),
            /* @__PURE__ */ u3("button", { onClick: () => action({ type: "help" }), children: "Workbench help" }),
            /* @__PURE__ */ u3("details", { children: [
              /* @__PURE__ */ u3("summary", { children: "Delete sessions" }),
              state.session && /* @__PURE__ */ u3("button", { onClick: () => action({ type: "delete", id: state.session.id }), children: "Delete this session" }),
              /* @__PURE__ */ u3("button", { onClick: () => action({ type: "clear" }), children: "Clear project sessions…" })
            ] })
          ] })
        ] }),
        /* @__PURE__ */ u3("button", { class: "icon-button", title: "New session", "aria-label": "New session", onClick: () => {
          action({ type: "new" });
          setInput("");
          composer.current?.focus();
        }, children: /* @__PURE__ */ u3(Icon, { name: "new" }) })
      ] }),
      state.notice && /* @__PURE__ */ u3("p", { class: "notice", role: "status", children: state.notice }),
      state.openSelection && /* @__PURE__ */ u3("p", { class: "note", children: [
        "Reads name these indexes and index sets explicitly, not the search scope: ",
        /* @__PURE__ */ u3("code", { children: openSelectionArgv(state.openSelection).join(" ") })
      ] }),
      state.sourceOpen && /* @__PURE__ */ u3("details", { children: [
        /* @__PURE__ */ u3("summary", { children: "Complete source-open observation" }),
        /* @__PURE__ */ u3("p", { children: "This dated native receipt is separate from the original query and does not promise continued freshness." }),
        /* @__PURE__ */ u3("pre", { children: canonical(state.sourceOpen.observation) }),
        /* @__PURE__ */ u3("button", { onClick: () => action({ type: "exportSourceOpen", id: state.sourceOpen.id, format: "copy" }), children: "Copy complete JSON" }),
        /* @__PURE__ */ u3("button", { onClick: () => action({ type: "exportSourceOpen", id: state.sourceOpen.id, format: "json" }), children: "Export JSON..." })
      ] }),
      (state.referenceRefresh ?? []).map((value) => /* @__PURE__ */ u3("section", { class: "reference-refresh", children: [
        /* @__PURE__ */ u3("h4", { children: "Reviewed native refresh" }),
        /* @__PURE__ */ u3("button", { disabled: state.busy, onClick: () => send({ type: "checkReferenceRefresh", id: String(value.id) }), children: "Check exact receipt" }),
        /* @__PURE__ */ u3("button", { disabled: state.busy || value.canRequery !== true, onClick: () => send({ type: "requeryReferenceRefresh", id: String(value.id) }), children: "Review same query again..." }),
        /* @__PURE__ */ u3("button", { onClick: () => send({ type: "exportReferenceRefresh", id: String(value.id), format: "json", action: "copy" }), children: "Copy complete JSON" }),
        /* @__PURE__ */ u3("button", { onClick: () => send({ type: "exportReferenceRefresh", id: String(value.id), format: "markdown", action: "copy" }), children: "Copy complete Markdown" }),
        /* @__PURE__ */ u3("button", { onClick: () => send({ type: "exportReferenceRefresh", id: String(value.id), format: "json", action: "save" }), children: "Save complete export..." }),
        /* @__PURE__ */ u3("p", { children: "Original evidence stays historical. Missing receipts never replay the mutation." }),
        /* @__PURE__ */ u3("pre", { children: canonical(value) })
      ] }, String(value.id))),
      /* @__PURE__ */ u3("div", { class: "transcript", ref: transcript, children: !state.session?.turns.length ? /* @__PURE__ */ u3("section", { class: "welcome", children: [
        /* @__PURE__ */ u3("h1", { children: "Explore your project" }),
        /* @__PURE__ */ u3("p", { children: "Search indexed files and collect cited evidence." }),
        /* @__PURE__ */ u3("div", { class: "suggestions", children: ["/search retry --mode lexical", "/symbols EvidenceClient", "/files"].map((example) => /* @__PURE__ */ u3("button", { onClick: () => {
          setInput(example);
          composer.current?.focus();
        }, children: example })) }),
        !state.projects.length && /* @__PURE__ */ u3("p", { children: [
          "No reviewed AIRK Index or Project scope is active in this window. Trust the workspace and finish setup if needed, then review and use a scope in Management. ",
          /* @__PURE__ */ u3("button", { onClick: () => action({ type: "manage" }), children: "Open Management" })
        ] })
      ] }) : state.session.turns.map((turn) => /* @__PURE__ */ u3(Turn, { turn, state }, turn.id)) }),
      /* @__PURE__ */ u3("div", { class: "composer", children: [
        (scope || recipe) && /* @__PURE__ */ u3("div", { class: "compose-context", children: [
          scope && /* @__PURE__ */ u3("button", { title: "Clear scope", onClick: () => setScope(""), children: [
            scope,
            " ×"
          ] }),
          recipe && /* @__PURE__ */ u3("button", { title: "Clear recipe", onClick: () => setRecipe(""), children: [
            RECIPES.find(([id]) => id === recipe)?.[1],
            " ×"
          ] })
        ] }),
        /* @__PURE__ */ u3("button", { type: "button", disabled: !state.project || state.busy || !input.trim(), onClick: () => {
          send({ type: "answer", question: input.trim() });
          setInput("");
        }, children: "Gather cited context" }),
        /* @__PURE__ */ u3("p", { class: "note", children: "Cited context uses bounded canonical reads and reports gaps. Optional local Arbiter models require explicit Service enrollment; unavailable models never imply a successful model decision." }),
        /* @__PURE__ */ u3("details", { children: [
          /* @__PURE__ */ u3("summary", { children: "Cited context question and budget" }),
          /* @__PURE__ */ u3("label", { children: [
            "Context question (up to 16,384 UTF-8 bytes)",
            /* @__PURE__ */ u3("textarea", { "aria-label": "Context question", maxLength: 16384, value: contextQuestion, onInput: (e3) => setContextQuestion(e3.currentTarget.value) })
          ] }),
          /* @__PURE__ */ u3("label", { children: [
            "Native context budget (JSON)",
            /* @__PURE__ */ u3("textarea", { "aria-label": "Native context budget", maxLength: 2048, value: contextBudget, onInput: (e3) => setContextBudget(e3.currentTarget.value) })
          ] }),
          /* @__PURE__ */ u3("p", { class: "note", children: "The native service validates budget limits. A smaller budget can return gaps or omissions." }),
          contextIssue && /* @__PURE__ */ u3("p", { role: "alert", children: contextIssue }),
          /* @__PURE__ */ u3("button", { type: "button", disabled: !state.project || state.busy || !contextQuestion.trim(), onClick: () => {
            try {
              const message = parseWorkbenchMessage({ type: "answer", question: contextQuestion, budget: JSON.parse(contextBudget) });
              if (!message) throw new Error("Provide a nonempty question within 16,384 UTF-8 bytes and every integer native budget field.");
              setContextIssue("");
              send(message);
            } catch (error) {
              setContextIssue(error instanceof Error ? error.message : String(error));
            }
          }, children: "Gather with this budget" })
        ] }),
        input.startsWith("/") && !input.includes(" ") && /* @__PURE__ */ u3("div", { class: "commands", "aria-label": "Commands", children: COMMANDS.filter((c3) => c3.startsWith(input)).map((c3) => /* @__PURE__ */ u3("button", { onClick: () => {
          setInput("".concat(c3, " "));
          composer.current?.focus();
        }, children: c3 })) }),
        /* @__PURE__ */ u3("textarea", { ref: composer, maxLength: 4096, "aria-label": "Request indexed evidence", placeholder: recipe ? "Enter a name or statement for this recipe" : "Ask about this project, or type /", value: input, onInput: (e3) => {
          setInput(e3.currentTarget.value);
          setHistory(-1);
        }, onKeyDown: (e3) => {
          if (e3.key === "Enter" && !e3.shiftKey && !e3.isComposing) {
            e3.preventDefault();
            run();
          }
          if (e3.key === "ArrowUp" && !input && state.session?.turns.length) {
            e3.preventDefault();
            const next = history < 0 ? state.session.turns.length - 1 : Math.max(0, history - 1);
            setHistory(next);
            setInput(state.session.turns[next].input);
          }
        } }),
        /* @__PURE__ */ u3("div", { class: "compose-actions", children: [
          /* @__PURE__ */ u3("div", { class: "menu-anchor add-anchor", children: [
            /* @__PURE__ */ u3("button", { class: "icon-button add-button", title: "Add scope, recipe or tool", "aria-label": "Add scope, recipe or tool", "aria-expanded": menu === "add", "aria-controls": "evidence-options", onClick: (e3) => toggle("add", e3), children: /* @__PURE__ */ u3(Icon, { name: "plus" }) }),
            menu === "add" && /* @__PURE__ */ u3("section", { class: "popover above", id: "evidence-options", "aria-label": "Evidence options", children: [
              /* @__PURE__ */ u3("h2", { children: "Collect evidence" }),
              /* @__PURE__ */ u3("label", { children: [
                "Scope",
                /* @__PURE__ */ u3("input", { "aria-label": "Literal collection-relative path prefix", placeholder: "Project root, or src/component/", maxLength: 1024, value: scope, onInput: (e3) => setScope(e3.currentTarget.value) })
              ] }),
              /* @__PURE__ */ u3("label", { children: [
                "Recipe",
                /* @__PURE__ */ u3("select", { "aria-label": "Recipe", value: recipe, onChange: (e3) => setRecipe(e3.currentTarget.value), children: [
                  /* @__PURE__ */ u3("option", { value: "", children: "Command or supported request" }),
                  RECIPES.map(([id, label]) => /* @__PURE__ */ u3("option", { value: id, children: label }))
                ] })
              ] }),
              /* @__PURE__ */ u3("button", { disabled: !state.project || state.busy || !input.trim() && !recipe, onClick: () => run(true), children: "Prepare plan before running" }),
              /* @__PURE__ */ u3("button", { disabled: !state.project, onClick: () => send({ type: "forms" }), children: "Tool forms" }),
              state.operations && /* @__PURE__ */ u3(ToolForm, { state, issue: formIssue, currentDraft, onSelect: (tool) => changeForm({ tool }), onField: (tool, name, value) => changeForm({ tool, field: { name, value } }), onReset: (tool) => changeForm({ tool }, true), onIssue: setFormIssue }),
              /* @__PURE__ */ u3("button", { onClick: () => action({ type: "help" }), children: "Commands and help" })
            ] })
          ] }),
          /* @__PURE__ */ u3("small", { title: state.save || "Requests read the local index", children: "Sources read-only · Refresh requires review (no generative AI used in AIRK Workbench)" }),
          state.busy ? /* @__PURE__ */ u3("button", { class: "icon-button send-button", "aria-label": "Stop collecting evidence", title: "Stop", onClick: () => send({ type: "stop" }), children: /* @__PURE__ */ u3(Icon, { name: "stop" }) }) : /* @__PURE__ */ u3("button", { class: "icon-button send-button", "aria-label": "Run request", title: "Run · Enter (Shift+Enter for a new line)", disabled: !state.project || !input.trim() && !recipe, onClick: () => run(), children: /* @__PURE__ */ u3(Icon, { name: "send" }) })
        ] })
      ] }),
      /* @__PURE__ */ u3("div", { class: "sr-only", "aria-live": "polite", children: state.busy ? "Collecting evidence" : "Ready" })
    ] });
  }
  R(/* @__PURE__ */ u3(App, {}), document.getElementById("root"));
})();
