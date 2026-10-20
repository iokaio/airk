# Current-index cleanup

AIRK 1.2 retains one current index and durable pending work. Historical generations, retention policies, checkpoints and rollback are unsupported. Old 1.1 state remains isolated in its original namespace; it is not automatically migrated or deleted.

Publication attempts bounded cleanup after committing. Run `airk collections maintain` or the extension's **Reclaim Unreferenced Index Objects** command to repeat it. The result reports removed files/bytes and whether another pass is pending. Cleanup requires a complete authenticated ownership plan, no pending publication and exclusive current-request access. Corrupt ownership, links and active readers prevent deletion. A cleanup failure does not undo a successful publication; inspect `IndexReport.maintenance_error`.

See [current-index storage and recovery](current-index-runtime.md), [quota limits](storage-quotas.md) and [collection removal](collections.md#retention-and-removal). Source files and other collections are preserved.
