# SPDX-License-Identifier: Apache-2.0
"""Loopback judging service: one loaded reranker shared by many sessions.

Evaluation tooling for the model-dose experiment of the Gather Context
strategy of 27 November 2026. Agent cost sessions run several at a time and
each starts its own one-tool server; a 4B model cannot be loaded once per
session. This process loads the model once and scores pairs for every
session over 127.0.0.1. It is not part of the product and its timing is not
a measure of anything: the experiment's result is the acting agent's cost.

The model is Qwen3-Reranker-4B in FP16 through PyTorch, the same judge and
prompt as ``arbiter_loop_prototype.py``; ``--bits 8`` rounds the linear
weights to 8 bits and expands them to FP16, the working model of
26 November 2026. ``--cheap`` loads no model and scores by shared words,
for tests of the plumbing.

    python -B arbiter_judge_service.py --model <checkpoint dir> --bits 8 --port 8765 --state <new file.json>

``POST /score`` takes ``{"instruction": str, "pairs": [[need, passage], ...]}``
and returns ``{"scores": [...]}``. ``GET /stats`` returns the judgments made,
the seconds spent scoring and the peak GPU memory. ``--state`` receives the
address once the model is loaded, which is how a caller knows it is ready.
"""

from __future__ import annotations

import argparse
import http.server
import json
import pathlib
import sys
import threading
import urllib.request

SCHEMA = "airk.private.arbiter-judge-service/1"
MAX_BODY = 32 * 1024 * 1024


def make_handler(judge, lock: threading.Lock, peak_memory):
    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *args) -> None:  # quiet: stats carry what matters
            return

        def reply(self, status: int, body: dict) -> None:
            data = json.dumps(body).encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def do_GET(self) -> None:
            if self.path != "/stats":
                return self.reply(404, {"error": "not found"})
            with lock:
                self.reply(200, {"schema": SCHEMA, "judgments": judge.pairs, "seconds": round(judge.seconds, 3),
                                 "clipped": judge.clipped, "peak_gpu_bytes": peak_memory()})

        def do_POST(self) -> None:
            if self.path != "/score":
                return self.reply(404, {"error": "not found"})
            length = int(self.headers.get("Content-Length") or 0)
            if not 0 < length <= MAX_BODY:
                return self.reply(400, {"error": "body size"})
            try:
                body = json.loads(self.rfile.read(length))
                instruction, pairs = body["instruction"], [(str(need), str(passage)) for need, passage in body["pairs"]]
            except (ValueError, KeyError, TypeError):
                return self.reply(400, {"error": "expected instruction and pairs"})
            with lock:
                scores = judge.score(instruction, pairs) if pairs else []
            self.reply(200, {"scores": scores})

    return Handler


def serve(judge, port: int, peak_memory=lambda: None) -> http.server.ThreadingHTTPServer:
    """Bind the loopback server for ``judge``; the caller runs ``serve_forever``."""
    return http.server.ThreadingHTTPServer(("127.0.0.1", port), make_handler(judge, threading.Lock(), peak_memory))


def score(address: str, instruction: str, pairs: list[tuple[str, str]], timeout: float = 600.0) -> list[float]:
    """Client side: score pairs at a running service."""
    request = urllib.request.Request(address + "/score", method="POST", headers={"Content-Type": "application/json"},
                                     data=json.dumps({"instruction": instruction, "pairs": pairs}).encode("utf-8"))
    with urllib.request.urlopen(request, timeout=timeout) as response:
        return json.loads(response.read())["scores"]


def stats(address: str, timeout: float = 30.0) -> dict:
    with urllib.request.urlopen(address + "/stats", timeout=timeout) as response:
        return json.loads(response.read())


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--model", type=pathlib.Path)
    parser.add_argument("--bits", type=int, default=0)
    parser.add_argument("--max-tokens", type=int, default=1024)
    parser.add_argument("--batch", type=int, default=8)
    parser.add_argument("--cheap", action="store_true")
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--state", type=pathlib.Path, required=True)
    args = parser.parse_args(argv)
    import arbiter_loop_prototype as prototype
    peak_memory = lambda: None  # noqa: E731
    if args.cheap:
        judge = prototype.CheapJudge()
    else:
        if args.model is None:
            parser.error("--model is needed unless --cheap")
        judge = prototype.Judge(args.model, args.max_tokens, args.batch, args.bits)
        peak_memory = lambda: int(judge.torch.cuda.max_memory_allocated())  # noqa: E731
    server = serve(judge, args.port, peak_memory)
    address = f"http://127.0.0.1:{server.server_address[1]}"
    args.state.write_text(json.dumps({"schema": SCHEMA, "address": address, "model": str(args.model) if args.model else None,
                                      "weight_bits": args.bits or 16, "cheap": args.cheap}, indent=2) + "\n",
                          encoding="utf-8", newline="\n")
    print(address, flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    return 0


if __name__ == "__main__":
    sys.exit(main())
