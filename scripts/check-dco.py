# SPDX-License-Identifier: Apache-2.0
"""Validate the human author/co-author DCO trailers of every commit in a PR."""
import argparse
import json
import re
import subprocess

IDENTITY = re.compile(r"([^<>\r\n]+) <([^<>\s]+@[^<>\s]+)>")


def identity(value):
    match = IDENTITY.fullmatch(value.strip())
    if not match:
        raise ValueError("Invalid contributor identity")
    return match[1].strip().casefold(), match[2].casefold()


def validate(commit):
    details = commit["commit"]
    author = details["author"]
    expected = {identity(f'{author["name"]} <{author["email"]}>')}
    message = details["message"].replace("\r\n", "\n").rstrip()
    # Trailers must occupy the final paragraph, not quoted text in the body.
    trailers = message.rsplit("\n\n", 1)[-1].splitlines()
    signed = set()
    for line in trailers:
        if line.casefold().startswith("signed-off-by:"):
            signed.add(identity(line.split(":", 1)[1]))
        elif line.casefold().startswith("co-authored-by:"):
            expected.add(identity(line.split(":", 1)[1]))
    if not expected.issubset(signed):
        raise ValueError("Missing matching author/co-author Signed-off-by trailer; the contributor must supply their own sign-off")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True)
    parser.add_argument("--pr", type=int, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r"[\w.-]+/[\w.-]+", args.repo) or args.pr <= 0:
        raise SystemExit("Invalid repository or PR number")
    # Use the API's declared count to reject its 250-commit response limit.
    pull = json.loads(subprocess.check_output(["gh", "api", f"repos/{args.repo}/pulls/{args.pr}"]))
    pages = json.loads(subprocess.check_output([
        "gh", "api", "--paginate", "--slurp", f"repos/{args.repo}/pulls/{args.pr}/commits?per_page=100"]))
    commits = [commit for page in pages for commit in page]
    if len(commits) != pull["commits"] or not commits:
        raise SystemExit("Cannot validate the complete PR commit inventory; split an oversized PR")
    failures = []
    for commit in commits:
        try:
            validate(commit)
        except (ValueError, KeyError) as error:
            failures.append(f'{commit["sha"][:12]}: {error}')
    if failures:
        raise SystemExit("\n".join(failures))
    print(f"DCO checked for all {len(commits)} PR commits.")


if __name__ == "__main__":
    main()
