# SPDX-License-Identifier: Apache-2.0
"""Phase 4 (split plan §5, U93, U94): remove the user command tree from `airk`.

Edits `crates/airk-cli/src/main.rs` once, asserting each anchor:

* `Command` keeps only the interim variants U93 keeps behind the private-role
  selector for their Phase 7 or developer owners: `DumpVocab`, `Runtime`,
  `ProjectWorker`, `DerivedWorker`, `SourceWorker` (process fixtures only),
  `Setup`, `Uninstall`, `Host` and `Service` (`Legacy` only). Every other
  variant has an `airkctl` owner or is retired (`docs/split/inventory/
  commands.md`, `inventory/deletion-review.md`).
* `dispatch` keeps the arms of those variants; the read, tools, combined and
  project paths after them are removed.
* `run` routes §5.1: a private role, the interim selector
  (`__airk_role setup-interim <pre-split arguments>`), or `manual_start`.
* The binary modules `tools` and `session_commands` are no longer declared.

Functions left without a caller are then removed by hand with the compiler's
dead-code report as evidence (CLAUDE.md "Delete only with evidence").

    python scripts/dev/split_p4_service_only.py
"""
from pathlib import Path
import re
import sys

ROOT = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path(__file__).resolve().parents[2]
MAIN = ROOT / "crates" / "airk-cli" / "src" / "main.rs"
KEEP = {"DumpVocab", "Runtime", "ProjectWorker", "DerivedWorker", "SourceWorker",
        "Setup", "Uninstall", "Host", "Service"}


def enum_body(text: str, name: str) -> tuple[int, int]:
    start = text.index(f"enum {name} {{\n") + len(f"enum {name} {{\n")
    end = text.index("\n}\n", start) + 1
    return start, end


def variants(body: str) -> list[tuple[str, str]]:
    """Split an enum body into (name, block) with each block's docs and attributes."""
    blocks, current = [], []
    depth = 0
    for line in body.splitlines(keepends=True):
        current.append(line)
        depth += line.count("{") - line.count("}")
        stripped = line.strip()
        if depth == 0 and (stripped.endswith("},") or re.fullmatch(r"[A-Z]\w*,", stripped)):
            block = "".join(current)
            name = re.search(r"^    ([A-Z]\w*)\b", block, re.M).group(1)
            blocks.append((name, block))
            current = []
    assert not "".join(current).strip(), "unparsed enum tail"
    return blocks


DISPATCH = """/// The pre-split operations U93 keeps reachable behind the interim selector
/// until their Phase 7 setup-role or developer owners land.
async fn dispatch(cli: Cli, explicit_root: bool, io: &CliIo) -> Result<()> {
    cli.validate_setup()?;
    match &cli.command {
        #[cfg(feature = "parser-training")]
        Command::DumpVocab { output } => parser_training::dump(
            &cli.root,
            cli.data_dir
                .as_deref()
                .context("dump-vocab requires explicit --data-dir outside the source")?,
            output,
        ),
        Command::Runtime { action } => airk_cli::runtime_install::run_with(action, io).await,
        Command::Setup {
            client,
            connection_id,
            existing_connection_id,
            explorer_descriptor,
            standalone,
        } => {
            run_setup(
                &cli,
                *client,
                *connection_id,
                *existing_connection_id,
                explorer_descriptor.as_deref(),
                standalone,
                explicit_root,
                io,
            )
            .await
        }
        Command::Uninstall { yes } => {
            airk_cli::standalone_setup::uninstall(
                cli.service_install_dir.as_deref(),
                *yes,
                &mut airk_cli::standalone_setup::Terminal,
            )
            .await
        }
        Command::ProjectWorker => airk_cli::run_project_worker().await,
        Command::DerivedWorker => airk_cli::derived_worker::run(),
        Command::SourceWorker => airk_cli::source_worker::run(),
        Command::Host => {
            airk_cli::runtime_install::run(&airk_cli::runtime_install::RuntimeCommand::Foreground {
                install_dir: install_root(&cli)?,
            })
            .await
        }
        Command::Service {
            action: ServiceCommand::Legacy { action },
        } => airk_cli::legacy_startup::run_with(action, io),
    }
}
"""


def main() -> None:
    text = MAIN.read_text(encoding="utf-8")
    assert "mod session_commands;" in text, "already applied"
    start, end = enum_body(text, "Command")
    kept = [block for name, block in variants(text[start:end]) if name in KEEP]
    assert {re.search(r"^    ([A-Z]\w*)\b", b, re.M).group(1) for b in kept} == KEEP
    text = text[:start] + "".join(kept) + text[end:]

    start, end = enum_body(text, "ServiceCommand")
    legacy = [block for name, block in variants(text[start:end]) if name == "Legacy"]
    assert len(legacy) == 1
    text = text[:start] + legacy[0] + text[end:]

    text = text.replace("mod session_commands;\nmod tools;\n", "", 1)

    # dispatch: replaced by the interim arms (U93); the read, tools, combined
    # and project paths have `airkctl` owners.
    head = text.index("async fn dispatch(mut cli: Cli, explicit_root: bool, executable: &Path, io: &CliIo) -> Result<()> {")
    end = text.index("\n}\n", head) + 3
    text = text[:head] + DISPATCH + text[end:]
    MAIN.write_bytes(text.encode("utf-8"))
    print("main.rs: Command keeps", sorted(KEEP))


if __name__ == "__main__":
    main()
