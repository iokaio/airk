# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib

root = Path(__file__).resolve().parents[2]
name = 'crates/airk-cli/tests/mcp.rs'
original = (root / name).read_text(encoding='utf-8')
start = original.index('fn saved_source_verification_preserves_complete_results_through_cli_and_public_mcp()')
end = original.index('\n#[test]', start)
at = original.rfind('    Ok(())\n}', start, end)
assert at > start
extra = '''    // The actual CLI and public MCP paths share the same explicit terse
    // capability projection. Full mode retains the canonical Workbench schemas.
    for (ordinal, mode) in ["terse", "full"].into_iter().enumerate() {
        let args = json!({"adapter":"markdown","response_mode":mode,"include_status":true});
        let wire = session.read_retry(9000 + ordinal as u64, "capabilities", args.clone())?;
        anyhow::ensure!(wire["result"]["isError"] != true, "capabilities: {wire}");
        let mut native = payload(&wire["result"])?;
        let tools = common::call(root.path(), state.path(), "project-a", &[
            "tools", "call", "airk_capabilities", "--args", &serde_json::to_string(&args)?
        ])?;
        let mut cli = payload(&tools)?;
        if mode == "terse" {
            assert!(native["workbench"].get("operations").is_none());
            assert_eq!(native["workbench"]["operation_schemas"]["included"], false);
            assert!(native["workbench"]["operation_names"].as_array().is_some_and(|names| !names.is_empty()));
        } else {
            assert_eq!(native["workbench"], airk_kernel::workbench::operation_catalog());
        }
        stable(&mut native);
        stable(&mut cli);
        assert_eq!(cli, native, "complete {mode} capabilities differ by transport");
    }
'''
changed = original[:at] + extra + original[at:]
(Path(__file__).parent / 'capabilities-mcp-test.proposed.rs').write_text(changed, encoding='utf-8', newline='\n')
patch = ''.join(difflib.unified_diff(original.splitlines(keepends=True), changed.splitlines(keepends=True), fromfile='a/'+name, tofile='b/'+name))
(Path(__file__).parent / 'capabilities-transport.patch').write_text(patch, encoding='utf-8', newline='\n')
print('Prepared capabilities-transport.patch')
