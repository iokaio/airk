# SPDX-License-Identifier: Apache-2.0
"""Independent offline checks for the prospective Python streaming task."""
import argparse
import json
from pathlib import Path
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("client_source", type=Path)
    args = parser.parse_args()
    sys.path.insert(0, str(args.client_source.resolve(strict=True)))
    from munarium_client._sse import SseParser, TurnEventMachine
    from munarium_client.models import TurnProgress, TurnResult

    mark = bytes.fromhex("efbbbf")
    later_mark = "\ufeff"
    body = f"event: notice\r\ndata: {later_mark}content\r\n\r\n".encode()
    named = [("notice", later_mark + "content")]
    cases = [
        ("one chunk", [mark + body], named),
        ("first split with empty pushes", [b"", mark[:1], b"", mark[1:] + body], named),
        ("second split with empty pushes", [mark[:2], b"", mark[2:], body], named),
        ("ordinary stream and split CRLF", [body[:14], b"", body[14:]], named),
        ("every byte separately", [bytes([x]) for x in mark + body], named),
        ("nonmatching opening bytes", [mark[:2], b"", b"event: hidden\r", b"\ndata: kept\r\n\r\n"],
         [("", "kept")]),
        ("consume only one opening mark", [mark, mark + body], [("", later_mark + "content")]),
        ("later event marker is ordinary", [b"data: first\n\n", mark + body],
         [("", "first"), ("", later_mark + "content")]),
    ]
    failed = []
    for name, chunks, expected in cases:
        instance = SseParser()
        observed = [(ev.event, ev.data) for chunk in chunks for ev in instance.push(chunk)]
        if observed != expected:
            failed.append({"case": name, "expected": expected, "observed": observed})
    done = {"session_id": "panel-7", "ordinal": 5, "collections_searched": [], "hits": [], "envelopes": []}
    progress = b'event: progress\r\ndata: {"stage":"merge","hits":2}\r\n\r\n'
    terminal = b"event: done\ndata: " + json.dumps(done).encode() + b"\n\n"
    machine = TurnEventMachine()
    items = [item for chunk in [mark[:1], b"", mark[1:2], mark[2:] + progress, terminal]
             for item in machine.feed(chunk)]
    if (len(items) != 2 or not isinstance(items[0], TurnProgress) or not isinstance(items[1], TurnResult)
            or items[1].session_id != "panel-7" or not machine.terminal or machine.error is not None):
        failed.append({"case": "typed progress then terminal", "observed": [type(item).__name__ for item in items]})
    print(json.dumps({"cases": len(cases) + 1, "failures": failed}, ensure_ascii=True))
    return int(bool(failed))


if __name__ == "__main__":
    raise SystemExit(main())
