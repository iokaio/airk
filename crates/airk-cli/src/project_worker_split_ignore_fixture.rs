// SPDX-License-Identifier: Apache-2.0
//! Exact native split tests own their ignore inputs. The parent environment and
//! production refusal for inaccessible ignore controls remain unchanged.
use anyhow::{Result, ensure};
use std::{
    fs,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub(super) fn run(name: &str) -> Result<bool> {
    let test = format!("project_worker_split::tests::{name}");
    if std::env::var("AIRK_SPLIT_OWNED_IGNORE_TEST")
        .ok()
        .as_deref()
        == Some(&test)
    {
        return Ok(false);
    }
    let owner = tempfile::Builder::new()
        .prefix("airk-split-ignore-")
        .tempdir()?;
    let root = owner.path().canonicalize()?;
    let ignore = root.join("fixture.ignore");
    let config = root.join("fixture.gitconfig");
    fs::write(&ignore, b"# Exact fixture has no global exclusions.\n")?;
    let ignore = ignore
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("fixture ignore path is not UTF-8"))?
        .replace('\\', "/");
    ensure!(
        !ignore.contains(['"', '\n', '\r']),
        "ambiguous fixture ignore locator"
    );
    fs::write(&config, format!("[core]\nexcludesFile = \"{ignore}\"\n"))?;
    let stdout = root.join("test.stdout");
    let stderr = root.join("test.stderr");
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["--exact", &test, "--nocapture"])
        .env("AIRK_SPLIT_OWNED_IGNORE_TEST", &test)
        .env("GIT_CONFIG_GLOBAL", &config)
        .stdin(Stdio::null())
        .stdout(fs::File::create(&stdout)?)
        .stderr(fs::File::create(&stderr)?);
    let mut child = crate::native_process::ContainedChild::spawn(&mut command)?;
    drop(command);
    let deadline = Instant::now() + Duration::from_secs(180);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        ensure!(
            Instant::now() < deadline,
            "owned split test exceeded 180 seconds"
        );
        std::thread::sleep(Duration::from_millis(25));
    };
    drop(child);
    ensure!(
        fs::metadata(&stdout)?.len() <= 16 << 20 && fs::metadata(&stderr)?.len() <= 16 << 20,
        "owned split diagnostics exceed limit"
    );
    ensure!(
        status.success(),
        "owned split test failed: {}\n{}",
        String::from_utf8_lossy(&fs::read(stdout)?),
        String::from_utf8_lossy(&fs::read(stderr)?)
    );
    Ok(true)
}
