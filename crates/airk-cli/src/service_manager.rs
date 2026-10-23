// SPDX-License-Identifier: Apache-2.0
//! Explicit per-user OS lifecycle. Inspection never installs or starts a worker.
use airk_kernel::Kernel;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const MAX_CONFIG: u64 = 128 * 1024;
#[cfg(target_os = "linux")]
#[path = "service_manager_linux.rs"]
mod linux;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Platform {
    Windows,
    Linux,
    Macos,
}

#[derive(Debug, Serialize)]
pub struct ServicePlan {
    pub id: String,
    pub platform: &'static str,
    pub definition_path: PathBuf,
    pub definition: String,
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub scope: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ServiceStatus {
    pub id: String,
    pub platform: &'static str,
    pub definition_path: PathBuf,
    pub installed: bool,
    pub loaded: Option<bool>,
    pub running: Option<bool>,
    pub registered_definition_matches: Option<bool>,
    pub registration_verification_error: Option<String>,
    pub manager_output: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: u32,
    id: String,
    data_root: String,
    definition_sha256: String,
}

pub struct ServiceManager {
    platform: Platform,
    id: String,
    principal: String,
    data: String,
    executable: PathBuf,
    args: Vec<String>,
    definition: PathBuf,
    receipt: PathBuf,
}

#[derive(Clone, Debug)]
struct Invocation {
    program: PathBuf,
    args: Vec<String>,
    env: BTreeMap<String, String>,
    input: Option<Vec<u8>>,
}
impl Invocation {
    fn new(program: impl Into<PathBuf>, args: &[&str]) -> Self {
        Self {
            program: program.into(),
            args: args.iter().map(|arg| (*arg).into()).collect(),
            env: BTreeMap::new(),
            input: None,
        }
    }
}
struct Output {
    success: bool,
    text: String,
}

impl ServiceManager {
    pub fn for_kernel(kernel: &Kernel) -> Result<Self> {
        let data = absolute_existing_ancestor(kernel.data_dir())?;
        let args = vec![
            "--root".into(),
            kernel
                .root()
                .to_str()
                .context("service root must be UTF-8")?
                .into(),
            "--data-dir".into(),
            data.to_str()
                .context("service data root must be UTF-8")?
                .into(),
            "--collection".into(),
            kernel.collection().into(),
            "schedule".into(),
            "run".into(),
            "--watch".into(),
        ];
        Self::for_command(&data, std::env::current_exe()?.canonicalize()?, args, false)
    }

    /// A separate shared-service identity never overwrites an old root scheduler.
    pub fn for_runtime(root: &Path) -> Result<Self> {
        let root = root.canonicalize()?;
        let launcher = crate::runtime_install::launcher_path(&root);
        Self::for_command(
            &root,
            launcher,
            vec![
                "runtime".into(),
                "foreground".into(),
                "--install-dir".into(),
                root.to_str().context("install root must be UTF-8")?.into(),
            ],
            true,
        )
    }

    fn for_command(
        data_path: &Path,
        executable: PathBuf,
        args: Vec<String>,
        shared: bool,
    ) -> Result<Self> {
        let platform = if cfg!(windows) {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::Macos
        } else if cfg!(target_os = "linux") {
            Platform::Linux
        } else {
            bail!("per-user service management is supported on Windows, macOS and Linux");
        };
        let principal = crate::local_ipc::current_user()?;
        let data = absolute_existing_ancestor(data_path)?;
        let data = data
            .to_str()
            .context("service data root must be UTF-8")?
            .to_owned();
        let prefix = if shared { "airksvc-" } else { "airk-" };
        let id = format!(
            "{prefix}{:x}",
            Sha256::digest(format!("{principal}\0{data}").as_bytes())
        );
        let id = id[..prefix.len() + 32].to_owned();
        let home = if platform == Platform::Windows {
            env_path("APPDATA")?
        } else {
            env_path("HOME")?
        };
        let config = if platform == Platform::Windows {
            home.join("AIRK")
        } else if platform == Platform::Linux {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"))
        } else {
            home.join("Library/Application Support/AIRK")
        };
        ensure!(
            config.is_absolute(),
            "per-user configuration directory must be absolute"
        );
        let definition = match platform {
            Platform::Windows => config.join("services").join(format!("{id}.xml")),
            Platform::Linux => config.join("systemd/user").join(format!("{id}.service")),
            Platform::Macos => home
                .join("Library/LaunchAgents")
                .join(format!("{id}.plist")),
        };
        let receipt = config.join("airk/services").join(format!("{id}.json"));
        for argument in args.iter().map(String::as_str).chain([executable
            .to_str()
            .context("service executable must be UTF-8")?])
        {
            ensure!(
                !argument.chars().any(char::is_control),
                "service paths cannot contain control characters"
            );
            ensure!(
                argument.len() <= 32768,
                "service argument exceeds path budget"
            );
        }
        Ok(Self {
            platform,
            id,
            principal,
            data,
            executable,
            args,
            definition,
            receipt,
        })
    }

    pub fn plan(&self) -> Result<ServicePlan> {
        let executable = self.executable.to_str().context("non-UTF-8 executable")?;
        let definition = match self.platform {
            Platform::Linux => {
                let command = std::iter::once(executable)
                    .chain(self.args.iter().map(String::as_str))
                    .map(systemd_argument)
                    .collect::<Vec<_>>()
                    .join(" ");
                format!(
                    "# AIRK managed per-user scheduler {}\n[Unit]\nDescription=AIRK native indexing scheduler\n\n[Service]\nType=simple\nExecStart=:{command}\nRestart=on-failure\nRestartSec=10\nTimeoutStopSec=30\nNoNewPrivileges=yes\nUMask=0077\n\n[Install]\nWantedBy=default.target\n",
                    self.id
                )
            }
            Platform::Macos => {
                let args = std::iter::once(executable)
                    .chain(self.args.iter().map(String::as_str))
                    .map(|arg| format!("<string>{}</string>", xml(arg)))
                    .collect::<String>();
                format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>{}</string><key>ProgramArguments</key><array>{args}</array><key>RunAtLoad</key><true/><key>KeepAlive</key><true/><key>ThrottleInterval</key><integer>10</integer><key>ProcessType</key><string>Background</string></dict></plist>\n",
                    xml(&self.id)
                )
            }
            Platform::Windows => {
                let arguments = self
                    .args
                    .iter()
                    .map(|arg| windows_argument(arg))
                    .collect::<Vec<_>>()
                    .join(" ");
                format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n<Task version=\"1.2\" xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\"><RegistrationInfo><Description>AIRK managed per-user scheduler {}</Description></RegistrationInfo><Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{}</UserId></LogonTrigger></Triggers><Principals><Principal id=\"User\"><UserId>{}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals><Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><StartWhenAvailable>true</StartWhenAvailable><ExecutionTimeLimit>PT0S</ExecutionTimeLimit><RestartOnFailure><Interval>PT1M</Interval><Count>3</Count></RestartOnFailure></Settings><Actions Context=\"User\"><Exec><Command>{}</Command><Arguments>{}</Arguments></Exec></Actions></Task>\n",
                    xml(&self.id),
                    xml(&self.principal),
                    xml(&self.principal),
                    xml(&windows_path(executable)),
                    xml(&arguments)
                )
            }
        };
        ensure!(
            definition.len() as u64 <= MAX_CONFIG,
            "service definition exceeds 128 KiB"
        );
        Ok(ServicePlan {
            id: self.id.clone(),
            platform: self.platform_name(),
            definition_path: self.definition.clone(),
            definition,
            executable: self.executable.clone(),
            arguments: self.args.clone(),
            scope: if self.id.starts_with("airksvc-") {
                "current-user shared AIRK service; optional login registration"
            } else {
                "current-user login session; one worker per canonical data root"
            },
        })
    }

    pub fn status(&self) -> Result<ServiceStatus> {
        let installed = self.check_owned()?;
        let output = execute(&self.query(installed)?)?;
        let (loaded, running) = parse_status(self.platform, &output);
        let verification = if installed && loaded == Some(true) {
            self.verify_registration(&output)
        } else {
            Ok(None)
        };
        let (registered_definition_matches, registration_verification_error) = match verification {
            Ok(matches) => (matches, None),
            Err(error) => (None, Some(error.to_string().chars().take(512).collect())),
        };
        Ok(ServiceStatus {
            id: self.id.clone(),
            platform: self.platform_name(),
            definition_path: self.definition.clone(),
            installed,
            loaded,
            running,
            registered_definition_matches,
            registration_verification_error,
            manager_output: output.text,
        })
    }

    fn verify_registration(&self, output: &Output) -> Result<Option<bool>> {
        if self.platform == Platform::Windows {
            let value: serde_json::Value = serde_json::from_str(&output.text)?;
            return Ok(Some(
                value
                    .get("definition_matches")
                    .and_then(serde_json::Value::as_bool)
                    .context("Task Scheduler did not return a definition comparison")?,
            ));
        }
        #[cfg(target_os = "linux")]
        if self.platform == Platform::Linux {
            return linux::verify(self).map(Some);
        }
        bail!("registered configuration inspection is unavailable on this platform")
    }

    /// Explicit user action: register login startup and start immediately.
    pub fn install(&self) -> Result<ServiceStatus> {
        self.check_launchable()?;
        ensure!(
            !self.check_owned()?,
            "service already installed; use start or uninstall before reinstalling"
        );
        ensure!(
            !self.definition.try_exists()? && !self.receipt.try_exists()?,
            "refusing to replace an existing service definition"
        );
        let status = self.status()?;
        ensure!(
            status.loaded == Some(false),
            "service manager must confirm the service is absent before installation: {}",
            status.manager_output
        );
        let plan = self.plan()?;
        let definition = self.definition_bytes(&plan.definition);
        write_new(&self.definition, &definition)?;
        let receipt = Receipt {
            schema: 1,
            id: self.id.clone(),
            data_root: self.data.clone(),
            definition_sha256: format!("{:x}", Sha256::digest(&definition)),
        };
        if let Err(error) = write_new(&self.receipt, &serde_json::to_vec(&receipt)?) {
            let _ = fs::remove_file(&self.definition);
            return Err(error);
        }
        self.register_and_start()
    }

    pub fn start(&self) -> Result<ServiceStatus> {
        self.check_launchable()?;
        ensure!(self.check_owned()?, "service is not installed");
        let status = self.status()?;
        ensure!(
            status.loaded.is_some(),
            "service manager status is unavailable: {}",
            status.manager_output
        );
        if status.loaded == Some(false) {
            return self.register_and_start();
        }
        self.require_registered(&status)?;
        let invocation = match self.platform {
            Platform::Windows => self.schtasks(&["/Run", "/TN", &self.id])?,
            Platform::Linux => self.systemctl(&["start", &self.unit()]),
            Platform::Macos => self.launchctl(&["kickstart", &self.target()]),
        };
        checked(&invocation)?;
        self.status()
    }

    /// Stop this login's process, retaining automatic startup for the next login.
    pub fn stop(&self) -> Result<ServiceStatus> {
        ensure!(self.check_owned()?, "service is not installed");
        let status = self.status()?;
        ensure!(
            status.loaded.is_some(),
            "service manager status is unavailable: {}",
            status.manager_output
        );
        if status.loaded == Some(false) {
            return Ok(status);
        }
        self.require_registered(&status)?;
        if status.running == Some(false) && self.platform != Platform::Macos {
            return Ok(status);
        }
        let invocation = match self.platform {
            Platform::Windows => self.schtasks(&["/End", "/TN", &self.id])?,
            Platform::Linux => self.systemctl(&["stop", &self.unit()]),
            Platform::Macos => self.launchctl(&["bootout", &self.target()]),
        };
        checked(&invocation)?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let status = self.status()?;
            if status.running == Some(false)
                && (self.platform != Platform::Macos || status.loaded == Some(false))
            {
                return Ok(status);
            }
            ensure!(
                Instant::now() < deadline,
                "service manager has not confirmed the worker stopped"
            );
            thread::sleep(Duration::from_millis(100));
        }
    }

    pub fn uninstall(&self) -> Result<ServiceStatus> {
        ensure!(self.check_owned()?, "service is not installed");
        let status = self.stop()?;
        if status.loaded == Some(true) {
            self.require_registered(&self.status()?)?;
        }
        match self.platform {
            _ if status.loaded == Some(false) => (),
            Platform::Windows => checked(&self.schtasks(&["/Delete", "/TN", &self.id, "/F"])?)?,
            Platform::Linux => checked(&self.systemctl(&["disable", &self.unit()]))?,
            Platform::Macos => (),
        }
        ensure!(
            self.check_owned()?,
            "service ownership changed during uninstall"
        );
        fs::remove_file(&self.definition)?;
        fs::remove_file(&self.receipt)?;
        if self.platform == Platform::Linux {
            checked(&self.systemctl(&["daemon-reload"]))?;
        }
        self.status()
    }

    pub fn owned_registration_exists(&self) -> Result<bool> {
        self.check_owned()
    }

    fn check_launchable(&self) -> Result<()> {
        if self.id.starts_with("airksvc-") {
            crate::runtime_install::verified_launcher(Path::new(&self.data))?;
        }
        Ok(())
    }
    fn require_registered(&self, status: &ServiceStatus) -> Result<()> {
        if self.id.starts_with("airksvc-") {
            ensure!(
                status.loaded == Some(true) && status.registered_definition_matches == Some(true),
                "registered service definition is different or could not be verified; inspect OS startup configuration before changing it"
            );
        }
        Ok(())
    }
    fn register_and_start(&self) -> Result<ServiceStatus> {
        if self.platform == Platform::Windows {
            checked(
                &self.schtasks(&[
                    "/Create",
                    "/TN",
                    &self.id,
                    "/XML",
                    self.definition
                        .to_str()
                        .context("non-UTF-8 task definition")?,
                ])?,
            )?;
            let status = self.status()?;
            self.require_registered(&status)?;
            checked(&self.schtasks(&["/Run", "/TN", &self.id])?)?;
        } else if self.platform == Platform::Linux {
            checked(&self.systemctl(&["daemon-reload"]))?;
            let status = self.status()?;
            self.require_registered(&status)?;
            checked(&self.systemctl(&["enable", &self.unit()]))?;
            let status = self.status()?;
            self.require_registered(&status)?;
            checked(&self.systemctl(&["start", &self.unit()]))?;
        } else {
            // bootstrap takes the exact owned file, not a previously loaded label.
            checked(
                &self.launchctl(&[
                    "bootstrap",
                    &self.domain(),
                    self.definition
                        .to_str()
                        .context("non-UTF-8 service definition")?,
                ]),
            )?;
            let status = self.status()?;
            self.require_registered(&status)?;
        }
        self.status()
    }

    fn check_owned(&self) -> Result<bool> {
        let Some(bytes) = read_config(&self.receipt)? else {
            return Ok(false);
        };
        let receipt: Receipt = serde_json::from_slice(&bytes).context("invalid service receipt")?;
        ensure!(
            receipt.schema == 1 && receipt.id == self.id && receipt.data_root == self.data,
            "service receipt belongs to another data root"
        );
        let definition =
            read_config(&self.definition)?.context("installed service definition is missing")?;
        ensure!(
            format!("{:x}", Sha256::digest(&definition)) == receipt.definition_sha256,
            "service definition was modified; refusing to alter it"
        );
        Ok(true)
    }
    fn platform_name(&self) -> &'static str {
        match self.platform {
            Platform::Windows => "windows_task_scheduler",
            Platform::Linux => "systemd_user",
            Platform::Macos => "launchd_user",
        }
    }
    fn definition_bytes(&self, definition: &str) -> Vec<u8> {
        if self.platform == Platform::Windows {
            // schtasks imports UTF-16 XML. The BOM and declaration must agree.
            std::iter::once(0xfeffu16)
                .chain(definition.encode_utf16())
                .flat_map(u16::to_le_bytes)
                .collect()
        } else {
            definition.as_bytes().to_vec()
        }
    }
    fn unit(&self) -> String {
        format!("{}.service", self.id)
    }
    fn domain(&self) -> String {
        format!("gui/{}", self.principal)
    }
    fn target(&self) -> String {
        format!("{}/{}", self.domain(), self.id)
    }
    fn systemctl(&self, args: &[&str]) -> Invocation {
        let mut invocation = Invocation::new("/usr/bin/systemctl", &["--user", "--no-pager"]);
        invocation.args.extend(args.iter().map(|arg| (*arg).into()));
        invocation.env.insert("SYSTEMD_PAGER".into(), String::new());
        #[cfg(target_os = "linux")]
        {
            // Match the authority inspected by the SDK, independent of inherited
            // session-bus overrides from the invoking editor or shell.
            let directory = format!("/run/user/{}", unsafe { libc::geteuid() });
            invocation.env.insert(
                "DBUS_SESSION_BUS_ADDRESS".into(),
                format!("unix:path={directory}/bus"),
            );
            invocation.env.insert("XDG_RUNTIME_DIR".into(), directory);
        }
        invocation
    }
    fn launchctl(&self, args: &[&str]) -> Invocation {
        Invocation::new("/bin/launchctl", args)
    }
    fn schtasks(&self, args: &[&str]) -> Result<Invocation> {
        Ok(Invocation::new(windows_tool("schtasks.exe")?, args))
    }
    fn query(&self, installed: bool) -> Result<Invocation> {
        Ok(match self.platform {
            Platform::Linux => self.systemctl(&[
                "show",
                &self.unit(),
                "--property=LoadState",
                "--property=ActiveState",
                "--property=UnitFileState",
            ]),
            Platform::Macos => self.launchctl(&["print", &self.target()]),
            Platform::Windows => {
                // Task identity is data in the environment, never interpolated into PowerShell code.
                let mut invocation = Invocation::new(
                    windows_tool("WindowsPowerShell/v1.0/powershell.exe")?,
                    &[
                        "-NoLogo",
                        "-NoProfile",
                        "-NonInteractive",
                        "-Command",
                        include_str!("service_manager_windows.ps1"),
                    ],
                );
                invocation
                    .env
                    .insert("AIRK_SERVICE_ID".into(), self.id.clone());
                let expected = if installed {
                    let bytes =
                        read_config(&self.definition)?.context("owned task definition missing")?;
                    ensure!(
                        bytes.starts_with(&[0xff, 0xfe]) && bytes.len() % 2 == 0,
                        "owned task definition is not UTF-16LE"
                    );
                    let text = String::from_utf16(
                        &bytes[2..]
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|bytes| u16::from_le_bytes(*bytes))
                            .collect::<Vec<_>>(),
                    )?;
                    text.into_bytes()
                } else {
                    Vec::new()
                };
                invocation.input = Some(expected);
                invocation
            }
        })
    }
}

fn parse_status(platform: Platform, output: &Output) -> (Option<bool>, Option<bool>) {
    match platform {
        Platform::Windows if output.success => {
            let value: serde_json::Value = serde_json::from_str(&output.text).unwrap_or_default();
            (
                value.get("loaded").and_then(serde_json::Value::as_bool),
                value.get("running").and_then(serde_json::Value::as_bool),
            )
        }
        Platform::Linux if output.success => {
            let values: BTreeMap<_, _> = output
                .text
                .lines()
                .filter_map(|line| line.split_once('='))
                .collect();
            (
                values.get("LoadState").map(|state| *state != "not-found"),
                values
                    .get("ActiveState")
                    .map(|state| matches!(*state, "active" | "activating")),
            )
        }
        Platform::Macos if output.success => (
            Some(true),
            Some(
                output
                    .text
                    .lines()
                    .any(|line| line.trim() == "state = running"),
            ),
        ),
        Platform::Macos
            if output.text.contains("Could not find service")
                || output.text.contains("service not found") =>
        {
            (Some(false), Some(false))
        }
        _ => (None, None),
    }
}

fn env_path(name: &str) -> Result<PathBuf> {
    let path =
        PathBuf::from(std::env::var_os(name).with_context(|| format!("{name} is unavailable"))?);
    ensure!(path.is_absolute(), "{name} must be absolute");
    Ok(path)
}
#[cfg(windows)]
fn windows_tool(name: &str) -> Result<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    let mut buffer = vec![0u16; 32768];
    // SAFETY: the native API receives a writable, correctly bounded UTF-16 buffer.
    let length = unsafe {
        windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW(
            buffer.as_mut_ptr(),
            buffer.len() as u32,
        )
    };
    ensure!(
        length > 0 && (length as usize) < buffer.len(),
        "native system directory unavailable"
    );
    let path = PathBuf::from(std::ffi::OsString::from_wide(&buffer[..length as usize])).join(name);
    ensure!(
        path.is_absolute() && path.is_file(),
        "native Windows lifecycle tool is unavailable"
    );
    Ok(path)
}
#[cfg(not(windows))]
fn windows_tool(_name: &str) -> Result<PathBuf> {
    bail!("Windows lifecycle tools are unavailable on this host")
}
fn absolute_existing_ancestor(path: &Path) -> Result<PathBuf> {
    let mut parent = std::path::absolute(path)?;
    let mut suffix = Vec::new();
    while !parent.try_exists()? {
        suffix.push(
            parent
                .file_name()
                .context("missing existing data ancestor")?
                .to_owned(),
        );
        ensure!(parent.pop(), "missing existing data ancestor");
    }
    ensure!(parent.is_dir(), "data root ancestor must be a directory");
    let mut path = parent.canonicalize()?;
    for component in suffix.into_iter().rev() {
        path.push(component);
    }
    Ok(path)
}
fn read_config(path: &Path) -> Result<Option<Vec<u8>>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= MAX_CONFIG,
        "invalid, linked or oversized service configuration"
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_CONFIG + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_CONFIG,
        "service configuration exceeds size limit"
    );
    Ok(Some(bytes))
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() as u64 <= MAX_CONFIG,
        "service configuration exceeds size limit"
    );
    fs::create_dir_all(path.parent().context("service path has no parent")?)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn systemd_argument(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%")
    )
}
fn windows_path(value: &str) -> String {
    value
        .strip_prefix(r"\\?\UNC\")
        .map(|rest| format!(r"\\{rest}"))
        .unwrap_or_else(|| value.trim_start_matches(r"\\?\").into())
}
pub(crate) fn windows_argument(value: &str) -> String {
    let mut out = String::from("\"");
    let mut slashes = 0;
    for ch in value.chars() {
        if ch == '\\' {
            slashes += 1;
            continue;
        }
        if ch == '"' {
            out.extend(std::iter::repeat_n('\\', slashes * 2 + 1));
        } else {
            out.extend(std::iter::repeat_n('\\', slashes));
        }
        slashes = 0;
        out.push(ch);
    }
    out.extend(std::iter::repeat_n('\\', slashes * 2));
    out.push('"');
    out
}
fn checked(invocation: &Invocation) -> Result<()> {
    let output = execute(invocation)?;
    ensure!(
        output.success,
        "service manager command failed: {}",
        output.text
    );
    Ok(())
}
fn execute(invocation: &Invocation) -> Result<Output> {
    ensure!(
        invocation
            .input
            .as_ref()
            .is_none_or(|input| input.len() as u64 <= MAX_CONFIG),
        "manager input exceeds byte allowance"
    );
    let mut command = Command::new(&invocation.program);
    command
        .args(&invocation.args)
        .envs(&invocation.env)
        .stdin(if invocation.input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = crate::native_process::spawn(&mut command)
        .with_context(|| format!("start service manager {}", invocation.program.display()))?;
    let input = if let Some(bytes) = &invocation.input {
        let bytes = bytes.clone();
        let mut input = child.stdin.take().context("missing manager input")?;
        Some(thread::spawn(move || {
            let _ = input.write_all(&bytes);
        }))
    } else {
        None
    };
    let (sender, receiver) = mpsc::channel();
    for mut stream in [
        Box::new(child.stdout.take().context("missing manager stdout")?) as Box<dyn Read + Send>,
        Box::new(child.stderr.take().context("missing manager stderr")?),
    ] {
        let sender = sender.clone();
        thread::spawn(move || {
            let mut retained = Vec::new();
            let mut block = [0; 4096];
            while let Ok(read) = stream.read(&mut block) {
                if read == 0 {
                    break;
                }
                let remaining = 65536usize.saturating_sub(retained.len());
                retained.extend_from_slice(&block[..read.min(remaining)]);
            }
            let _ = sender.send(retained);
        });
    }
    drop(sender);
    let deadline = Instant::now() + Duration::from_secs(30);
    let success = loop {
        if let Some(status) = child.try_wait()? {
            break status.success();
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            if let Some(input) = input {
                let _ = input.join();
            }
            bail!("service manager exceeded 30 seconds; inspect OS service status before retrying");
        }
        thread::sleep(Duration::from_millis(20));
    };
    if let Some(input) = input {
        let _ = input.join();
    }
    let mut text = String::new();
    for _ in 0..2 {
        if let Ok(bytes) = receiver.recv_timeout(Duration::from_secs(1)) {
            text.push_str(&String::from_utf8_lossy(&bytes));
        }
    }
    Ok(Output { success, text })
}

/// The same bounded native command adapter also checks the paired helper's role/version.
pub(crate) fn check_executable_identity(program: &Path, expected: &str) -> Result<()> {
    let output = execute(&Invocation::new(program, &["--version"]))?;
    ensure!(
        output.success && output.text.trim() == expected,
        "paired executable identity mismatch"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn fixture(platform: Platform, root: &Path) -> ServiceManager {
        ServiceManager {
            platform,
            id: "airk-fixture".into(),
            principal: "1000".into(),
            data: root.to_string_lossy().into(),
            executable: PathBuf::from("/fixture path/$bin%/airk"),
            args: vec![
                "--root".into(),
                "/fixture & \"quoted\"/$name%/".into(),
                "schedule".into(),
                "run".into(),
                "--watch".into(),
            ],
            definition: root.join("fixture.conf"),
            receipt: root.join("receipt.json"),
        }
    }
    #[test]
    fn service_rendering_escapes_all_platform_arguments_without_shells() -> Result<()> {
        let root = tempfile::tempdir()?;
        let linux = fixture(Platform::Linux, root.path()).plan()?;
        assert!(
            linux
                .definition
                .contains("ExecStart=:\"/fixture path/$bin%%/airk\"")
        );
        assert!(linux.definition.contains("\\\"quoted\\\"/$name%%/"));
        let mac = fixture(Platform::Macos, root.path()).plan()?;
        assert!(mac.definition.contains("&amp; &quot;quoted&quot;"));
        assert!(mac.definition.contains("<key>ProgramArguments</key>"));
        let windows_service = fixture(Platform::Windows, root.path());
        let windows = windows_service.plan()?;
        let bytes = windows_service.definition_bytes(&windows.definition);
        assert_eq!(&bytes[..2], &[0xff, 0xfe]);
        let decoded = String::from_utf16(
            &bytes[2..]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect::<Vec<_>>(),
        )?;
        assert_eq!(decoded, windows.definition);
        assert!(decoded.starts_with("<?xml version=\"1.0\" encoding=\"UTF-16\"?>"));
        assert!(
            windows
                .definition
                .contains("<LogonType>InteractiveToken</LogonType>")
        );
        assert!(
            windows
                .definition
                .contains("<RunLevel>LeastPrivilege</RunLevel>")
        );
        assert_eq!(windows_argument("trailing\\"), "\"trailing\\\\\"");
        assert_eq!(windows_argument("a\\\"b"), "\"a\\\\\\\"b\"");
        assert!(
            !root.path().join("fixture.conf").exists(),
            "plan is read only"
        );
        Ok(())
    }
    #[test]
    fn service_receipt_refuses_modified_or_foreign_configuration() -> Result<()> {
        let root = tempfile::tempdir()?;
        let service = fixture(Platform::Linux, root.path());
        assert!(!service.check_owned()?);
        let definition = service.plan()?.definition;
        write_new(&service.definition, definition.as_bytes())?;
        let receipt = Receipt {
            schema: 1,
            id: service.id.clone(),
            data_root: service.data.clone(),
            definition_sha256: format!("{:x}", Sha256::digest(definition.as_bytes())),
        };
        write_new(&service.receipt, &serde_json::to_vec(&receipt)?)?;
        assert!(service.check_owned()?);
        fs::write(&service.definition, "user modified unit")?;
        assert!(service.check_owned().is_err());
        assert!(write_new(&service.definition, b"replacement").is_err());
        assert_eq!(
            fs::read_to_string(&service.definition)?,
            "user modified unit"
        );
        Ok(())
    }
    #[test]
    fn service_status_distinguishes_absent_running_and_manager_failure() {
        assert_eq!(
            parse_status(
                Platform::Linux,
                &Output {
                    success: true,
                    text: "LoadState=not-found\nActiveState=inactive\n".into()
                }
            ),
            (Some(false), Some(false))
        );
        assert_eq!(
            parse_status(
                Platform::Windows,
                &Output {
                    success: true,
                    text: "{\"loaded\":true,\"running\":true}".into()
                }
            ),
            (Some(true), Some(true))
        );
        assert_eq!(
            parse_status(
                Platform::Macos,
                &Output {
                    success: true,
                    text: "service = {\n state = running\n}".into()
                }
            ),
            (Some(true), Some(true))
        );
        assert_eq!(
            parse_status(
                Platform::Linux,
                &Output {
                    success: false,
                    text: "Failed to connect to bus".into()
                }
            ),
            (None, None)
        );
    }

    #[test]
    fn shared_service_mutation_requires_registered_definition_proof() -> Result<()> {
        let root = tempfile::tempdir()?;
        let mut service = fixture(Platform::Linux, root.path());
        service.id = "airksvc-fixture".into();
        let mut status = ServiceStatus {
            id: service.id.clone(),
            platform: "systemd_user",
            definition_path: service.definition.clone(),
            installed: true,
            loaded: Some(true),
            running: Some(false),
            registered_definition_matches: None,
            registration_verification_error: None,
            manager_output: String::new(),
        };
        assert!(
            service.require_registered(&status).is_err(),
            "an owned file does not prove manager identity"
        );
        status.registered_definition_matches = Some(false);
        assert!(
            service.require_registered(&status).is_err(),
            "a foreign inactive registration must survive"
        );
        status.registered_definition_matches = Some(true);
        service.require_registered(&status)?;
        status.loaded = None;
        assert!(service.require_registered(&status).is_err());
        Ok(())
    }

    #[test]
    #[cfg(windows)]
    fn task_scheduler_schema_compares_actions_principals_and_generated_metadata() -> Result<()> {
        let root = tempfile::tempdir()?;
        let mut service = fixture(Platform::Windows, root.path());
        service.principal = crate::local_ipc::current_user()?;
        let expected = service.plan()?.definition;
        // Native schema normalization, with an in-memory task and fake folder.
        // No connection, registration, login trigger or process is created.
        let fixture = r#"
$airkFixtureNative = New-Object -ComObject 'Schedule.Service'
$airkFixtureDefinition = $airkFixtureNative.NewTask(0)
$airkFixtureDefinition.XmlText = $env:AIRK_FIXTURE_XML
$airkFixtureTask = [pscustomobject]@{ State = 3; Definition = $airkFixtureDefinition }
$airkFixtureFolder = [pscustomobject]@{}
$airkFixtureFolder | Add-Member ScriptMethod GetTask { param($name) return $script:airkFixtureTask }
$airkFixtureScheduler = [pscustomobject]@{}
$airkFixtureScheduler | Add-Member ScriptMethod Connect {}
$airkFixtureScheduler | Add-Member ScriptMethod GetFolder { param($path) return $script:airkFixtureFolder }
$airkFixtureScheduler | Add-Member ScriptMethod NewTask { param($flags) return $script:airkFixtureNative.NewTask($flags) }
"#;
        let adapter = include_str!("service_manager_windows.ps1").replace(
            "New-Object -ComObject 'Schedule.Service'",
            "$airkFixtureScheduler",
        );
        for (xml, matches) in [
            (expected.clone(), true),
            (
                expected.replace(
                    "<Description>",
                    "<Author>generated by OS</Author><Description>",
                ),
                true,
            ),
            (expected.replace("<Command>", "<Command>changed-"), false),
            (
                expected.replace(
                    "<RunLevel>LeastPrivilege</RunLevel>",
                    "<RunLevel>HighestAvailable</RunLevel>",
                ),
                false,
            ),
            (
                expected.replace(
                    "</Actions>",
                    "<Exec><Command>other.exe</Command></Exec></Actions>",
                ),
                false,
            ),
        ] {
            let mut invocation = Invocation::new(
                windows_tool("WindowsPowerShell/v1.0/powershell.exe")?,
                &[
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    &format!("{fixture}\n{adapter}"),
                ],
            );
            invocation.env.insert("AIRK_FIXTURE_XML".into(), xml);
            invocation.input = Some(expected.as_bytes().to_vec());
            let output = execute(&invocation)?;
            ensure!(
                output.success,
                "native schema fixture failed: {}",
                output.text
            );
            let result: serde_json::Value = serde_json::from_str(&output.text)?;
            assert_eq!(result["loaded"], true);
            assert_eq!(result["running"], false);
            assert_eq!(result["definition_matches"], matches);
        }
        Ok(())
    }
}
