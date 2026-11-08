// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn native_help_from_tokio_worker_preserves_complete_parser_catalog() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .thread_stack_size(2 << 20)
        .enable_all()
        .build()?;
    let (catalog, guidance, command) = runtime.block_on(async {
        tokio::spawn(async {
            airk_mcp::help::install_cli_help(cli_help_catalog);
            // Exercise first-use initialization through the actual native help
            // entrypoint, on the same small stack as the project worker.
            let guidance = airk_mcp::help::call(json!({
                "topic":"workbench", "response_mode":"full"
            }));
            let command = airk_mcp::help::call(json!({
                "command":"runtime http-connection", "response_mode":"full"
            }));
            (cli_help_catalog(), guidance, command)
        })
        .await
    })?;
    let expected = on_cli_stack(build_cli_help_catalog);
    assert_eq!(catalog, expected, "all native commands retain exact help");
    assert!(!catalog.is_empty());
    assert_ne!(serde_json::to_value(guidance)?["isError"], true);
    assert_ne!(serde_json::to_value(command)?["isError"], true);
    assert_eq!(
        cli_help_catalog(),
        expected,
        "cached help remains identical"
    );
    Ok(())
}
