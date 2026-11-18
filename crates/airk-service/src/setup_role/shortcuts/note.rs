// SPDX-License-Identifier: Apache-2.0
//! Exact-byte ownership for the human-readable alias installation note.
use super::*;
use airk_ipc::private_record;
use airk_process::directory::OpenedDirectory;

const NOTE: &str = "airkctl-aliases.txt";
const OWNER: &str = "cli-alias-note-owner.json";

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Note {
    schema: String,
    install_id: Incarnation,
    target: PathBuf,
    alias: Option<String>,
    note_sha256: Sha256Digest,
}

fn body(home: &Path, target: &Path, id: Incarnation, alias: Option<&str>) -> Result<Vec<u8>> {
    let shortcut = match alias {
        Some(alias) => expected(home, target, id, alias)?
            .launcher_path
            .display()
            .to_string(),
        None => "none".into(),
    };
    let text = format!(
        "AIRK command-line client\nCanonical executable: {}\nSelected alias: {}\nShortcut: {}\nThe running native client remains airkctl.\nFor bare commands, add the executable/shortcut directory to your PATH if needed.\nNo shell profile was edited. Existing shell aliases/functions can shadow executable lookup.\n",
        target.display(),
        alias.unwrap_or("none"),
        shortcut
    );
    ensure!(text.len() <= LIMIT, "alias note exceeds bound");
    Ok(text.into_bytes())
}

fn saved(
    root: &OpenedDirectory,
    home: &Path,
    target: &Path,
    id: Incarnation,
) -> Result<Option<(Vec<u8>, Vec<u8>)>> {
    let Some(raw) = intent::read_owner(root, OWNER)? else {
        return Ok(None);
    };
    let note: Note = serde_json::from_slice(&raw)?;
    ensure!(
        note.schema == "airk-cli-alias-note/1" && note.install_id == id && note.target == target,
        "alias note ownership changed"
    );
    let expected = body(home, target, id, note.alias.as_deref())?;
    ensure!(
        files::digest_bytes(&expected) == note.note_sha256,
        "alias note hash changed"
    );
    Ok(Some((raw, expected)))
}

// All note/owner retirement paths use the platform's exact final-handle policy.
fn remove_exact(root: &OpenedDirectory, name: &str, expected: &[u8]) -> Result<()> {
    #[cfg(windows)]
    {
        super::windows::remove_exact(root, name, expected)
    }
    #[cfg(not(windows))]
    {
        private_record::remove_private_at(root, name, expected).map(|_| ())
    }
}

pub(in crate::setup_role) fn write_note(
    root: &OpenedDirectory,
    home: &Path,
    target: &Path,
    id: Incarnation,
    alias: Option<&str>,
) -> Result<()> {
    let text = body(home, target, id, alias)?;
    let old = saved(root, home, target, id)?;
    let actual = intent::read_owner(root, NOTE)?;
    if let Some((raw, old_body)) = old {
        if let Some(actual) = actual {
            ensure!(actual == old_body, "modified alias note preserved");
            if actual == text {
                return Ok(());
            }
            remove_exact(root, NOTE, &old_body)?;
        }
        remove_exact(root, OWNER, &raw)?;
    } else {
        ensure!(actual.is_none(), "unowned alias note preserved");
    }
    let note = Note {
        schema: "airk-cli-alias-note/1".into(),
        install_id: id,
        target: target.to_owned(),
        alias: alias.map(str::to_owned),
        note_sha256: files::digest_bytes(&text),
    };
    private_record::write_owner_at(root, OWNER, &serde_json::to_vec(&note)?, LIMIT, false)?;
    private_record::write_owner_at(root, NOTE, &text, LIMIT, false)
}

pub(in crate::setup_role) fn remove_note(
    root: &OpenedDirectory,
    home: &Path,
    target: &Path,
    id: Incarnation,
) -> Result<()> {
    if let Some((raw, text)) = saved(root, home, target, id)? {
        remove_exact(root, NOTE, &text)?;
        remove_exact(root, OWNER, &raw)?;
    }
    Ok(())
}
