# SPDX-License-Identifier: Apache-2.0
"""Resolve bounded publication references without rewriting received evidence."""
from copy import deepcopy
import hashlib
import json
import re

HEADER_SCHEMA = "airk.shared-native-header-templates/1"
HEADER_REFERENCE = "$airk_shared_native_header_fields"
CAPABILITY_REFERENCE = "$airk_shared_capability_fields"
HEADER_ROOTS = frozenset(("collection", "discovery_observation", "discovery_readiness", "semantic_readiness",
                          "execution", "freshness", "generation", "publication", "request_id", "scope_observation", "truncated"))
FORBIDDEN = frozenset((HEADER_REFERENCE, CAPABILITY_REFERENCE, "__proto__", "constructor", "prototype"))
PUBLICATION_PATHS = (("publication",), ("read", "publication"), ("read", "current_publication"),
                     ("evidence", "publication"), ("evidence", "read", "publication"), ("evidence", "read", "current_publication"))


def canonical(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"), allow_nan=False).encode("utf-8")


def framed_digest(domain, value):
    digest = hashlib.sha256(b"AIRK-DIGEST-1\0")
    for field in (domain.encode("ascii"), canonical(value)):
        digest.update(len(field).to_bytes(8, "big"))
        digest.update(field)
    return digest.hexdigest()


def primitive(value):
    if value is None:
        return "null"
    if type(value) is bool:
        return "boolean"
    if type(value) is int and abs(value) <= 9007199254740991:
        return "integer"
    if isinstance(value, str) and len(value.encode("utf-8")) <= 8192:
        return "string"
    return None


def bounded_value(value, depth=0, remaining=None):
    remaining = [32768] if remaining is None else remaining
    remaining[0] -= 1
    if depth > 32 or remaining[0] < 0:
        return False
    if isinstance(value, dict):
        return len(value) <= 1024 and all(isinstance(key, str) and len(key) <= 128 and key.isascii() and key not in FORBIDDEN
                   and bounded_value(item, depth + 1, remaining) for key, item in value.items())
    if isinstance(value, list):
        return len(value) <= 1024 and all(bounded_value(item, depth + 1, remaining) for item in value)
    return primitive(value) is not None


def at(value, path):
    for segment in path:
        if isinstance(segment, str) and isinstance(value, dict):
            value = value[segment]
        elif type(segment) is int and isinstance(value, list) and 0 <= segment < len(value):
            value = value[segment]
        else:
            raise ValueError("combined_header_target_invalid")
    return value


def header_templates(table):
    if (not isinstance(table, dict) or set(table) != {"schema", "values"} or table["schema"] != HEADER_SCHEMA
            or not isinstance(table["values"], dict) or not 0 < len(table["values"]) <= 32
            or len(canonical(table)) > 65536):
        raise ValueError("combined_header_namespace_invalid")
    for identity, template in table["values"].items():
        if (not isinstance(identity, str) or not re.fullmatch("[0-9a-f]{64}", identity)
                or not isinstance(template, dict) or set(template) != {"fields", "slots"}
                or not isinstance(template["fields"], dict) or not template["fields"]
                or not set(template["fields"]) <= HEADER_ROOTS or not bounded_value(template["fields"])
                or not isinstance(template["slots"], list) or len(template["slots"]) > 512):
            raise ValueError("combined_header_template_invalid")
        seen = set()
        for slot in template["slots"]:
            if not isinstance(slot, dict) or set(slot) != {"path", "kind"}:
                raise ValueError("combined_header_slot_invalid")
            path = slot["path"]
            if (not isinstance(path, list) or not 0 < len(path) <= 16 or not isinstance(path[0], str)
                    or path[0] not in HEADER_ROOTS or slot["kind"] not in ("string", "boolean", "integer", "null")
                    or any(not ((isinstance(part, str) and 0 < len(part) <= 128 and part.isascii() and part not in FORBIDDEN)
                               or (type(part) is int and 0 <= part < 1024)) for part in path)
                    or tuple(path) in seen or at(template["fields"], path) is not None):
                raise ValueError("combined_header_slot_invalid")
            seen.add(tuple(path))
        if identity != framed_digest("airk/shared-native-header-template/1", template):
            raise ValueError("combined_header_digest_mismatch")
    return table["values"]


def member_result(combined, index_id):
    """Expand only a selected native result's metadata, retaining raw addresses."""
    members = combined.get("members", [])
    identities = [member.get("index_id") for member in members]
    if (combined.get("schema") != "airk.combined-read/1" or not 0 < len(members) <= 32
            or len(set(identities)) != len(members) or index_id not in identities):
        raise ValueError("combined_member_namespace_invalid")
    result = deepcopy(members[identities.index(index_id)]["result"])
    table = combined.get("shared_values")
    if table is not None:
        if table.get("schema") == HEADER_SCHEMA:
            templates = header_templates(table)
            if isinstance(result, dict) and CAPABILITY_REFERENCE in result:
                raise ValueError("combined_header_namespace_mixed")
            if isinstance(result, dict) and HEADER_REFERENCE in result:
                reference = result.pop(HEADER_REFERENCE)
                if not isinstance(reference, dict) or set(reference) != {"id", "values"} or reference["id"] not in templates:
                    raise ValueError("combined_header_reference_invalid")
                template = templates[reference["id"]]
                values = reference["values"]
                if not isinstance(values, list) or len(values) != len(template["slots"]):
                    raise ValueError("combined_header_slot_count")
                fields = deepcopy(template["fields"])
                for slot, item in zip(template["slots"], values):
                    if primitive(item) != slot["kind"]:
                        raise ValueError("combined_header_slot_kind")
                    parent = at(fields, slot["path"][:-1])
                    parent[slot["path"][-1]] = deepcopy(item)
                if set(fields) & set(result):
                    raise ValueError("combined_header_collision")
                result.update(fields)
        elif table.get("schema") == "airk.shared-capability-fields/1":
            # Capability reads have a separate contract and are not evidence
            # benchmark inputs; refuse referenced forms rather than guess.
            if isinstance(result, dict) and CAPABILITY_REFERENCE in result:
                raise ValueError("combined_capability_decoder_required")
        else:
            raise ValueError("combined_shared_namespace_invalid")
    if isinstance(result, dict) and HEADER_REFERENCE in result:
        raise ValueError("combined_header_table_missing")
    for path in PUBLICATION_PATHS:
        try:
            reference = at(result, path)
        except (KeyError, ValueError):
            continue
        if isinstance(reference, dict) and "member" in reference:
            if set(reference) != {"member"}:
                raise ValueError("combined_publication_reference_invalid")
            at(result, path[:-1])[path[-1]] = publication(combined, reference, index_id)
    if len(canonical(result)) > 65536:
        raise ValueError("combined_materialized_member_budget")
    return result


def publication(combined, reference, index_id):
    rows = combined.get("publication_vector", {}).get("members", [])
    identities = [row.get("index_id") for row in rows]
    if not 0 < len(rows) <= 32 or len(set(identities)) != len(rows):
        raise ValueError("combined_publication_vector_invalid")
    if not isinstance(reference, dict):
        raise ValueError("combined_publication_invalid")
    if set(reference) == {"member"}:
        offset = reference["member"]
        if type(offset) is not int or not 0 <= offset < len(rows) or identities[offset] != index_id:
            raise ValueError("combined_publication_reference_invalid")
        result = rows[offset].get("publication")
    else:
        result = reference
    expected = next((row.get("publication") for row in rows if row.get("index_id") == index_id), None)
    if not isinstance(result, dict) or not result or result != expected:
        raise ValueError("combined_publication_mismatch")
    return deepcopy(result)


def ranked_origin(combined, rank):
    origin = deepcopy(rank["origin"])
    if origin.get("index_id") != rank.get("index_id"):
        raise ValueError("combined_origin_index_mismatch")
    if "publication_ref" in origin:
        if "publication" in origin:
            raise ValueError("combined_origin_publication_ambiguous")
        reference = origin.pop("publication_ref")
        if not isinstance(reference, dict) or set(reference) != {"member"}:
            raise ValueError("combined_publication_reference_invalid")
    else:
        reference = origin.get("publication")
    origin["publication"] = publication(combined, reference, rank["index_id"])
    return origin
