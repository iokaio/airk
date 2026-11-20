# SPDX-License-Identifier: Apache-2.0
"""Survey current GitHub repo metadata for the sealed MultiLang review roster.

In the checked same-human shell, audit the private roster and a fresh output
leaf. Run this script with isolated Python and disabled bytecode, retaining
stdout only in a private, audited receipt. Each of the 50 public upstreams
gets one unauthenticated metadata GET. Redirects are surfaced, not followed.
"""

import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import re
import sys
from urllib.error import HTTPError, URLError
from urllib.request import HTTPRedirectHandler, Request, build_opener


ROSTER_SHA = "28abd5497d6446adda3915b6f0f97a3374d40db28310762ae2de9c8891ae150e"
REPO = re.compile(r"[a-zA-Z0-9][a-zA-Z0-9_.-]*/[a-zA-Z0-9][a-zA-Z0-9_.-]*")
METADATA_LIMIT = 256 << 10


class SurfaceRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def repo_name(metadata):
    name = metadata.get("full_name") if isinstance(metadata, dict) else None
    return name.lower() if isinstance(name, str) and REPO.fullmatch(name) else None


def query(repo):
    request = Request(
        f"https://api.github.com/repos/{repo}",
        headers={"User-Agent": "AIRK-public-source-metadata-audit", "Accept": "application/vnd.github+json",
                 "X-GitHub-Api-Version": "2022-11-28"},
    )
    try:
        with build_opener(SurfaceRedirect()).open(request, timeout=10) as response:
            raw = response.read(METADATA_LIMIT + 1)
            etag = response.headers.get("ETag")
        if not 0 < len(raw) <= METADATA_LIMIT:
            return {"requested_repo": repo, "status": "unbounded_metadata"}
        metadata = json.loads(raw)
    except HTTPError as error:
        return {"requested_repo": repo, "status": f"http_{error.code}"}
    except (URLError, TimeoutError, OSError, ValueError) as error:
        return {"requested_repo": repo, "status": type(error).__name__}

    if not isinstance(metadata, dict):
        return {"requested_repo": repo, "status": "unresolved_metadata_shape"}
    canonical = repo_name(metadata)
    fork, identity = metadata.get("fork"), metadata.get("id")
    if canonical is None or type(fork) is not bool or type(identity) is not int or identity <= 0:
        return {"requested_repo": repo, "status": "unresolved_metadata_shape"}
    license_row = metadata.get("license")
    spdx = license_row.get("spdx_id") if isinstance(license_row, dict) else None
    return {
        "requested_repo": repo,
        "status": "metadata_observed",
        "canonical_repo": canonical,
        "github_repository_id": identity,
        "fork": fork,
        "parent": repo_name(metadata.get("parent")),
        "source": repo_name(metadata.get("source")),
        "current_spdx_classifier": spdx if isinstance(spdx, str) else None,
        "response_sha256": hashlib.sha256(raw).hexdigest(),
        "response_etag": etag,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--roster", type=Path, required=True)
    path = parser.parse_args().roster
    if not (sys.flags.isolated and sys.dont_write_bytecode and path.is_absolute() and path.is_file()
            and path.resolve(strict=True) == path
            and not any(part.is_symlink() or part.is_junction() for part in (path, *path.parents))):
        parser.error("isolated Python and a canonical sealed private roster are required")
    with path.open("rb") as source:
        raw = source.read((1 << 20) + 1)
    if not 0 < len(raw) <= 1 << 20 or hashlib.sha256(raw).hexdigest() != ROSTER_SHA:
        parser.error("sealed review roster differs")
    roster = json.loads(raw)
    repos = [row["repo"] for row in roster["rows"]]
    if not (len(repos) == len(set(repos)) == 50 and all(REPO.fullmatch(repo) for repo in repos)):
        parser.error("expected 50 distinct exact upstream slugs")
    with ThreadPoolExecutor(max_workers=5) as executor:
        observed = list(executor.map(query, repos))
    resolved = [row for row in observed if row["status"] == "metadata_observed"]
    result = {
        "schema": "airk.private.repository-confirmation-upstream-metadata/1",
        "roster": {"path": str(path), "bytes": len(raw), "sha256": ROSTER_SHA},
        "request_count": len(observed),
        "status_counts": dict(sorted(Counter(row["status"] for row in observed).items())),
        "canonical_match_count": sum(row["requested_repo"] == row["canonical_repo"] for row in resolved),
        "distinct_github_ids": len({row["github_repository_id"] for row in resolved}),
        "observed_forks": sum(row["fork"] for row in resolved),
        "current_spdx_counts": dict(sorted(Counter(row["current_spdx_classifier"] or "unknown"
                                                   for row in resolved).items())),
        "rights_and_chronology": "current repository API only; revision-specific license, clones and issue dates pending",
        "admitted_cases": 0,
        "rows": observed,
    }
    print(json.dumps(result, sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    main()
