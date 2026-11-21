# SPDX-License-Identifier: Apache-2.0
"""Observe native file results and grant identities across a tiny authentic publisher control."""


def durable_scope(scope):
    """The public selection fields. Native fingerprints also bind the private service/lease."""
    return {field: value for field, value in scope.items() if field != "fingerprint"}


def collect(tool, label, sources, require):
    seen = []
    observations = []
    args = {"prefix": "", "limit": 1, "max_bytes": 65536, "response_mode": "full", "include_status": False}
    token = None
    for number in range(len(sources)):
        page = tool(f"{label}-page-{number}", "airk_list_files", args)
        require(len(page["paths"]) == 1 and page["paths"][0] not in seen, "missing or repeated native control page")
        seen.extend(page["paths"])
        observed_token = page["publication"]["token"]
        require(token is None or observed_token == token, "native control revision changed")
        token = observed_token
        observations.append(page["scope_observation"])
        cursor = page.get("cursor")
        require(bool(cursor) == page["truncated"] == (number + 1 < len(sources)), "invalid native control cursor")
        if cursor:
            args = {**args, "cursor": cursor}
    require(set(seen) == sources.keys(), "native control membership differs")
    for number, path in enumerate(seen):
        info = tool(f"{label}-info-{number}", "airk_file_info", {"path": path, "response_mode": "full"})
        require(info["content_hash"] == sources[path] and info["publication"]["token"] == token,
                "native control content or revision differs")
        observations.append(info["scope_observation"])
    require(all(durable_scope(s) == durable_scope(observations[0]) for s in observations),
            "public selection changed between control commands")
    return {"revision": token, "paths": seen, "scope_observations": observations}
