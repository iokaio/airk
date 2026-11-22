# Client timeout probe

Evaluation tooling for final plan section 12.4 and 12.4.1 (step L2): it measures
what an MCP client does with a long tool call from a server that sends progress
notifications and does **not** support MCP tasks. It is never product code.

## Files

- `probe_server.py`: a minimal newline-delimited JSON-RPC MCP stdio server. Its
  one tool, `hold {"seconds": n}`, holds the call open for `n` seconds and sends
  `notifications/progress` every `--progress-interval` seconds when the request
  carries `_meta.progressToken`. It declares only the tools capability, so a
  client cannot turn the call into a task. Every message in and out is logged
  with a UTC time and a monotonic offset to `--log`.
- `probe_run.py`: runs headless Claude Code once against the server, with the
  evaluation harness's binary (`common.find_claude`) and child environment
  (`common.clean_child_env`), and writes a receipt folder.

## Use

```console
python scripts/arbiter-answer-eval/client-timeout-probe/probe_run.py --seconds 150 --out <new folder>
python scripts/arbiter-answer-eval/client-timeout-probe/probe_run.py --seconds 330 --progress-interval 100000 --out <new folder>
python scripts/arbiter-answer-eval/client-timeout-probe/probe_run.py --http 39521 --seconds 330 --out <new folder>
```

Options: `--model` (default the harness's agent model), `--effort` (default
`low`), `--progress-interval` (default 10 s), `--session-timeout`, `--claude`,
`--http PORT` (streamable HTTP on a local port instead of stdio, as AIRK's
HTTPS agent connection is) and `--env NAME=VALUE` for client settings such as
`MCP_TOOL_TIMEOUT`.

## Outputs

The `--out` folder (it must not exist) receives `mcp-config.json`,
`command.json` (command, prompt, Claude Code version, timeout-related
environment), `stream.jsonl` and `stderr.txt` from Claude Code, `server.jsonl`
(the server's message log) and `summary.json`: whether the call was answered,
cancelled or timed out, how many progress notifications were sent, the tool
result the agent saw and the session cost. Keep receipts in the campaign's
private folder, never in the repository.

## Results (22 November 2026, Claude Code 2.1.330, Haiku at low effort)

| Transport | Hold | Progress | Outcome |
| --- | --- | --- | --- |
| stdio | 150 s | every 10 s | waited; result delivered to the agent at 151.6 s |
| stdio | 330 s | every 10 s | waited; result delivered at 331.5 s |
| stdio | 330 s | none | waited; result delivered at 331.5 s |
| stdio | 610 s | every 10 s | waited; result delivered at 611.4 s |
| HTTP | 330 s | every 10 s | waited; result delivered at 333.2 s |
| HTTP | 330 s | none | aborted at 300 s; the agent saw an error result |

In every run Claude Code sent a progress token and no `task` parameter, sent
no cancellation and did not move the call to the background. The HTTP abort
is the documented idle timeout (`CLAUDE_CODE_MCP_TOOL_IDLE_TIMEOUT`, 5 minutes
on HTTP); the agent saw: `MCP server "probe" tool "hold" sent no response or
progress for 300s; aborting. ...`. Progress resets it. Receipts: `answer-eval-r1/answer-loop/L2/probe/`.
