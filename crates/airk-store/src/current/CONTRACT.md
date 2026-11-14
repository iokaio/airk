# Current source admission and public catalog projection

Owner: Store. Live source admission excludes reserved `.airk` roots independently
of whether a permanent source policy is present. Persisted source definitions,
schema identities and digest inputs are unchanged.

Before exposing a current reader, authenticate and freeze its published primary
root and alias map. Reserved primary or alias roots refuse the whole reader,
including status/count metadata. Candidate checks use that immutable map without
SQL reentry. Ordinary published aliases retain their previous behavior.

Public typed catalog pages/details refuse historical reserved source/backing
roots and retained agent-edit backing data. Internal `item`/`optional_item`
readers remain available to native mutation and recovery. Refusal preserves the
stored rows; it does not sanitize, rewrite or fabricate an empty public result.

Regressions cover new admission, historical primary/alias publication, direct
candidates, public page/detail and the service's actual catalog listing caller.
Ordinary `.airk/data` backing assets remain supported. Windows passes do not
qualify Unix native aliases or other platform transports.
