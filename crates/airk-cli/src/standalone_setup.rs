// SPDX-License-Identifier: Apache-2.0
//! Standalone installation and uninstall through the native owners. The
//! interim setup enrollment (scope review, reviewed grant, connection and
//! client launch) was removed with the credential (local trust plan B):
//! agents connect with `airkctl agents connect`, which writes the endpoint
//! URL only.
use airk_core::identity::{Sha256Digest, framed_digest};
use anyhow::{Result, bail, ensure};
use serde_json::Value;
use std::{
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
};

pub trait SetupPrompt {
    fn interactive(&self) -> bool;
    fn message(&mut self, text: &str) -> Result<()>;
    fn answer(&mut self, text: &str) -> Result<String>;
}

pub struct Terminal;
impl SetupPrompt for Terminal {
    fn interactive(&self) -> bool {
        io::stdin().is_terminal()
    }
    fn message(&mut self, text: &str) -> Result<()> {
        writeln!(io::stderr().lock(), "{text}")?;
        Ok(())
    }
    fn answer(&mut self, text: &str) -> Result<String> {
        ensure!(
            self.interactive(),
            "interactive setup needs a terminal; supply --yes"
        );
        let mut output = io::stderr().lock();
        write!(output, "{text} ")?;
        output.flush()?;
        drop(output);
        let mut line = String::new();
        ensure!(
            io::stdin().read_line(&mut line)? > 0,
            "setup cancelled at end of input"
        );
        ensure!(line.len() <= 16 * 1024, "setup answer exceeds bound");
        Ok(line.trim().to_owned())
    }
}

fn confirm(prompt: &mut impl SetupPrompt, yes: bool, text: &str) -> Result<()> {
    prompt.message(text)?;
    ensure!(
        yes || prompt.answer("Continue? [y/N]")?.eq_ignore_ascii_case("y"),
        "setup cancelled; no requested change was submitted"
    );
    Ok(())
}

pub fn installation(explicit: Option<&Path>) -> Result<PathBuf> {
    explicit
        .map(Path::to_path_buf)
        .map_or_else(crate::runtime_install::default_install_root, Ok)
}

/// The exact instruction for agents that referenced a replaced installation.
pub fn reconnect_instruction(client_references: &Value, installed_cli: Option<&Path>) -> String {
    let command =
        installed_cli.map_or_else(|| "airk".to_owned(), |cli| format!("\"{}\"", cli.display()));
    format!(
        "Reconnect every coding agent that used the replaced installation: an agent entry that starts one of {} or connects to {} no longer works. Run airkctl agents connect --client claude (or codex or vscode) from the agent's working folder, or airkctl agents repair to rewrite existing entries; the installed {command} needs no agent setup of its own. In VS Code, use AIRK Explorer > Reconnect.",
        client_references["executables"],
        client_references["http_endpoint"]
            .as_str()
            .unwrap_or("no saved HTTPS endpoint"),
    )
}

/// The expected manifest digest comes from an explicitly trusted distributor or
/// user, never from hashing the same untrusted payload and trusting its result.
///
/// Before 1.9.0 an installed predecessor is replaced, never upgraded in place,
/// through the same boundary as `runtime replace-installation`. The returned
/// receipt carries its `client_references`.
pub async fn install_payload(
    install: &Path,
    source: &Path,
    approved: &str,
    yes: bool,
    rebuild: bool,
    prompt: &mut impl SetupPrompt,
) -> Result<Option<Value>> {
    approved.parse::<Sha256Digest>()?;
    ensure!(
        crate::runtime_pair::digest_file(&source.join(crate::install_files::main_name()))?
            == crate::runtime_pair::current_executable_sha256()?,
        "run setup using the reviewed payload's own AIRK executable before installing it"
    );
    let operation_id =
        match crate::runtime_install::pending_replacement_operation(install, approved)? {
            Some(operation) => operation,
            None => framed_digest(
                b"airk/standalone-reinstall/1",
                &[airk_kernel::service_broker::new_instance_id()?.as_bytes()],
            )
            .to_string(),
        };
    let (replace, plan) =
        crate::runtime_install::replacement_plan(install, approved, Some(&operation_id), None)?;
    let replaced = if replace {
        ensure!(
            !rebuild,
            "this version replaces the earlier AIRK installation instead of upgrading it; --rebuild-indexes does not apply"
        );
        confirm(
            prompt,
            yes,
            &format!(
                "AIRK {} replaces the earlier installation instead of upgrading it. These AIRK folders are removed, including their saved indexes; your source folders are not touched: {}. Agents that use these executables or this endpoint must be reconnected afterwards: {}",
                env!("CARGO_PKG_VERSION"),
                plan["locations"],
                plan["client_references"]
            ),
        )?;
        let receipt = crate::runtime_install::replace_installation(
            install,
            approved,
            Some(&operation_id),
            None,
        )?;
        if let Some(residue) = receipt["residue"]
            .as_array()
            .filter(|residue| !residue.is_empty())
        {
            prompt.message(&format!(
                "Replacement left residue to review: {}",
                Value::Array(residue.clone())
            ))?;
        }
        Some(receipt)
    } else {
        None
    };
    let prior = if install.join("selected.json").try_exists()? {
        Some(crate::runtime_install::selected(install)?.0)
    } else {
        None
    };
    if prior
        .as_ref()
        .is_some_and(|selection| selection.active == approved)
    {
        ensure!(
            !rebuild,
            "early rebuilding requires a distinct reviewed upgrade candidate"
        );
        crate::runtime_install::stage(
            source,
            install,
            approved,
            crate::runtime_install::VerificationBasis::TrustedDistribution,
        )?;
        return Ok(replaced);
    }
    confirm(
        prompt,
        yes,
        &format!(
            "Install the reviewed AIRK payload {approved} at {}? Source files and existing indexes stay under native ownership.",
            install.display()
        ),
    )?;
    if replaced.is_none() {
        // Consume fresh-install admission after confirmation and before staging.
        let receipt = crate::runtime_install::replace_installation(
            install,
            approved,
            Some(&operation_id),
            None,
        )?;
        if receipt["plan"]["reason"] == "installation_already_replaced" {
            return Ok(None);
        }
    }
    let rebuild = if prior.is_some() && !yes && !rebuild {
        match prompt.answer("Upgrade: [1] migrate existing indexes, [2] rebuild from saved sources (previous state preserved). Choice [1]:")?.as_str() {
            ""|"1"=>false,"2"=>true,_=>bail!("select migration or rebuilding; no runtime was staged"),
        }
    } else {
        rebuild
    };
    ensure!(
        !rebuild || prior.is_some(),
        "rebuilding requires an installed predecessor"
    );
    crate::runtime_install::stage(
        source,
        install,
        approved,
        crate::runtime_install::VerificationBasis::TrustedDistribution,
    )?;
    match prior {
        Some(prior) if rebuild => {
            crate::runtime_install::activate_rebuilding_reviewed(install, approved, &prior.active)
                .await?;
        }
        Some(prior) => {
            crate::runtime_install::activate_reviewed(install, approved, &prior.active).await?;
        }
        None => {
            crate::runtime_install::activate(install, approved).await?;
            // A fresh installation starts at login and survives a reboot. A
            // refusal (OS policy, unqualified adapter) keeps the on-demand
            // service and is reported; it never fails the installation.
            match crate::runtime_commands::install_login_startup(install).await {
                Ok(_) => prompt.message("AIRK starts automatically when you sign in.")?,
                Err(error) => prompt.message(&format!(
                    "AIRK could not register login startup, so the service starts on demand: {error:#}"
                ))?,
            }
        }
    }
    Ok(replaced)
}

pub async fn uninstall(
    install: Option<&Path>,
    yes: bool,
    prompt: &mut impl SetupPrompt,
) -> Result<()> {
    let install = installation(install)?;
    confirm(
        prompt,
        yes,
        &format!(
            "Uninstall AIRK from {}? Native uninstall stops owned processes and removes owned runtime files; indexed source files and index data are preserved.",
            install.display()
        ),
    )?;
    let receipt = crate::runtime_install::uninstall_receipt(&install).await?;
    serde_json::to_writer_pretty(io::stdout().lock(), &receipt)?;
    writeln!(io::stdout().lock())?;
    require_completed_uninstall(&receipt)
}

fn require_completed_uninstall(receipt: &Value) -> Result<()> {
    ensure!(
        receipt.get("uninstalled").and_then(Value::as_bool) == Some(true),
        "native uninstall is incomplete; preserve the installer and its uninstall entry, then resume native uninstall using the complete receipt above"
    );
    Ok(())
}

#[cfg(test)]
mod tests;
