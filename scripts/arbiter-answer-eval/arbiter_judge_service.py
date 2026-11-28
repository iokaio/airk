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
from uuid import uuid4

SCHEMA = "airk.private.arbiter-judge-service/1"
MAX_BODY = 32 * 1024 * 1024


def make_handler(judge, lock: threading.Lock, peak_memory, identity):
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
                                 "clipped": judge.clipped, "peak_gpu_bytes": peak_memory(), **identity})

        def do_POST(self) -> None:
            if self.path != "/score":
                return self.reply(404, {"error": "not found"})
            length = int(self.headers.get("Content-Length") or 0)
            if not 0 < length <= MAX_BODY:
                return self.reply(400, {"error": "body size"})
            try:
                body = json.loads(self.rfile.read(length))
                if identity and body.get("instance") != identity["instance"]:
                    return self.reply(409, {"error": "scorer instance changed or was not selected"})
                instruction, pairs = body["instruction"], [(str(need), str(passage)) for need, passage in body["pairs"]]
            except (ValueError, KeyError, TypeError):
                return self.reply(400, {"error": "expected instruction and pairs"})
            with lock:
                scores = judge.score(instruction, pairs) if pairs else []
            self.reply(200, {"scores": scores, **({"instance": identity["instance"]} if identity else {})})

    return Handler


def serve(judge, port: int, peak_memory=lambda: None, *, identity=None) -> http.server.ThreadingHTTPServer:
    """Bind the loopback server for ``judge``; the caller runs ``serve_forever``."""
    return http.server.ThreadingHTTPServer(("127.0.0.1", port), make_handler(judge, threading.Lock(), peak_memory, identity or {}))


def score(address: str, instruction: str, pairs: list[tuple[str, str]], timeout: float = 600.0, *, instance=None) -> list[float]:
    """Client side: score pairs at a running service."""
    body = {"instruction": instruction, "pairs": pairs}
    if instance is not None:
        body["instance"] = instance
    request = urllib.request.Request(address + "/score", method="POST", headers={"Content-Type": "application/json"},
                                     data=json.dumps(body).encode("utf-8"))
    with urllib.request.urlopen(request, timeout=timeout) as response:
        reply = json.loads(response.read())
    if instance is not None and reply.get("instance") != instance:
        raise ValueError("Scorer response belongs to a different process")
    return reply["scores"]


def stats(address: str, timeout: float = 30.0) -> dict:
    with urllib.request.urlopen(address + "/stats", timeout=timeout) as response:
        return json.loads(response.read())


def verified_client(state_path):
    """Bind each score request to the checkpoint and process verified at service startup."""
    from functools import partial
    from urllib.parse import urlsplit

    from qwen_replay_conformance import BLOBS, CENSUS, PREFIX_SHA, REVISION, SUFFIX_SHA

    state = json.loads(state_path.read_text(encoding="utf-8"))
    url = urlsplit(state["address"])
    if url.scheme != "http" or url.hostname != "127.0.0.1" or not url.port or url.path or url.username:
        raise ValueError("Expected a local scorer address")
    model = state.get("verified_model", {})
    expected = {"model": "Qwen/Qwen3-Reranker-4B", "revision": REVISION, "publisher_blobs": BLOBS,
                "weight_bits": 8, "compute_dtype": "float16", "weights": CENSUS, "max_tokens": 1024,
                "system_prefix_sha256": PREFIX_SHA, "assistant_suffix_sha256": SUFFIX_SHA}
    if state.get("cheap") is not False or not state.get("instance") or any(model.get(k) != v for k, v in expected.items()):
        raise ValueError("Scorer is missing its qualified single-Arbiter identity")
    current = stats(state["address"])
    if current.get("instance") != state["instance"] or current.get("verified_model") != model:
        raise ValueError("Scorer instance or loaded model changed")
    return partial(score, state["address"], instance=state["instance"])


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--model", type=pathlib.Path)
    parser.add_argument("--bits", type=int, default=0)
    parser.add_argument("--max-tokens", type=int, default=1024)
    parser.add_argument("--batch", type=int, default=8)
    parser.add_argument("--cheap", action="store_true")
    parser.add_argument("--verified-file-reranker", action="store_true")
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--state", type=pathlib.Path, required=True)
    args = parser.parse_args(argv)
    import arbiter_loop_prototype as prototype
    peak_memory = lambda: None  # noqa: E731
    identity = {}
    if args.verified_file_reranker:
        if args.cheap or args.model is None or (args.bits, args.batch, args.max_tokens) != (8, 8, 1024):
            parser.error("verified mode requires --model and --bits 8 with batch 8 and 1024 tokens")
        from qwen_replay_conformance import load_verified_judge
        judge, receipt = load_verified_judge(args.model)
        identity = {"instance": uuid4().hex, "verified_model": receipt}
        peak_memory = lambda: int(judge.torch.cuda.max_memory_allocated())  # noqa: E731
    elif args.cheap:
        judge = prototype.CheapJudge()
    else:
        if args.model is None:
            parser.error("--model is needed unless --cheap")
        judge = prototype.Judge(args.model, args.max_tokens, args.batch, args.bits)
        peak_memory = lambda: int(judge.torch.cuda.max_memory_allocated())  # noqa: E731
    server = serve(judge, args.port, peak_memory, identity=identity)
    address = f"http://127.0.0.1:{server.server_address[1]}"
    args.state.write_text(json.dumps({"schema": SCHEMA, "address": address, "model": str(args.model) if args.model else None,
                                      "weight_bits": args.bits or 16, "cheap": args.cheap, **identity}, indent=2) + "\n",
                          encoding="utf-8", newline="\n")
    print(address, flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    return 0


if __name__ == "__main__":
    sys.exit(main())
