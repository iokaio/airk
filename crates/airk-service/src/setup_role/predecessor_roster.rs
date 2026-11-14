// SPDX-License-Identifier: Apache-2.0
//! Bounded orphan discovery. Names locate candidates; they never prove ownership
//! or authorize a deletion. Existing retirement owners keep their receipts.
use super::*;
use airk_process::directory::OpenedDirectory;
#[cfg(unix)]
use std::io::Read;
use std::{collections::BTreeSet, ffi::OsString, fs::File};

const OMITTED: &str = "predecessor_report_capacity_reached: at least one additional observed item could not be listed; discovery/cleanup is incomplete, physical items remain preserved; inspect the original setup receipt and predecessor ownership receipts";
const MAX_ENTRIES: usize = 128; // Existing native editor/profile discovery bound.

/// Couple every caller to Core's existing disclosure bound. Keep restart risks
/// ahead of ordinary residues and reserve an explicit, conservative omission
/// marker instead of silently losing evidence. This never removes artifacts.
pub(super) fn retain_bounded(items: &mut Vec<Leftover>, incoming: Leftover, context: &Path) {
    let incoming = Leftover {
        item: text(incoming.item),
        reason: text(incoming.reason),
        restarts_old_service: incoming.restarts_old_service,
    };
    let marker = Leftover {
        item: text(context.display().to_string()),
        reason: OMITTED.into(),
        restarts_old_service: true,
    };
    let had_marker = items.contains(&marker);
    items.retain(|item| *item != marker);
    if !items.contains(&incoming) {
        items.push(incoming);
    }
    items.sort_by(|a, b| {
        b.restarts_old_service
            .cmp(&a.restarts_old_service)
            .then_with(|| a.item.cmp(&b.item))
    });
    if had_marker || items.len() > airk_core::install_record::MAX_SETUP_LEFTOVERS {
        items.truncate(airk_core::install_record::MAX_SETUP_LEFTOVERS - 1);
        items.push(marker);
    }
}

fn text(value: impl AsRef<str>) -> String {
    let value = value.as_ref();
    let max = airk_core::install_record::MAX_SETUP_TEXT;
    if value.len() <= max {
        return value.into();
    }
    let mut end = max - 32;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{} [path/detail exceeds bound]", &value[..end])
}

struct Roster {
    items: Vec<Leftover>,
    context: PathBuf,
}
impl Roster {
    fn add(&mut self, item: impl AsRef<str>, state: &str, detail: impl AsRef<str>, restarts: bool) {
        retain_bounded(
            &mut self.items,
            Leftover {
                item: text(item),
                reason: text(format!(
                    "{state}: {}; preserved, no cleanup was authorized by this observation",
                    detail.as_ref()
                )),
                restarts_old_service: restarts,
            },
            &self.context,
        );
    }
    fn failed(&mut self, item: impl AsRef<str>, detail: impl AsRef<str>, restarts: bool) {
        self.add(item, "discovery_unknown", detail, restarts);
    }
    fn candidate(&mut self, path: &Path, kind: &str, restarts: bool) {
        match node(path) {
            Node::Absent=>{},
            Node::File(file)=>{if file.metadata().is_ok() {self.add(path.display().to_string(),"unproven_preserved",format!("{kind}; retain original ownership/retirement receipt for its existing native owner"),restarts);} else {self.failed(path.display().to_string(),"held file metadata unavailable",restarts);}},
            Node::Directory(_)=>self.add(path.display().to_string(),"unproven_preserved",format!("{kind}; retain original ownership/retirement receipt for its existing native owner"),restarts),
            Node::Refused=>self.add(path.display().to_string(),"inspection_refused",format!("{kind}; linked, redirected, foreign-writable or unsupported filesystem object"),restarts),
            Node::Unknown=>self.failed(path.display().to_string(),format!("{kind}; filesystem observation unavailable"),restarts),
        }
    }
    fn directory(
        &mut self,
        path: &Path,
        kind: &str,
        restarts: bool,
        accept: impl Fn(&str) -> bool,
    ) {
        let directory = match node(path) {
            Node::Absent => return,
            Node::Directory(directory) => directory,
            Node::Unknown => {
                self.failed(
                    path.display().to_string(),
                    format!("{kind}; directory unavailable"),
                    restarts,
                );
                return;
            }
            _ => {
                self.add(
                    path.display().to_string(),
                    "inspection_refused",
                    format!("{kind}; directory is not a safe native directory"),
                    restarts,
                );
                return;
            }
        };
        match names(&directory, path) {
            Ok((entries, complete)) => {
                for name in entries {
                    if let Some(name) = name.to_str()
                        && accept(name)
                    {
                        self.candidate(&path.join(name), kind, restarts);
                    }
                }
                if !complete {
                    self.failed(path.display().to_string(),format!("{kind}; enumeration exceeded existing {MAX_ENTRIES}-entry bound; additional candidates are unknown"),restarts);
                }
            }
            Err(_) => self.failed(
                path.display().to_string(),
                format!("{kind}; enumeration unavailable"),
                restarts,
            ),
        }
    }
}

enum Node {
    Absent,
    File(File),
    Directory(OpenedDirectory),
    Refused,
    Unknown,
}

/// Open the leaf relative to the existing shared native parent pin. No link
/// traversal, index open, credential read, image execution or mutation occurs.
fn node(path: &Path) -> Node {
    let Some(parent_path) = path.parent() else {
        return Node::Refused;
    };
    let parent = match OpenedDirectory::open(parent_path) {
        Ok(parent) => parent,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Node::Absent,
        Err(_) => return Node::Refused,
    };
    if airk_ipc::endpoint::require_protected_parent_handle(parent.as_file()).is_err() {
        return Node::Refused;
    }
    let Some(name) = path.file_name() else {
        return Node::Refused;
    };
    let file = match open_leaf(&parent, path, name) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Node::Absent,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return Node::Refused,
        #[cfg(unix)]
        Err(error) if error.raw_os_error() == Some(libc::ELOOP) => return Node::Refused,
        Err(_) => return Node::Unknown,
    };
    let metadata = match file.metadata() {
        Ok(metadata) => metadata,
        Err(_) => return Node::Unknown,
    };
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Node::Refused;
        }
    }
    if metadata.is_dir() {
        match parent.open_child(name) {
            Ok(directory)
                if matches!((directory.identity(), airk_process::directory::identity(&file)),
                    (Ok(actual), Ok(expected)) if actual == expected) =>
            {
                Node::Directory(directory)
            }
            _ => Node::Refused,
        }
    } else if metadata.is_file() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            // SAFETY: geteuid has no preconditions and returns the caller UID.
            if metadata.nlink() != 1 || metadata.uid() != unsafe { libc::geteuid() } {
                return Node::Refused;
            }
        }
        Node::File(file)
    } else {
        Node::Refused
    }
}

#[cfg(windows)]
fn open_leaf(_: &OpenedDirectory, path: &Path, _: &std::ffi::OsStr) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(0x00200000 | 0x02000000)
        .open(path)
}
#[cfg(unix)]
fn open_leaf(parent: &OpenedDirectory, _: &Path, name: &std::ffi::OsStr) -> std::io::Result<File> {
    use std::os::{
        fd::{AsRawFd, FromRawFd},
        unix::ffi::OsStrExt,
    };
    let name = std::ffi::CString::new(name.as_bytes())
        .map_err(|_| std::io::Error::other("invalid leaf"))?;
    // SAFETY: the held directory FD and NUL-terminated one-component leaf live
    // across openat; O_NOFOLLOW refuses a link and O_NONBLOCK avoids FIFO waits.
    let fd = unsafe {
        libc::openat(
            parent.as_file().as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
        )
    };
    if fd < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        // SAFETY: openat returned one fresh owned descriptor.
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}

#[cfg(windows)]
fn names(_: &OpenedDirectory, path: &Path) -> Result<(Vec<OsString>, bool)> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(path)? {
        if names.len() == MAX_ENTRIES {
            return Ok((names, false));
        }
        names.push(entry?.file_name());
    }
    names.sort();
    Ok((names, true))
}
#[cfg(unix)]
fn names(directory: &OpenedDirectory, _: &Path) -> Result<(Vec<OsString>, bool)> {
    use std::os::{fd::AsRawFd, unix::ffi::OsStringExt};
    // SAFETY: open this pinned directory itself, obtaining an independent
    // enumeration offset without resolving a named ancestor or following links.
    let fd = unsafe {
        libc::openat(
            directory.as_file().as_raw_fd(),
            c".".as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        )
    };
    ensure!(fd >= 0, "cannot open pinned directory observation");
    // SAFETY: fd is a fresh descriptor for an opened directory.
    let raw = unsafe { libc::fdopendir(fd) };
    if raw.is_null() {
        // SAFETY: failed fdopendir did not consume the fresh descriptor.
        unsafe { libc::close(fd) };
        bail!("cannot enumerate pinned directory");
    }
    struct Held(*mut libc::DIR);
    impl Drop for Held {
        fn drop(&mut self) {
            // SAFETY: Held uniquely owns this successful fdopendir result.
            unsafe { libc::closedir(self.0) };
        }
    }
    let held = Held(raw);
    let mut result = Vec::new();
    loop {
        #[cfg(target_os = "linux")]
        // SAFETY: __errno_location returns this thread's writable errno slot.
        unsafe {
            *libc::__errno_location() = 0;
        }
        #[cfg(target_os = "macos")]
        // SAFETY: __error returns this thread's writable errno slot.
        unsafe {
            *libc::__error() = 0;
        }
        // SAFETY: the owned stream remains live; readdir returns its own entry.
        let entry = unsafe { libc::readdir(held.0) };
        if entry.is_null() {
            ensure!(
                std::io::Error::last_os_error().raw_os_error().unwrap_or(0) == 0,
                "directory enumeration incomplete"
            );
            break;
        }
        // SAFETY: POSIX dirent d_name is NUL-terminated within this live entry.
        let name = unsafe { std::ffi::CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
        if matches!(name, b"." | b"..") {
            continue;
        }
        if result.len() == MAX_ENTRIES {
            return Ok((result, false));
        }
        result.push(OsString::from_vec(name.to_vec()));
    }
    result.sort();
    Ok((result, true))
}

fn old_id(name: &str) -> bool {
    let name = name
        .strip_suffix(".service")
        .or_else(|| name.strip_suffix(".plist"))
        .unwrap_or(name);
    ["airksvc-", "airk-"].into_iter().any(|prefix| {
        name.strip_prefix(prefix).is_some_and(|suffix| {
            suffix.len() == 32 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    })
}

/// Root-independent disclosure is used both before placement and after owned
/// retirement, so an absent install folder never suppresses orphan evidence.
pub(super) fn unretired_items() -> Result<Vec<Leftover>> {
    let context =
        super::files::pointer_path().unwrap_or_else(|_| PathBuf::from("predecessor_roster"));
    let mut roster = Roster {
        items: Vec::new(),
        context,
    };
    match airk_installer::runtime_install::predecessor_discovery_leftovers() {
        Ok(items) => {
            for item in items {
                retain_bounded(&mut roster.items, item, &roster.context);
            }
        }
        Err(_) => roster.failed(
            "predecessor_install_roots",
            "default/custom root discovery could not complete",
            true,
        ),
    }
    let mut locations = BTreeSet::new();
    let mut launchers = BTreeSet::new();
    for channel in [
        airk_host::user_paths::Channel::Stable,
        airk_host::user_paths::Channel::Preview,
    ] {
        match airk_host::user_paths::UserPaths::historical(channel) {
            Ok(paths) => {
                launchers.insert(paths.launcher);
                for install in [
                    paths.install.clone(),
                    paths.install.with_file_name(channel.leaf("runtime")),
                ] {
                    match airk_host::user_paths::owned_install_locations(&install) {
                        Ok(paths) => locations.extend(paths),
                        Err(_) => roster.failed(
                            install.display().to_string(),
                            "legacy location ownership inspection refused",
                            false,
                        ),
                    }
                }
            }
            Err(_) => roster.failed(
                format!("predecessor_{channel:?}"),
                "native user folders unavailable",
                true,
            ),
        }
    }
    for path in &locations {
        for leaf in [
            "control/http-1/user-trust-1.json",
            "http-1/user-trust-1.json",
        ] {
            roster.candidate(
                &path.join(leaf),
                "original AIRK user-trust receipt; only trust::remove_owned may retire it",
                false,
            );
        }
        for leaf in ["control/service-1.4.sqlite3", "service-1.4.sqlite3"] {
            let registry = path.join(leaf);
            if !matches!(node(&registry), Node::Absent) {
                roster.candidate(&registry,"original service registry/custom-data inventory; source/data bindings must be reauthenticated by the existing replacement owner, custom locations remain unknown here",false);
            }
        }
    }
    for launcher in launchers {
        for leaf in [
            "airk-launcher.json",
            "universal-uninstall.json",
            airk_installer::universal_launcher::launcher_name(),
        ] {
            roster.candidate(&launcher.join(leaf),"universal launcher remnant; original launcher owner/retirement receipt is required",true);
        }
        roster.directory(
            &launcher.join("extension-uninstall"),
            "original extension PATH/shell retirement receipt",
            false,
            |name| name.ends_with(".json"),
        );
    }
    match airk_host::user_paths::configuration_base() {
        Ok(base) => {
            roster.directory(
                &base.join("services"),
                "saved pre-split login definition/receipt",
                true,
                |name| {
                    old_id(
                        name.strip_suffix(".xml")
                            .or_else(|| name.strip_suffix(".json"))
                            .unwrap_or(name),
                    )
                },
            );
            roster.directory(
                &base.join("airk/services"),
                "original 1.3 login ownership receipt",
                true,
                |name| old_id(name.strip_suffix(".json").unwrap_or(name)),
            );
            #[cfg(target_os = "linux")]
            roster.directory(
                &base.join("systemd/user"),
                "pre-split systemd user unit",
                true,
                old_id,
            );
        }
        Err(_) => roster.failed(
            "predecessor_login_receipts",
            "native configuration root unavailable",
            true,
        ),
    }
    match airk_installer::runtime_legacy_origin::native_cache_roots() {
        Ok(roots) => {
            for root in roots {
                editor(&mut roster, &root);
            }
        }
        Err(_) => {
            roster.failed("editor_cache_discovery","native editor profile/root ownership could not be established; caches/CLI copies remain unknown",false);
            if let Ok(roots) = airk_installer::runtime_legacy_origin::native_global_stores() {
                for root in roots {
                    roster.candidate(
                        &root.join("iokaio.airk"),
                        "unproven editor storage; no traversal/cleanup performed",
                        false,
                    );
                }
            }
        }
    }
    shell(&mut roster);
    process_path(&mut roster);
    native_os(&mut roster);
    roster.items.sort_by_key(|item| !item.restarts_old_service);
    Ok(roster.items)
}

fn editor(roster: &mut Roster, root: &Path) {
    let extension = root.join("iokaio.airk");
    roster.directory(
        &extension.join("helpers"),
        "old native editor helper cache; original legacy-origin proof required",
        false,
        |_| true,
    );
    for leaf in [
        "cli/bin/airk",
        "cli/bin/airk.exe",
        "service-cli/bin/airk",
        "service-cli/bin/airk.exe",
        "service-cli/bin/airk.cmd",
    ] {
        roster.candidate(
            &extension.join(leaf),
            "old editor CLI copy; preserve original extension uninstall receipt",
            false,
        );
    }
}

fn shell(roster: &mut Roster) {
    #[cfg(windows)]
    let _ = roster;
    #[cfg(unix)]
    {
        let home = match airk_host::user_paths::home() {
            Ok(home) => home,
            Err(_) => {
                roster.failed("shell_profiles", "native home unavailable", false);
                return;
            }
        };
        let zdot = std::env::var_os("ZDOTDIR")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| home.clone());
        let mut profiles: Vec<_> = [".profile", ".bashrc", ".bash_profile", ".bash_login"]
            .into_iter()
            .map(|name| home.join(name))
            .collect();
        profiles.extend([zdot.join(".zprofile"), zdot.join(".zshrc")]);
        if let Ok(config) = airk_host::user_paths::configuration_base() {
            profiles.push(config.join("fish/conf.d/airk-path.fish"));
        }
        profiles.sort();
        profiles.dedup();
        for path in profiles {
            match node(&path) {
                Node::Absent => {}
                Node::File(file) => {
                    let mut bytes = Vec::new();
                    let result = file
                        .take(airk_host::native_command::MAX_CONFIG + 1)
                        .read_to_end(&mut bytes);
                    if result.is_err() || bytes.len() as u64 > airk_host::native_command::MAX_CONFIG
                    {
                        roster.failed(path.display().to_string(),"shell profile exceeds bounded observation; markers/ownership remain unknown",false);
                    } else if bytes
                        .windows(b"# AIRK CLI PATH".len())
                        .any(|part| part == b"# AIRK CLI PATH")
                    {
                        roster.add(path.display().to_string(),"unproven_preserved","AIRK shell marker observed; only the original shell receipt/exact block owner can authorize removal",false);
                    }
                }
                _ => roster.add(
                    path.display().to_string(),
                    "inspection_refused",
                    "shell profile link/type/ownership refused; no bytes changed",
                    false,
                ),
            }
        }
    }
}

fn native_os(roster: &mut Roster) {
    #[cfg(windows)]
    {
        windows_files(roster);
        let result = (|| -> Result<Value> {
            let program =
                airk_host::native_command::windows_tool("WindowsPowerShell/v1.0/powershell.exe")?;
            let output =
                airk_host::native_command::execute(&airk_host::native_command::Invocation::new(
                    program,
                    &[
                        "-NoLogo",
                        "-NoProfile",
                        "-NonInteractive",
                        "-Command",
                        include_str!("predecessor_roster/windows.ps1"),
                    ],
                ))?;
            ensure!(output.success, "OS observation unavailable");
            Ok(serde_json::from_str(&output.text)?)
        })();
        match result {
            Ok(value) => windows_observations(roster, &value),
            Err(_) => roster.failed(
                "Windows AIRK tasks/PATH/uninstall registrations",
                "bounded native OS observation could not complete",
                true,
            ),
        }
    }
    #[cfg(target_os = "linux")]
    {
        for args in [
            vec![
                "--user",
                "list-unit-files",
                "--type=service",
                "--no-legend",
                "--no-pager",
            ],
            vec![
                "--user",
                "list-units",
                "--all",
                "--type=service",
                "--no-legend",
                "--no-pager",
                "--plain",
            ],
        ] {
            match airk_host::native_command::execute(&airk_host::native_command::Invocation::new(
                "/usr/bin/systemctl",
                &args,
            )) {
                Ok(output) if output.success => {
                    for line in output.text.lines() {
                        if let Some(name) = line.split_whitespace().next()
                            && old_id(name)
                        {
                            roster.add(name,"unproven_preserved","OS systemd registration; original service-manager receipt/definition proof required",true);
                        }
                    }
                }
                _ => roster.failed(
                    "systemd user AIRK registrations",
                    "native user-bus observation unavailable",
                    true,
                ),
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = airk_host::user_paths::home() {
            roster.directory(
                &home.join("Library/LaunchAgents"),
                "old LaunchAgent definition",
                true,
                |name| old_id(name) || name.starts_with("io.iokaio.airk.service."),
            );
        }
        let uid = match airk_ipc::endpoint::current_user() {
            Ok(uid) => uid,
            Err(_) => {
                roster.failed(
                    "launchd GUI domain",
                    "native user identity unavailable",
                    true,
                );
                return;
            }
        };
        let domain = format!("gui/{uid}");
        let mut current = BTreeSet::new();
        if let Ok(Some(installed)) = super::files::read_installation() {
            let image = PathBuf::from(&installed.record.locations.program)
                .join(&installed.record.executables.airk.path);
            if let Ok((digest, bytes)) = super::files::digest_file(&image)
                && digest == installed.record.executables.airk.sha256
                && bytes == installed.record.executables.airk.bytes
                && let Some(bundle) = image.ancestors().nth(3)
                && bundle.file_name().is_some_and(|name| name == "AIRK.app")
                && matches!(
                    airk_installer::service_manager::macos_i4_login(
                        bundle,
                        &installed.record.channel,
                        "status"
                    ),
                    Ok(airk_installer::service_manager::MacLoginState::Enabled
                        | airk_installer::service_manager::MacLoginState::RequiresApproval)
                )
            {
                current.insert(format!(
                    "io.iokaio.airk.service.{}",
                    installed.record.channel
                ));
            }
        }
        match airk_host::native_command::execute(&airk_host::native_command::Invocation::new(
            "/bin/launchctl",
            &["print", &domain],
        )) {
            Ok(output) if output.success => mac_observations(roster, &output.text, &current),
            _ => roster.failed(
                "launchd AIRK registrations",
                "native GUI-domain observation unavailable",
                true,
            ),
        }
    }
}

fn process_path(roster: &mut Roster) {
    let Some(value) = std::env::var_os("PATH") else {
        roster.failed(
            "invoking process PATH",
            "PATH observation unavailable",
            false,
        );
        return;
    };
    if value.len() as u64 > airk_host::native_command::MAX_CONFIG {
        roster.failed(
            "invoking process PATH",
            "PATH exceeds existing native configuration byte bound",
            false,
        );
        return;
    }
    for (count, path) in std::env::split_paths(&value).enumerate() {
        if count >= MAX_ENTRIES {
            roster.failed(
                "invoking process PATH",
                "PATH entry inspection bound reached; additional entries unknown",
                false,
            );
            break;
        }
        let text = path
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase();
        if text.ends_with("/airk/launcher")
            || text.ends_with("/airk/launcher-preview")
            || text.ends_with("/airk/standalone/runtime")
            || text.contains("/iokaio.airk/")
                && (text.ends_with("/service-cli/bin") || text.ends_with("/cli/bin"))
        {
            roster.add(path.display().to_string(),"unproven_preserved","invoking process PATH still names an earlier AIRK location; persistent PATH/shell ownership is not inferred",false);
        }
    }
}

#[cfg(any(target_os = "macos", test))]
fn mac_observations(roster: &mut Roster, output: &str, current: &BTreeSet<String>) {
    let mut names = BTreeSet::new();
    for token in output.split_whitespace() {
        let token = token.trim_matches(['"', '\'', ',', ';', '{', '}', '=']);
        if (old_id(token) || token.starts_with("io.iokaio.airk.service."))
            && !current.contains(token)
        {
            names.insert(token);
        }
    }
    for name in names {
        roster.add(name,"unproven_preserved","GUI-domain launchd candidate; retain original login receipt; U72 permits only its verified native unregister owner",true);
    }
}

#[cfg(any(windows, test))]
fn windows_observations(roster: &mut Roster, value: &Value) {
    let Some(object) = value.as_object() else {
        roster.failed("Windows AIRK roster", "invalid native observation", true);
        return;
    };
    for (key, kind, restarts) in [
        ("tasks", "OS task registration", true),
        ("uninstall", "uninstall registration", false),
        ("path", "user PATH contribution", false),
    ] {
        let Some(items) = object.get(key).and_then(Value::as_array) else {
            roster.failed(kind, "native observation missing category", restarts);
            continue;
        };
        for item in items
            .iter()
            .take(airk_core::install_record::MAX_SETUP_LEFTOVERS)
        {
            if let Some(location) = item
                .as_str()
                .or_else(|| item.get("item").and_then(Value::as_str))
            {
                roster.add(location,"unproven_preserved",format!("{kind}; names alone are not ownership; retain original registration/PATH receipt"),restarts);
                if key == "tasks"
                    && let Some(executables) = item.get("executables").and_then(Value::as_array)
                {
                    for executable in executables.iter().take(MAX_ENTRIES) {
                        if let Some(executable) = executable.as_str() {
                            roster.add(executable,"unproven_preserved","OS registration names this executable/custom-root location; original captured runtime/install receipt must prove ownership before retirement; no old image was executed",true);
                        } else {
                            roster.failed(location,"OS executable location is not a string; custom-root discovery incomplete",true);
                        }
                    }
                    if executables.len() > MAX_ENTRIES {
                        roster.failed(location,"OS executable list exceeded native roster bound; custom roots remain unknown",true);
                    }
                }
            } else {
                roster.failed(kind, "invalid native item", restarts);
            }
        }
        if items.len() > airk_core::install_record::MAX_SETUP_LEFTOVERS {
            roster.failed(
                kind,
                "native observation exceeds report bound; additional entries unknown",
                restarts,
            );
        }
    }
    if object.get("complete") != Some(&Value::Bool(true)) {
        roster.failed(
            "Windows AIRK roster",
            "OS enumeration incomplete; additional restart risks may exist",
            true,
        );
    }
}

#[cfg(windows)]
fn windows_files(roster: &mut Roster) {
    match airk_process::user_folders::known_folder(
        airk_process::user_folders::KnownFolder::LocalAppData,
    ) {
        Ok(root) => {
            let standalone = root.join("AIRK/standalone");
            roster.candidate(
                &standalone,
                "old standalone installer folder (never run its uninstaller)",
                false,
            );
            roster.candidate(&standalone.join("user-path-owner.txt"),"original standalone PATH receipt; its existing owner alone may remove the exact contribution",false);
        }
        Err(_) => roster.failed(
            "standalone predecessor",
            "native LocalAppData unavailable",
            false,
        ),
    }
}

#[cfg(test)]
mod tests;
