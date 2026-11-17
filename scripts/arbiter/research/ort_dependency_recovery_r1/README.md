# Observed ORT dependency recovery

These compact records preserve exact observed dependency changes whose historical patch application was not reproduced by the strict diagnostic. They do not assert AIRK authorship, alter attribution or select build inputs. The original archive, member, base bytes and observed target are independently SHA-256 pinned in source-recovery.json.

Each deduplicated record stores only replaced byte ranges and inserted bytes. scripts/arbiter/analysis/upstream_ort/recovery.py authenticates the entire base, each deletion and the entire result, without fuzzy matching. Reproduce in memory with recover(base_member_bytes, json.loads(record_bytes)). Obtain the exact base member from the pinned archive. Neither an archive nor a full vendor source copy is included here.

Original upstream license, copyright and author notices from authenticated archives are retained under notices/ and linked per source. These notices continue to govern the corresponding upstream code and changes; this archive does not relicense them. Earlier upstream patches remain provenance evidence, not proof that a historical command ran.

Capture: python -S -B scripts/arbiter/analysis/upstream_ort/capture_recovery.py --receipt <diagnostic.json> --receipt-sha256 <SHA256>. This emits metadata on stdout only; guarded materialization is separate. Qualification and full-directory coverage are not claimed.
