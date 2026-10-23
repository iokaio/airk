// SPDX-License-Identifier: Apache-2.0
//! One durable, bounded launcher replacement. Callers hold installer and OS-owner locks.
use super::*;

const INTENT: &str = "launcher-repair.json";
const MAX_IMAGE: u64 = 2 * 1024 * 1024 * 1024;
const MAX_INTENT: u64 = 16 * 1024;

fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    schema: u32,
    root: String,
    runtime: String,
    previous: Option<String>,
    target: String,
    bytes: u64,
    nonce: airk_core::identity::Incarnation,
}
impl Intent {
    fn partial(&self, root: &Path) -> PathBuf {
        root.join(format!(".airk-launcher-{}.part", self.nonce))
    }
    fn read(root: &Path) -> Result<Self> {
        let value: Self = serde_json::from_slice(&read_bounded(&root.join(INTENT), MAX_INTENT)?)?;
        ensure!(
            value.schema == 1
                && value.root
                    == root
                        .canonicalize()?
                        .to_str()
                        .context("non-UTF-8 installation")?,
            "launcher repair ownership mismatch"
        );
        digest(&value.runtime)?;
        digest(&value.target)?;
        if let Some(previous) = &value.previous {
            digest(previous)?;
        }
        ensure!(
            value.bytes > 0 && value.bytes <= MAX_IMAGE,
            "invalid launcher repair byte allowance"
        );
        Ok(value)
    }
    fn inspect_destination(&self, root: &Path) -> Result<Option<String>> {
        let path = launcher_path(root);
        if !exists(&path)? {
            return Ok(None);
        }
        safe_file(&path)?;
        ensure!(
            fs::metadata(&path)?.len() <= MAX_IMAGE,
            "oversized launcher preserved"
        );
        let actual = runtime_pair::digest_file(&path)?;
        ensure!(
            Some(&actual) == self.previous.as_ref() || actual == self.target,
            "changed launcher preserved during repair"
        );
        Ok(Some(actual))
    }
    fn inspect_partial(&self, root: &Path) -> Result<bool> {
        let path = self.partial(root);
        if !exists(&path)? {
            return Ok(false);
        }
        safe_file(&path)?;
        ensure!(
            fs::metadata(&path)?.len() <= self.bytes,
            "oversized launcher partial preserved"
        );
        Ok(true)
    }
    fn inspect_receipt(&self, root: &Path) -> Result<()> {
        let known = receipt(root)?;
        ensure!(
            known == self.previous || known.as_deref() == Some(&self.target),
            "launcher receipt changed during repair"
        );
        Ok(())
    }
}

fn receipt(root: &Path) -> Result<Option<String>> {
    let path = root.join("launcher.json");
    if !exists(&path)? {
        return Ok(None);
    }
    let known: LauncherReceipt = serde_json::from_slice(&read_bounded(&path, 4096)?)?;
    ensure!(known.schema == 1, "foreign launcher receipt preserved");
    digest(&known.sha256)?;
    Ok(Some(known.sha256))
}

pub(super) fn require_settled(root: &Path) -> Result<()> {
    ensure!(
        !exists(&root.join(INTENT))?,
        "launcher repair is pending; resume repair-launcher or inspect/abort it before launching or activating"
    );
    Ok(())
}

pub(super) fn pending(root: &Path) -> Result<Option<String>> {
    if !exists(&root.join(INTENT))? {
        return Ok(None);
    }
    let intent = Intent::read(root)?;
    intent.inspect_receipt(root)?;
    intent.inspect_destination(root)?;
    intent.inspect_partial(root)?;
    Ok(Some(intent.runtime))
}

fn begin(root: &Path, runtime: &str) -> Result<Intent> {
    let (main, _) = verify_runtime(root, runtime)?;
    let target = runtime_pair::digest_file(&main)?;
    let bytes = fs::metadata(&main)?.len();
    ensure!(
        bytes > 0 && bytes <= MAX_IMAGE,
        "launcher image exceeds repair allowance"
    );
    let previous = receipt(root)?;
    let destination = launcher_path(root);
    if exists(&destination)? {
        safe_file(&destination)?;
        ensure!(
            fs::metadata(&destination)?.len() <= MAX_IMAGE,
            "oversized launcher preserved"
        );
        let actual = runtime_pair::digest_file(&destination)?;
        ensure!(
            Some(actual) == previous,
            "unknown or modified launcher preserved"
        );
    }
    let intent = Intent {
        schema: 1,
        root: root
            .canonicalize()?
            .to_str()
            .context("non-UTF-8 installation")?
            .into(),
        runtime: runtime.into(),
        previous,
        target,
        bytes,
        nonce: airk_kernel::service_broker::new_instance_id()?,
    };
    ensure!(
        !exists(&intent.partial(root))?,
        "launcher partial name is occupied"
    );
    // Publish ownership and the byte allowance before creating a partial file.
    replace_json(root, INTENT, &intent)?;
    Ok(intent)
}

fn copy(root: &Path, intent: &Intent, main: &Path) -> Result<()> {
    let partial = intent.partial(root);
    let exists = intent.inspect_partial(root)?;
    // The same intent may recopy its unfinished file. Growth is bounded by the
    // authenticated image length; neither source growth nor an extra probe is written.
    airk_store::capacity::probe(root)?.require_growth(intent.bytes)?;
    let mut source = File::open(main)?;
    let mut options = OpenOptions::new();
    options.write(true);
    if exists {
        options.truncate(true);
    } else {
        options.create_new(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o700);
    }
    let mut output = options.open(&partial)?;
    let mut remaining = intent.bytes;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    while remaining > 0 {
        let wanted = remaining.min(buffer.len() as u64) as usize;
        let read = source.read(&mut buffer[..wanted])?;
        ensure!(read > 0, "approved launcher image shrank while copying");
        output.write_all(&buffer[..read])?;
        hash.update(&buffer[..read]);
        remaining -= read as u64;
    }
    ensure!(
        source.read(&mut buffer[..1])? == 0 && format!("{:x}", hash.finalize()) == intent.target,
        "approved launcher image changed while copying"
    );
    output.sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        output.set_permissions(fs::Permissions::from_mode(0o700))?;
        output.sync_all()?;
    }
    Ok(())
}

fn finish(root: &Path, intent: &Intent) -> Result<()> {
    intent.inspect_receipt(root)?;
    ensure!(
        intent.inspect_destination(root)?.as_deref() == Some(&intent.target),
        "replacement launcher is not complete"
    );
    if intent.inspect_partial(root)? {
        fs::remove_file(intent.partial(root))?;
    }
    replace_json(
        root,
        "launcher.json",
        &LauncherReceipt {
            schema: 1,
            sha256: intent.target.clone(),
        },
    )?;
    fs::remove_file(root.join(INTENT))?;
    sync_directory(root)
}

/// Caller owns the installer lock, stopped-manager proof and all process leases.
pub(super) fn repair(root: &Path, runtime: &str) -> Result<()> {
    let intent = if exists(&root.join(INTENT))? {
        Intent::read(root)?
    } else {
        begin(root, runtime)?
    };
    ensure!(
        intent.runtime == runtime,
        "pending launcher repair targets another selected runtime; inspect or abort first"
    );
    intent.inspect_receipt(root)?;
    let (main, _) = verify_runtime(root, runtime)?;
    ensure!(
        runtime_pair::digest_file(&main)? == intent.target
            && fs::metadata(&main)?.len() == intent.bytes,
        "pending launcher repair no longer matches approved payload"
    );
    if intent.inspect_destination(root)?.as_deref() == Some(&intent.target) {
        return finish(root, &intent);
    }
    copy(root, &intent, &main)?;
    intent.inspect_receipt(root)?;
    intent.inspect_destination(root)?;
    let partial = intent.partial(root);
    safe_file(&partial)?;
    ensure!(
        runtime_pair::digest_file(&partial)? == intent.target,
        "launcher partial changed before replacement"
    );
    fs::rename(&partial, launcher_path(root)).context("launcher replacement deferred; stop owners and run repair from an external approved payload if Windows still has the image open")?;
    sync_directory(root)?;
    finish(root, &intent)
}

pub(super) fn abort(root: &Path) -> Result<()> {
    if !exists(&root.join(INTENT))? {
        return Ok(());
    }
    let intent = Intent::read(root)?;
    intent.inspect_receipt(root)?;
    let current = intent.inspect_destination(root)?;
    ensure!(
        current.as_deref() != Some(&intent.target) || current == intent.previous,
        "launcher replacement already completed; resume repair to finalize its receipt"
    );
    if intent.inspect_partial(root)? {
        fs::remove_file(intent.partial(root))?;
    }
    fs::remove_file(root.join(INTENT))?;
    sync_directory(root)
}

/// Settle only already-copied state; uninstall never needs to recopy an image.
pub(super) fn settle_for_uninstall(root: &Path) -> Result<()> {
    if !exists(&root.join(INTENT))? {
        return Ok(());
    }
    let intent = Intent::read(root)?;
    if intent.inspect_destination(root)?.as_deref() == Some(&intent.target) {
        finish(root, &intent)
    } else {
        abort(root)
    }
}

#[cfg(test)]
#[path = "runtime_launcher_repair_tests.rs"]
mod tests;
