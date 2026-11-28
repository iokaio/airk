# SPDX-License-Identifier: Apache-2.0
"""Read current file eligibility from the single native index bound to this session."""

from contextlib import closing
from time import monotonic

import common
import product_skill_http


class InventoryUnavailable(ValueError):
    def __init__(self, page, identity, reply):
        self.page = page
        self.publication = identity[0] if identity else None
        self.reply = reply
        result = reply.get("result", {})
        native = (result.get("structuredContent") or {}).get("error") or reply.get("error") or {}
        self.code = native.get("code")
        super().__init__(f"Native workspace path inventory failed at page {page}: {self.code or 'untyped error'}")


class InventoryCache:
    """Retain a complete eligible set across calls to the same bound session."""

    def __init__(self):
        self.key = None
        self.paths = frozenset()


def published_paths(client, index, metrics=None, cache=None, observation=None):
    started = monotonic()
    args = {"prefix": "", "limit": 100, "max_bytes": 65536, "include_status": True, "verification": "off"}
    seen, paths = set(), set()
    identity = key = None
    cached = False
    page = 0
    while True:
        page += 1
        reply = client.exchange({"jsonrpc": "2.0", "id": page + 1, "method": "tools/call",
                                 "params": {"name": "list_indexed_files", "arguments": dict(args)}})
        envelope = reply.get("result") or {}
        if reply.get("error") or envelope.get("isError") or not envelope:
            raise InventoryUnavailable(page, identity, reply)
        body = common.envelope_body(envelope)
        if not isinstance(body, dict) or (body.get("read_scope") or {}).get("index_ids") != [index]:
            raise ValueError("Native workspace path inventory returned a different scope")
        token = (body.get("publication") or {}).get("token")
        fingerprint = (body.get("scope_observation") or {}).get("fingerprint")
        if not token or not fingerprint or (body.get("discovery_readiness") or {}).get("state") != "ready":
            raise ValueError("Native workspace path inventory lacks a ready, identified publication")
        if identity and (token, fingerprint) != identity:
            raise ValueError("Native workspace path inventory changed between pages")
        if observation is not None:
            raw = body["scope_observation"]
            if page == 1:
                observation.update(raw)
            elif observation != raw:
                raise ValueError("Native workspace authorization changed between pages")
        identity = (token, fingerprint)
        rows = body.get("paths")
        if not isinstance(rows, list) or any(not isinstance(path, str) or not path for path in rows):
            raise ValueError("Malformed native workspace path inventory")
        if len(set(rows)) != len(rows) or not paths.isdisjoint(rows):
            raise ValueError("Repeated path in native workspace inventory")
        paths.update(rows)
        cursor = body.get("cursor")
        finished = body.get("truncated") is False and not cursor
        if not finished and (body.get("truncated") is not True or not isinstance(cursor, str)
                             or not cursor or cursor in seen or not rows):
            raise ValueError("Incomplete native workspace path inventory")
        if page == 1:
            key = (index, token, fingerprint, tuple(rows), cursor)
            if cache is not None and key == cache.key:
                cached = True
                break
        if finished:
            break
        seen.add(cursor)
        args["cursor"] = cursor
    if cache is not None:
        if cached:
            paths = cache.paths
        else:
            cache.key, cache.paths = key, frozenset(paths)
    if metrics is not None:
        metrics.update(publication=token, scope_fingerprint=fingerprint, inventory_pages=page, inventory_cached=cached,
                       inventory_seconds=round(monotonic() - started, 4))
    return paths


def check_denies(paths, rules):
    """A listing that includes a recorded denied path cannot authorize a live read."""
    if not isinstance(rules, list):
        raise ValueError("Native workspace source policy missing")
    for rule in rules:
        if (not isinstance(rule, (list, tuple)) or len(rule) != 2 or rule[0] not in ("exact", "subtree")
                or not isinstance(rule[1], str) or not rule[1]):
            raise ValueError("Unknown native workspace deny rule")
        kind, denied = rule
        if denied in paths or (kind == "subtree" and any(path.startswith(denied + "/") for path in paths)):
            raise ValueError("Native workspace listing contradicts a recorded deny")


def eligible_paths(handle, repo, metrics=None, cache=None, observation=None):
    with closing(product_skill_http.ScopedClient(handle, repo)) as client:
        init = client.exchange({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "workspace-scope", "version": "1"}}})
        if "result" not in init:
            raise ValueError("Native workspace path session failed to initialize")
        client.exchange({"jsonrpc": "2.0", "method": "notifications/initialized"})
        paths = published_paths(client, client.index, metrics, cache, observation)
        records = common.read_json(handle.path)["indexes"]
        policies = [record.get("denies") for record in records.values() if record["index_id"] == client.index]
        if len(policies) != 1:
            raise ValueError("Native workspace source policy is ambiguous")
        check_denies(paths, policies[0])
        return paths
