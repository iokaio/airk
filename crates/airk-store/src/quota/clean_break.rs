// SPDX-License-Identifier: Apache-2.0
//! Typed, noncreating census of the native quota metadata retired by a clean break.
use super::*;
use crate::service::registry::catalog::setup::storage::{SetupDirectory, SetupFileMode};
use airk_core::{identity::Incarnation, service::ProjectRegistration};
use std::{collections::BTreeSet, ffi::OsStr};

#[derive(Default)]
pub(crate) struct CleanBreakInventory {
    pub files: Vec<(Vec<String>, u64, bool)>,
    pub directories: Vec<Vec<String>>,
    pub staging: BTreeMap<airk_core::service::ProjectBinding, BTreeSet<PathBuf>>,
}
impl CleanBreakInventory {
    pub(crate) fn file(&mut self, path: &[&str], maximum: u64, lock: bool) {
        self.files.push((
            path.iter().map(|value| (*value).into()).collect(),
            maximum,
            lock,
        ));
    }
    pub(crate) fn directory(&mut self, path: &[&str]) {
        self.directories
            .push(path.iter().map(|value| (*value).into()).collect());
    }
}
pub(crate) fn missing(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<io::Error>()
        .is_some_and(|error| error.kind() == io::ErrorKind::NotFound)
}

/// Whole-root reset validates credit ownership without converting old operations
/// into successful or terminal work. Native callers separately prove complete
/// logical absence and hold every existing advisory before discarding the rows.
pub(crate) fn clean_break_inventory(
    root: &SetupDirectory,
    data: &Path,
    registrations: &[ProjectRegistration],
    namespace: Incarnation,
) -> Result<CleanBreakInventory> {
    let known = registrations
        .iter()
        .filter(|row| Path::new(&row.data_root) == data)
        .map(|row| row.collection.as_str())
        .collect::<BTreeSet<_>>();
    let control = match root.open_dir(OsStr::new(".quota-1.4")) {
        Ok(directory) => directory,
        Err(error) if missing(&error) => return Ok(CleanBreakInventory::default()),
        Err(error) => return Err(error),
    };
    ensure!(
        control.identity()?.0 == root.identity()?.0,
        "quota cleanup crosses filesystem"
    );
    let mut result = CleanBreakInventory::default();
    result.directory(&[".quota-1.4"]);
    for (count, name) in control.entries()?.enumerate() {
        ensure!(count < 5, "unknown clean-break quota metadata");
        let name = name?;
        let text = name.to_str().context("invalid quota metadata name")?;
        match text {
            "ledger.lock" => {
                ensure!(
                    control
                        .open_file(&name, SetupFileMode::Read)?
                        .metadata()?
                        .len()
                        == 0,
                    "unknown quota ledger lock"
                );
                result.file(&[".quota-1.4", text], 0, true);
            }
            "policy.json" => {
                let policy: QuotaPolicy =
                    serde_json::from_slice(&control.read_file(&name, POLICY_MAX)?)?;
                policy.validate()?;
                ensure!(
                    policy
                        .collections
                        .keys()
                        .all(|name| known.contains(name.as_str())),
                    "foreign clean-break quota policy collection"
                );
                result.file(&[".quota-1.4", text], POLICY_MAX, false);
            }
            "reservations" => {
                let records = control.open_dir(&name)?;
                ensure!(
                    records.identity()?.0 == root.identity()?.0,
                    "reservation cleanup crosses filesystem"
                );
                result.directory(&[".quota-1.4", text]);
                let mut names = BTreeSet::new();
                for (count, name) in records.entries()?.enumerate() {
                    ensure!(
                        count < MAX_RESERVATIONS * 2 + 2,
                        "excessive clean-break reservations"
                    );
                    let name = name?;
                    let value = name.to_str().context("invalid reservation name")?;
                    ensure!(
                        value.starts_with("r-")
                            && value.len() <= 128
                            && value
                                .bytes()
                                .all(|byte| byte.is_ascii_alphanumeric()
                                    || matches!(byte, b'-' | b'.')),
                        "unknown reservation name"
                    );
                    let file = records.open_file(&name, SetupFileMode::Read)?;
                    if let Some(stem) = value.strip_suffix(".lease") {
                        ensure!(
                            !stem.contains('.') && file.metadata()?.len() == 0,
                            "invalid reservation lease"
                        );
                        result.file(&[".quota-1.4", text, value], 0, true);
                    } else {
                        ensure!(!value.contains('.'), "noncanonical reservation name");
                        let record: Record =
                            serde_json::from_slice(&records.read_file(&name, RECORD_MAX)?)?;
                        ensure!(
                            record.bytes > 0
                                && (record.collection.is_empty()
                                    || known.contains(record.collection.as_str())),
                            "foreign clean-break reservation credit"
                        );
                        if !record.collection.is_empty() {
                            let registration = registrations
                                .iter()
                                .find(|row| {
                                    Path::new(&row.data_root) == data
                                        && row.collection == record.collection
                                })
                                .context("reservation staging registration missing")?;
                            let quota = Quota {
                                root: data.to_path_buf(),
                                namespace: Namespace::Service,
                                funding: None,
                            };
                            let stage = staging_path(
                                &quota,
                                &record.collection,
                                &data.join(".quota-1.4/reservations").join(value),
                            )?;
                            result
                                .staging
                                .entry(registration.binding)
                                .or_default()
                                .insert(stage);
                        }
                        result.file(&[".quota-1.4", text, value], RECORD_MAX, false);
                    }
                    names.insert(value.to_owned());
                }
                for name in &names {
                    let other = name
                        .strip_suffix(".lease")
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("{name}.lease"));
                    ensure!(
                        names.contains(&other),
                        "unpaired reservation metadata is preserved"
                    );
                }
            }
            "current-grants" => {
                let records = control.open_dir(&name)?;
                ensure!(
                    records.identity()?.0 == root.identity()?.0,
                    "current grants cross filesystem"
                );
                result.directory(&[".quota-1.4", text]);
                for (count, name) in records.entries()?.enumerate() {
                    ensure!(
                        count < MAX_RESERVATIONS,
                        "excessive clean-break current grants"
                    );
                    let name = name?;
                    let bytes = records.read_file(&name, RECORD_MAX)?;
                    let row = current::decode_inventory_record(&name, &bytes)?;
                    ensure!(
                        known.contains(row.record.collection.as_str()),
                        "foreign clean-break current credit"
                    );
                    result.file(
                        &[
                            ".quota-1.4",
                            text,
                            name.to_str().context("invalid current grant name")?,
                        ],
                        RECORD_MAX,
                        false,
                    );
                }
            }
            "split-allocations" => {
                let funding =
                    funding::clean_break_inventory(&control, root, data, registrations, namespace)?;
                result.files.extend(funding.files);
                result.directories.extend(funding.directories);
                for (binding, paths) in funding.staging {
                    result.staging.entry(binding).or_default().extend(paths);
                }
            }
            _ => anyhow::bail!("unresolved or unknown quota metadata is preserved: {text}"),
        }
    }
    Ok(result)
}
