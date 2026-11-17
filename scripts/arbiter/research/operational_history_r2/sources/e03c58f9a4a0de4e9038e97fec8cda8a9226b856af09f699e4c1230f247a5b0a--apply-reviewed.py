# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
from pathlib import Path
import tomllib

root = Path(__file__).resolve().parents[4]
here = Path(__file__).resolve().parent
proposal_bytes = (here / "review-proposal.json").read_bytes()
proposal = json.loads(proposal_bytes)
sha = lambda value: hashlib.sha256(value).hexdigest()
paths = ["Cargo.toml", "Cargo.lock", "supply-chain/source-policy.json",
         "supply-chain/config.toml", "supply-chain/imports.lock", "supply-chain/audits.toml"]
before = {p: (root / p).read_bytes() for p in paths}
locked = tomllib.loads(before["Cargo.lock"].decode())
actual = next(p for p in locked["package"] if p["name"] == "rustls")
assert {k: actual[k] for k in proposal["new_identity"]} == proposal["new_identity"]
policy = json.loads(before["supply-chain/source-policy.json"])
old = proposal["old_identity"]
assert sum(row == old for row in policy["packages"]) == 1
policy["packages"] = [proposal["new_identity"] if row == old else row for row in policy["packages"]]
(root / "supply-chain/source-policy.json").write_text(json.dumps(policy, indent=2) + "\n", encoding="utf-8", newline="\n")
config = tomllib.loads(before["supply-chain/config.toml"].decode())
assert len(config["exemptions"]["rustls"]) == 1
entry = config["exemptions"]["rustls"][0]
assert entry["version"] == "0.23.45"
original = '\n'.join(['[[exemptions.rustls]]', 'version = "0.23.45"',
                       'criteria = "safe-to-deploy"', 'notes = ' + json.dumps(entry["notes"])])
replacement = '\n'.join(['[[exemptions.rustls]]', 'version = "0.23.46"',
                          'criteria = "safe-to-deploy"', 'notes = ' + json.dumps(proposal["exception_note"])])
text = before["supply-chain/config.toml"].decode()
assert text.count(original) == 1
(root / "supply-chain/config.toml").write_text(text.replace(original, replacement), encoding="utf-8", newline="\n")
updated = tomllib.loads((root / "supply-chain/config.toml").read_text())
config["exemptions"]["rustls"][0] = {"version": "0.23.46", "criteria": "safe-to-deploy", "notes": proposal["exception_note"]}
assert updated == config
for name in ("Cargo.toml", "Cargo.lock", "supply-chain/imports.lock", "supply-chain/audits.toml"):
    assert (root / name).read_bytes() == before[name]
proposal["status"] = "REVIEWED_LIMITED_EXACT_VERSION_EXCEPTION"
proposal["accepted_proposal_sha256"] = sha(proposal_bytes)
proposal["accepted_cargo_lock_sha256"] = sha(before["Cargo.lock"])
proposal["review_decision"] = "Primary independently reviewed full proposal and critical changed source; accepted exact limited exception, not a full audit or advisory waiver."
public = root / "supply-chain/reviews/rustls-0.23.46-2026-11-07.json"
public.write_text(json.dumps(proposal, indent=2) + "\n", encoding="utf-8", newline="\n")
receipt = {"before": {p: sha(b) for p,b in before.items()}, "after": {p: sha((root/p).read_bytes()) for p in paths}, "public_review_sha256": sha(public.read_bytes()), "source_rows_replaced": 1,"exception_rows_replaced": 1,"other_rows_unchanged": True}
(here / "applied-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
print(json.dumps(receipt, indent=2))
