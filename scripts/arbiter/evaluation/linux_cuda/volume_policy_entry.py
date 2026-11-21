# SPDX-License-Identifier: Apache-2.0
"""Start one shipping or packaging entry under the successor volume policy without changing its source."""
import argparse
from pathlib import Path
import runpy
import sys

import gnu_volume_policy

HERE = Path(__file__).resolve().parent
# Entries whose volume guard is shipping_build.monitored or the shipping_service observer.
ENTRIES = ("shipping_build.py", "shipping_archive.py", "shipping_service.py")


def split(argv):
    """Return the binding and the entry's own argument vector from `<policy options> -- <entry> <arguments>`."""
    if "--" not in argv:
        raise ValueError("expected: <volume policy options> -- <entry.py> <its arguments>")
    cut = argv.index("--")
    parser = argparse.ArgumentParser(description=__doc__)
    gnu_volume_policy.arguments(parser)
    binding = gnu_volume_policy.selected(parser.parse_args(argv[:cut]))
    entry = argv[cut + 1:]
    if binding is None or not entry or entry[0] not in ENTRIES:
        raise ValueError("a complete volume policy binding and one of " + ", ".join(ENTRIES) + " are required")
    return binding, [str(HERE / entry[0]), *entry[1:]]


if __name__ == "__main__":
    chosen, command = split(sys.argv[1:])
    gnu_volume_policy.activate(chosen)
    sys.argv = command
    runpy.run_path(command[0], run_name="__main__")
