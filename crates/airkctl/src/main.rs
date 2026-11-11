// SPDX-License-Identifier: Apache-2.0
//! `airkctl`: the AIRK command-line client (split plan §6).
//!
//! This build serves `--version`, `version` and `tools list|help`; every other
//! command answers the typed `not_yet_served` refusal (exit 5) until its handler
//! lands.
use airk_core::service::{ControlMethod, ControlResponse, ToolsCatalogDetail};
use airkctl::contract::{
    ErrorLine, ExitClass, I2_CONTRACT, I2_CONTRACTS_SUPPORTED, Installation, InstallationState,
    PRODUCT, VERSION_SCHEMA, VersionDocument,
};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy, PartialEq, Eq)]
enum Output {
    Text,
    Json,
}

fn own_digest() -> Option<String> {
    let bytes = std::fs::read(std::env::current_exe().ok()?).ok()?;
    let digest = Sha256::digest(bytes);
    Some(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn version_document() -> VersionDocument {
    VersionDocument {
        schema: VERSION_SCHEMA.into(),
        product: PRODUCT.into(),
        version: VERSION.into(),
        // The install pointer and record (I4) arrive in step C6; until then no
        // installation can be verified, so none is reported.
        channel: None,
        i2_contract: I2_CONTRACT,
        i2_contracts_supported: I2_CONTRACTS_SUPPORTED.to_vec(),
        i1_protocol: airk_core::service::PROTOCOL_VERSION,
        capabilities: Vec::new(),
        executable_sha256: own_digest(),
        installation: Installation {
            state: InstallationState::NotFound,
            airk_version: None,
            pair: None,
            install_id: None,
        },
    }
}

fn fail(output: Output, line: &ErrorLine) -> ExitCode {
    let mut stderr = std::io::stderr().lock();
    let _ = match output {
        Output::Json => writeln!(
            stderr,
            "{}",
            serde_json::to_string(line).unwrap_or_else(|_| "{}".into())
        ),
        Output::Text => writeln!(stderr, "airkctl: {}", line.message),
    };
    ExitCode::from(line.class.code())
}

fn usage(output: Output, message: &str) -> ExitCode {
    fail(
        output,
        &ErrorLine::new(ExitClass::Usage, "usage", false, message),
    )
}

fn tools(output: Output, arguments: &[&str]) -> ExitCode {
    let Some((&command, rest)) = arguments.split_first() else {
        return usage(output, "tools takes list or help <tool>");
    };
    if !matches!(command, "list" | "help") {
        return fail(
            output,
            &ErrorLine::new(
                ExitClass::UnsupportedCapability,
                "not_yet_served",
                false,
                "This command is not served by this AIRK Service version.",
            ),
        );
    }
    let parsed = match airkctl::tools::arguments(rest) {
        Ok(parsed) => parsed,
        Err(message) => return usage(output, message),
    };
    let detail = match (command, parsed.positionals.as_slice()) {
        ("list", []) => ToolsCatalogDetail::List {
            schemas: parsed.json || output == Output::Json,
        },
        ("help", [tool]) => ToolsCatalogDetail::Help {
            tool: (*tool).to_owned(),
        },
        ("help", _) => return usage(output, "tools help takes one tool name"),
        _ => return usage(output, "tools list takes no arguments"),
    };
    let Some(runtime_dir) = parsed.runtime_dir else {
        return fail(
            output,
            &ErrorLine::new(
                ExitClass::ServiceMissing,
                "installation_not_found",
                false,
                "No AIRK installation was found; pass --runtime-dir <service control directory>.",
            ),
        );
    };
    let help = match &detail {
        ToolsCatalogDetail::Help { tool } => Some(tool.clone()),
        ToolsCatalogDetail::List { .. } => None,
    };
    let response = match airkctl::service::request(
        &runtime_dir,
        ControlMethod::ToolsCatalog { detail },
        VERSION,
    ) {
        Ok(response) => response,
        Err(line) => return fail(output, &line),
    };
    let document = match response {
        ControlResponse::ToolsCatalog { document } => document,
        ControlResponse::Failure { code } if code == "unknown_tool" => {
            let name = help.unwrap_or_default();
            let mut line = airkctl::service::refusal(&ControlResponse::Failure { code });
            line.message = format!("Unknown MCP tool '{name}'. Use 'airkctl tools list'.");
            return fail(output, &line);
        }
        other => return fail(output, &airkctl::service::refusal(&other)),
    };
    if parsed.json || output == Output::Json {
        println!("{document}");
        return ExitCode::SUCCESS;
    }
    let text = serde_json::from_str::<serde_json::Value>(&document)
        .ok()
        .and_then(|value| match &help {
            Some(name) => airkctl::tools::help_text(&value, name),
            None => airkctl::tools::list_text(&value),
        });
    match text {
        Some(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        None => fail(
            output,
            &ErrorLine::new(
                ExitClass::Internal,
                "malformed_response",
                false,
                "The AIRK service returned a malformed tool catalog.",
            ),
        ),
    }
}

fn run(arguments: Vec<String>) -> ExitCode {
    let mut output = Output::Text;
    let mut rest = Vec::new();
    let mut iterator = arguments.into_iter();
    while let Some(argument) = iterator.next() {
        if argument == "--output" {
            output = match iterator.next().as_deref() {
                Some("json") => Output::Json,
                Some("text") => Output::Text,
                _ => return usage(output, "--output takes json or text"),
            };
        } else if let Some(value) = argument.strip_prefix("--output=") {
            output = match value {
                "json" => Output::Json,
                "text" => Output::Text,
                _ => return usage(output, "--output takes json or text"),
            };
        } else {
            rest.push(argument);
        }
    }
    match rest
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["--version"] | ["-V"] => {
            println!("airkctl {VERSION}");
            ExitCode::SUCCESS
        }
        ["version"] => {
            let document = version_document();
            match output {
                Output::Json => match serde_json::to_string(&document) {
                    Ok(text) => println!("{text}"),
                    Err(_) => {
                        return fail(
                            output,
                            &ErrorLine::new(
                                ExitClass::Internal,
                                "internal",
                                false,
                                "version document",
                            ),
                        );
                    }
                },
                Output::Text => {
                    println!("airkctl {VERSION} ({PRODUCT})");
                    println!(
                        "I2 contract {I2_CONTRACT}; I1 protocol {}",
                        document.i1_protocol
                    );
                    println!("installation: not found");
                }
            }
            ExitCode::SUCCESS
        }
        ["tools", rest @ ..] => tools(output, rest),
        [] => usage(output, "a command is required; run `airkctl version`"),
        _ => fail(
            output,
            &ErrorLine::new(
                ExitClass::UnsupportedCapability,
                "not_yet_served",
                false,
                "This command is not served by this AIRK Service version.",
            ),
        ),
    }
}

fn main() -> ExitCode {
    run(std::env::args().skip(1).collect())
}
