// SPDX-License-Identifier: Apache-2.0
//! Pure setup decisions (split plan §9.1–§9.6; `docs/split/contracts.md`
//! §5.5–§5.6): which actions an operation runs, which of them its receipt
//! already satisfied, whether a receipt admits a resume, and the location
//! rules (U39, U50). No filesystem, process or clock access. Tested against
//! `fixtures/split/i4/interruptions.json`.
use airk_core::{
    identity::Sha256Digest,
    install_record::{
        IndexDeletion, Locations, OperationKind, RECORD_FILE, SetupOperation, SetupOptions,
        SetupPhase, SetupReceipt,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};

/// The payload inventory file in the program folder (and, for `--prepare`,
/// beside the operation file).
pub(crate) const INVENTORY_FILE: &str = "payload-inventory.json";
/// The Windows installer's ownership receipt of its user `PATH` entry.
const PATH_RECEIPT: &str = "user-path-owner.txt";
/// The macOS and Linux installers' ownership receipt of the `airkctl` link in
/// `~/.local/bin` (§9.5), written beside the record after setup completes:
/// `airk-airkctl-link/1`, the link path and its target, one per line.
pub(crate) const LINK_RECEIPT: &str = "airkctl-link-owner.txt";
const LINK_RECEIPT_SCHEMA: &str = "airk-airkctl-link/1";

/// The platform whose adapters an action uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Host {
    Windows,
    Macos,
    Linux,
}

impl Host {
    pub(crate) fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else {
            Self::Linux
        }
    }
}

/// `Prepare` runs only the steps that must precede file placement (the
/// installer then places the files and runs `Complete` with the same
/// operation); `Complete` runs every remaining step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Mode {
    Prepare,
    Complete,
}

/// One setup action. Each records one or more `SetupPhase` steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Action {
    Lock,
    RetirePredecessors,
    Drain,
    VerifyPlacement,
    WriteRecord,
    WritePointer,
    RegisterLogin,
    Start,
    ApplyTrust,
    UnregisterLogin,
    RemoveTrust,
    RemovePointer,
    RemoveRecord,
    PurgeData,
    Close,
}

/// Uninstall and purge remove an installation; every other kind places one.
pub(crate) fn removes(kind: OperationKind) -> bool {
    matches!(kind, OperationKind::Uninstall | OperationKind::Purge)
}

/// Index data is deleted only by the `purge` kind or an explicit
/// `delete_indexes: purge` (U32). `ask` reaches setup unresolved only from an
/// unattended caller and keeps the data.
pub(crate) fn purges(kind: OperationKind, options: &SetupOptions) -> bool {
    kind == OperationKind::Purge || options.delete_indexes == IndexDeletion::Purge
}

/// The complete ordered action list of an operation.
///
/// Placing kinds start the service before applying trust: the HTTPS issuer
/// that trust installs is created by the service on its first start, so the
/// receipt records `started`/`healthy` before the trust phase (a recorded
/// deviation from the §5.6 table order; each phase is still separate).
pub(crate) fn actions(
    kind: OperationKind,
    options: &SetupOptions,
    mode: Mode,
) -> Result<Vec<Action>, &'static str> {
    use Action::*;
    if removes(kind) {
        if mode == Mode::Prepare {
            return Err("setup_prepare_not_applicable");
        }
        let mut list = vec![
            Lock,
            Drain,
            UnregisterLogin,
            RemoveTrust,
            RemovePointer,
            RemoveRecord,
        ];
        if purges(kind, options) {
            list.push(PurgeData);
        }
        list.push(Close);
        return Ok(list);
    }
    let mut list = vec![Lock, RetirePredecessors, Drain];
    if mode == Mode::Prepare {
        return Ok(list);
    }
    list.extend([VerifyPlacement, WriteRecord, WritePointer]);
    if options.login_startup {
        list.push(RegisterLogin);
    }
    if options.start {
        list.push(Start);
    }
    list.extend([ApplyTrust, Close]);
    Ok(list)
}

/// Whether the recorded phases already satisfy `action`. The lock is taken
/// on every run. A drain is re-observed until the step after it is durable
/// (contracts §5.6: "resume re-observes").
pub(crate) fn satisfied(action: Action, kind: OperationKind, recorded: &[SetupPhase]) -> bool {
    use SetupPhase as P;
    let has = |phase: P| recorded.contains(&phase);
    match action {
        Action::Lock => false,
        Action::RetirePredecessors => has(P::PredecessorsRetired),
        Action::Drain if removes(kind) => has(P::LoginUnregistered),
        Action::Drain => has(P::Placed),
        Action::VerifyPlacement => has(P::Placed),
        Action::WriteRecord => has(P::RecordWritten),
        Action::WritePointer => has(P::PointerWritten),
        Action::RegisterLogin => {
            has(P::LoginRegistered) || has(P::LoginFailed) || has(P::LoginUnsupported)
        }
        Action::Start => has(P::Healthy),
        Action::ApplyTrust => {
            has(P::TrustApplied) || has(P::TrustDeclined) || has(P::TrustUnsupported)
        }
        Action::UnregisterLogin => has(P::LoginUnregistered),
        Action::RemoveTrust => has(P::TrustRemoved),
        Action::RemovePointer => has(P::PointerRemoved),
        Action::RemoveRecord => has(P::RecordRemoved),
        Action::PurgeData => has(P::DataPurged),
        Action::Close => has(P::Complete),
    }
}

/// Whether an existing receipt admits this operation again. A receipt binds
/// the exact operation bytes; a closed receipt makes the operation consumed.
pub(crate) fn admit_resume(
    receipt: &SetupReceipt,
    operation: &SetupOperation,
    operation_sha256: Sha256Digest,
) -> Result<(), &'static str> {
    if receipt.operation_sha256 != operation_sha256
        || receipt.operation_id != operation.operation_id
        || receipt.kind != operation.kind
    {
        return Err("setup_operation_changed");
    }
    if receipt.closed {
        return Err("setup_operation_consumed");
    }
    Ok(())
}

/// The actions still to run for `receipt` (none recorded: all of them).
pub(crate) fn remaining(
    kind: OperationKind,
    options: &SetupOptions,
    mode: Mode,
    recorded: &[SetupPhase],
) -> Result<Vec<Action>, &'static str> {
    Ok(actions(kind, options, mode)?
        .into_iter()
        .filter(|action| !satisfied(*action, kind, recorded))
        .collect())
}

/// U13: an install that found a predecessor proceeds only when the caller
/// acknowledged both its removal and the index reset.
pub(crate) fn acknowledged(
    operation: &SetupOperation,
    predecessors: usize,
) -> Result<(), &'static str> {
    if predecessors > 0
        && !(operation.acknowledgements.predecessor_removal
            && operation.acknowledgements.index_reset)
    {
        return Err("predecessor_acknowledgement_required");
    }
    Ok(())
}

/// The five chosen locations with their role names.
pub(crate) fn roles(locations: &Locations) -> [(&'static str, PathBuf); 5] {
    [
        ("program", PathBuf::from(&locations.program)),
        ("data", PathBuf::from(&locations.data)),
        ("state", PathBuf::from(&locations.state)),
        ("cache", PathBuf::from(&locations.cache)),
        ("logs", PathBuf::from(&locations.logs)),
    ]
}

/// A path as compared for nesting: case-folded on Windows.
fn folded(path: &Path, fold_case: bool) -> PathBuf {
    if fold_case {
        PathBuf::from(path.to_string_lossy().to_lowercase())
    } else {
        path.to_path_buf()
    }
}

fn related(left: &Path, right: &Path, fold_case: bool) -> bool {
    let (left, right) = (folded(left, fold_case), folded(right, fold_case));
    left.starts_with(&right) || right.starts_with(&left)
}

/// U39/U50 relationship rules that need no filesystem: every location is an
/// absolute, non-top-level folder without parent traversal; no two chosen
/// locations nest; none equals, contains or lies inside a pre-split root.
/// Returns one message per problem (empty: acceptable).
pub(crate) fn location_conflicts(
    locations: &Locations,
    pre_split: &[PathBuf],
    fold_case: bool,
) -> Vec<String> {
    let roles = roles(locations);
    let mut problems = Vec::new();
    for (role, path) in &roles {
        let normal = path
            .components()
            .filter(|part| matches!(part, Component::Normal(_)))
            .count();
        if !path.is_absolute()
            || normal < 2
            || path
                .components()
                .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
        {
            problems.push(format!(
                "{role} location {} must be an absolute folder below a top-level folder",
                path.display()
            ));
        }
        for root in pre_split {
            if related(path, root, fold_case) {
                problems.push(format!(
                    "{role} location {} overlaps the earlier AIRK location {}",
                    path.display(),
                    root.display()
                ));
            }
        }
    }
    for (index, (role, path)) in roles.iter().enumerate() {
        for (other, other_path) in &roles[index + 1..] {
            if related(path, other_path, fold_case) {
                problems.push(format!(
                    "{role} location {} and {other} location {} overlap",
                    path.display(),
                    other_path.display()
                ));
            }
        }
    }
    problems
}

/// The files of a program folder (relative, `/`-separated) that neither the
/// payload inventory lists nor setup or the platform installer own (its
/// uninstaller files, `PATH` receipt or `airkctl` link receipt). A chosen
/// program folder holding any of them is not adopted (U50).
pub(crate) fn foreign_program_entries(
    entries: &[String],
    listed: &BTreeSet<String>,
) -> Vec<String> {
    let uninstaller = |name: &str| {
        let Some(stem) = name.strip_prefix("unins") else {
            return false;
        };
        match (stem.get(..3), stem.get(3..)) {
            (Some(number), Some(extension)) => {
                number.bytes().all(|byte| byte.is_ascii_digit())
                    && matches!(extension, ".exe" | ".dat" | ".msg")
            }
            _ => false,
        }
    };
    entries
        .iter()
        .filter(|entry| {
            !listed.contains(entry.as_str())
                && entry.as_str() != INVENTORY_FILE
                && entry.as_str() != RECORD_FILE
                && entry.as_str() != PATH_RECEIPT
                && entry.as_str() != LINK_RECEIPT
                && !uninstaller(entry)
        })
        .cloned()
        .collect()
}

/// A program-relative `/`-separated path: no absolute path, no backslash, no
/// empty, `.` or `..` component.
pub(crate) fn relative_inside(relative: &str) -> bool {
    !relative.is_empty()
        && !relative.starts_with('/')
        && !relative.contains('\\')
        && !relative.contains(':')
        && relative
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// `program` joined with a validated program-relative path, one native
/// component at a time (so Windows paths never mix separators).
pub(crate) fn program_file(program: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(program.to_path_buf(), |path, part| path.join(part))
}

/// Whether a removal also deletes the placed program files. On macOS and
/// Linux `airkctl service uninstall` is the uninstaller (§9.5), so setup
/// removes them; on Windows the Inno uninstaller does after setup returns.
pub(crate) fn removes_program_files(host: Host) -> bool {
    host != Host::Windows
}

/// The link a link receipt owns, when the receipt is well formed and names
/// exactly this installation's `airkctl` as its target. Only an absolute
/// `…/.local/bin/airkctl` without `.`/`..` components is accepted.
pub(crate) fn owned_link(receipt: &str, airkctl: &Path) -> Result<PathBuf, &'static str> {
    let body = receipt.strip_suffix('\n').ok_or("link_receipt_malformed")?;
    let lines: Vec<&str> = body.split('\n').collect();
    let [schema, link, target] = lines.as_slice() else {
        return Err("link_receipt_malformed");
    };
    let link = PathBuf::from(*link);
    if *schema != LINK_RECEIPT_SCHEMA
        || !link.is_absolute()
        || !link.ends_with(".local/bin/airkctl")
        || link
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err("link_receipt_malformed");
    }
    if Path::new(*target) != airkctl {
        return Err("link_receipt_foreign");
    }
    Ok(link)
}

/// What is at a receipt's link path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LinkObservation {
    Absent,
    /// A symbolic link and its target as read (not followed).
    Link(PathBuf),
    /// Anything else: a file, a folder, or an unreadable link.
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LinkAction {
    Remove,
    AlreadyGone,
    /// Left in place with the reason; the receipt's claim has lapsed.
    Leave(&'static str),
}

/// Receipts only (§9.3): the link is removed only while it still points at
/// exactly this installation's `airkctl`.
pub(crate) fn link_action(observed: &LinkObservation, airkctl: &Path) -> LinkAction {
    match observed {
        LinkObservation::Absent => LinkAction::AlreadyGone,
        LinkObservation::Link(target) if target == airkctl => LinkAction::Remove,
        LinkObservation::Link(_) => {
            LinkAction::Leave("the airkctl link now points elsewhere, so it was left")
        }
        LinkObservation::Other => {
            LinkAction::Leave("the airkctl link path is no longer a link, so it was left")
        }
    }
}

/// The program files a removal deletes, in order: every listed file except
/// the running `airk`, then the payload inventory, then the running `airk`
/// last, so an interruption before the end leaves an image that can resume.
pub(crate) fn program_removal(listed: &BTreeSet<String>, own: &str) -> Vec<String> {
    let mut order: Vec<String> = listed
        .iter()
        .filter(|name| name.as_str() != own)
        .cloned()
        .collect();
    order.push(INVENTORY_FILE.to_owned());
    if listed.contains(own) {
        order.push(own.to_owned());
    }
    order
}

/// The folders holding `files` (program-relative), deepest first; each is
/// removed only when it is empty.
pub(crate) fn program_folders(files: &[String]) -> Vec<String> {
    let mut folders = BTreeSet::new();
    for file in files {
        let mut folder = file.as_str();
        while let Some((parent, _)) = folder.rsplit_once('/') {
            folders.insert(parent.to_owned());
            folder = parent;
        }
    }
    let mut folders: Vec<String> = folders.into_iter().collect();
    folders.sort_by(|left, right| {
        right
            .matches('/')
            .count()
            .cmp(&left.matches('/').count())
            .then_with(|| left.cmp(right))
    });
    folders
}
