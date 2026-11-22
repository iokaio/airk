# SPDX-License-Identifier: Apache-2.0
"""Minimal MCP server that holds one tool call open while sending progress.

It answers ``initialize`` with tools capability only: no ``tasks`` capability
and no ``execution.taskSupport`` on the tool, so a client cannot turn the call
into an MCP task. Two transports:

- stdio (default): newline-delimited JSON-RPC 2.0;
- ``--http PORT``: MCP streamable HTTP on ``http://127.0.0.1:PORT/mcp``. A
  ``tools/call`` is answered as a ``text/event-stream`` carrying the progress
  notifications and then the result; other requests get one JSON reply.

Every message received and sent is logged with a UTC time and a monotonic
offset to ``--log`` (JSON lines), so a run shows exactly what the client did:
whether it waited, sent ``notifications/cancelled``, disconnected or retried.

Tool ``hold``: ``{"seconds": <number>}``. When the request carries
``_meta.progressToken``, a ``notifications/progress`` is sent every
``--progress-interval`` seconds. The reply is the text
``held <seconds> s; progress notifications sent: <n>``.
"""

from __future__ import annotations

import argparse
import json
import sys
import threading
import time
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Callable

VERSIONS = ("2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05")
Out = Callable[[dict], None]


class Server:
    def __init__(self, log_path: str, interval: float) -> None:
        self.log = open(log_path, "a", encoding="utf-8")
        self.lock = threading.Lock()
        self.interval = interval
        self.began = time.monotonic()
        self.cancelled: set = set()

    def record(self, direction: str, message: object) -> None:
        row = {"utc": datetime.now(timezone.utc).isoformat(), "t_s": round(time.monotonic() - self.began, 3),
               "dir": direction, "msg": message}
        with self.lock:
            self.log.write(json.dumps(row, separators=(",", ":")) + "\n")
            self.log.flush()

    def hold(self, msg_id: object, params: dict, out: Out) -> None:
        args = params.get("arguments") or {}
        seconds = float(args.get("seconds", 10))
        token = (params.get("_meta") or {}).get("progressToken")
        sent = 0
        end = time.monotonic() + seconds
        while True:
            left = end - time.monotonic()
            if left <= 0 or msg_id in self.cancelled:
                break
            time.sleep(min(self.interval, left))
            if token is not None and time.monotonic() < end and msg_id not in self.cancelled:
                sent += 1
                held = seconds - max(0.0, end - time.monotonic())
                out({"jsonrpc": "2.0", "method": "notifications/progress",
                     "params": {"progressToken": token, "progress": round(held, 1), "total": seconds,
                                "message": f"held {held:.0f} of {seconds:.0f} s"}})
        if msg_id in self.cancelled:
            self.record("note", {"cancelled_request": msg_id, "progress_sent": sent})
            return
        out({"jsonrpc": "2.0", "id": msg_id,
             "result": {"content": [{"type": "text", "text": f"held {seconds:g} s; progress notifications sent: {sent}"}],
                        "isError": False}})

    def reply(self, message: dict) -> dict | None:
        """The single reply to a request other than tools/call (None for a notification)."""
        method, msg_id, params = message.get("method"), message.get("id"), message.get("params") or {}
        if msg_id is None:
            if method == "notifications/cancelled":
                self.cancelled.add(params.get("requestId"))
            return None
        if method == "initialize":
            requested = params.get("protocolVersion")
            version = requested if requested in VERSIONS else VERSIONS[0]
            return {"jsonrpc": "2.0", "id": msg_id,
                    "result": {"protocolVersion": version, "capabilities": {"tools": {"listChanged": False}},
                               "serverInfo": {"name": "client-timeout-probe", "version": "1.0.0"}}}
        if method == "tools/list":
            return {"jsonrpc": "2.0", "id": msg_id, "result": {"tools": [{
                "name": "hold",
                "description": "Hold the call open for the given number of seconds, then reply.",
                "inputSchema": {"type": "object", "properties": {"seconds": {"type": "number"}},
                                "required": ["seconds"]}}]}}
        if method == "ping":
            return {"jsonrpc": "2.0", "id": msg_id, "result": {}}
        return {"jsonrpc": "2.0", "id": msg_id, "error": {"code": -32601, "message": f"method not found: {method}"}}


def is_call(message: dict) -> bool:
    return message.get("method") == "tools/call" and (message.get("params") or {}).get("name") == "hold" \
        and message.get("id") is not None


def serve_stdio(server: Server) -> None:
    def out(message: dict) -> None:
        with server.lock:
            sys.stdout.write(json.dumps(message, separators=(",", ":")) + "\n")
            sys.stdout.flush()
        server.record("out", message)

    for raw in sys.stdin:
        raw = raw.strip()
        if not raw:
            continue
        try:
            message = json.loads(raw)
        except json.JSONDecodeError:
            server.record("bad", raw[:500])
            continue
        server.record("in", message)
        if is_call(message):
            threading.Thread(target=server.hold, args=(message["id"], message.get("params") or {}, out),
                             daemon=True).start()
        elif (reply := server.reply(message)) is not None:
            out(reply)
    server.record("note", {"stdin_closed": True})


def serve_http(server: Server, port: int) -> None:
    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *_args) -> None:  # the JSON log is the record
            pass

        def _json(self, status: int, body: dict | None) -> None:
            data = b"" if body is None else json.dumps(body, separators=(",", ":")).encode()
            self.send_response(status)
            if body is not None:
                self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def do_GET(self) -> None:  # no server-initiated stream
            self.send_response(405)
            self.send_header("Content-Length", "0")
            self.end_headers()

        def do_DELETE(self) -> None:
            server.record("note", {"http_delete": True})
            self._json(200, None)

        def do_POST(self) -> None:
            raw = self.rfile.read(int(self.headers.get("Content-Length") or 0))
            try:
                message = json.loads(raw)
            except json.JSONDecodeError:
                server.record("bad", raw[:500].decode(errors="replace"))
                return self._json(400, None)
            server.record("in", message)
            if not is_call(message):
                reply = server.reply(message)
                if reply is not None:
                    server.record("out", reply)
                return self._json(200 if reply is not None else 202, reply)
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()

            def out(event: dict) -> None:
                try:
                    self.wfile.write(f"event: message\ndata: {json.dumps(event, separators=(',', ':'))}\n\n".encode())
                    self.wfile.flush()
                    server.record("out", event)
                except OSError as error:
                    server.record("note", {"stream_write_failed": str(error), "id": message.get("id")})
                    server.cancelled.add(message.get("id"))

            server.hold(message["id"], message.get("params") or {}, out)
            self.close_connection = True

    httpd = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    server.record("note", {"http_port": port})
    httpd.serve_forever()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--log", required=True, help="JSON-lines message log (appended)")
    parser.add_argument("--progress-interval", type=float, default=10.0)
    parser.add_argument("--http", type=int, metavar="PORT", help="serve streamable HTTP on 127.0.0.1:PORT/mcp")
    args = parser.parse_args()
    server = Server(args.log, args.progress_interval)
    server.record("note", {"started": True, "interval_s": args.progress_interval})
    if args.http:
        serve_http(server, args.http)
    else:
        serve_stdio(server)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
