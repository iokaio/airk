# SPDX-License-Identifier: Apache-2.0
"""Run disjoint checked Windows metadata batches within the same deadline."""

from concurrent.futures import ThreadPoolExecutor
import time

BATCH_SIZE = 128
WORKERS = 4


def audit_batches(paths, created, deadline, checked_batch):
    """Check every path, stopping after the first failed group."""
    if not paths:
        return
    group_size = WORKERS * BATCH_SIZE
    with ThreadPoolExecutor(max_workers=WORKERS) as executor:
        for start in range(0, len(paths), group_size):
            if time.monotonic() >= deadline:
                raise TimeoutError("original fresh-build audit deadline exhausted")
            pending = [
                executor.submit(checked_batch, paths[offset:offset + BATCH_SIZE], created, deadline)
                for offset in range(start, min(start + group_size, len(paths)), BATCH_SIZE)
            ]
            for result in pending:
                result.result()
