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
#[cfg(any(target_os = "macos", test))]
#[path = "service_manager_macos.rs"]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod macos;

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
    legacy_review: bool,
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
    /// Inspect/remove an explicitly selected former root scheduler. Never launch
    /// its command or open its Kernel/index to establish ownership.
    pub(crate) fn for_reviewed_legacy(
        root: &Path,
        data: &Path,
        collection: &str,
        executable: &Path,
    ) -> Result<Self> {
        ensure!(
            root.is_absolute() && data.is_absolute() && executable.is_absolute(),
            "legacy review requires absolute paths"
        );
        airk_store::validate_collection(collection)?;
        let source = absolute_existing_ancestor(root)?;
        let data = absolute_existing_ancestor(data)?;
        let executable = if executable.try_exists()? {
            ensure!(
                fs::metadata(executable)?.is_file(),
                "legacy executable path must name a file"
            );
            executable.canonicalize()?
        } else {
            absolute_existing_ancestor(
                executable
                    .parent()
                    .context("legacy executable parent missing")?,
            )?
            .join(
                executable
                    .file_name()
                    .context("legacy executable filename missing")?,
            )
        };
        ensure!(
            executable
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case(if cfg!(windows) {
                    "airk.exe"
                } else {
                    "airk"
                })),
            "legacy command must name an explicitly reviewed AIRK executable"
        );
        for path in [&source, &data, &executable] {
            ensure!(
                path.to_str().is_some_and(|value| value.len() <= 4096),
                "legacy review path exceeds budget"
            );
        }
        let args = vec![
            "--root".into(),
            source.to_str().context("non-UTF-8 source")?.into(),
            "--data-dir".into(),
            data.to_str().context("non-UTF-8 data")?.into(),
            "--collection".into(),
            collection.into(),
            "schedule".into(),
            "run".into(),
            "--watch".into(),
        ];
        let mut manager = Self::for_command(&data, executable, args, false)?;
        manager.legacy_review = true;
        Ok(manager)
    }
    pub(crate) fn legacy_definition_digest(&self) -> Result<Option<String>> {
        ensure!(self.legacy_review, "legacy review role required");
        if !self.check_owned()? {
            return Ok(None);
        }
        Ok(Some(format!(
            "{:x}",
            Sha256::digest(
                read_config(&self.definition)?.context("owned legacy definition missing")?
            )
        )))
    }
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
        let home = crate::user_paths::home()?;
        let config = crate::user_paths::configuration_base()?;
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
        let receipt = if shared {
            // On-demand startup must not require access to OS login configuration.
            // Only a private install-side receipt opts into manager ownership.
            Path::new(&data).join("startup-owner.json")
        } else {
            config.join("airk/services").join(format!("{id}.json"))
        };
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
            legacy_review: false,
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
        #[cfg(target_os = "macos")]
        if self.platform == Platform::Macos && self.id.starts_with("airksvc-") {
            return macos::Manager::new(self).plan();
        }
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
        #[cfg(target_os = "macos")]
        if self.platform == Platform::Macos && self.id.starts_with("airksvc-") {
            return macos::Manager::new(self).status();
        }
        let installed = self.check_owned()?;
        self.require_legacy_domain()?;
        let output = execute(&self.query(installed)?)?;
        let (loaded, running) = self.observed_state(&output);
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
        #[cfg(any(target_os = "macos", test))]
        if self.platform == Platform::Macos && self.legacy_review {
            self.verify_legacy_mac_job(&output.text)?;
            return Ok(Some(true));
        }
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
        #[cfg(target_os = "macos")]
        if self.platform == Platform::Macos && self.id.starts_with("airksvc-") {
            return macos::Manager::new(self).install();
        }
        self.require_registration_adapter()?;
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
        #[cfg(target_os = "macos")]
        if self.platform == Platform::Macos && self.id.starts_with("airksvc-") {
            return macos::Manager::new(self).start();
        }
        self.require_registration_adapter()?;
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
        #[cfg(target_os = "macos")]
        if self.platform == Platform::Macos && self.id.starts_with("airksvc-") {
            return macos::Manager::new(self).stop();
        }
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
        #[cfg(target_os = "macos")]
        if self.platform == Platform::Macos && self.id.starts_with("airksvc-") {
            return macos::Manager::new(self).uninstall();
        }
        ensure!(self.check_owned()?, "service is not installed");
        self.unregister_owned()?;
        ensure!(
            self.check_owned()?,
            "service ownership changed during uninstall"
        );
        fs::remove_file(&self.definition)?;
        fs::remove_file(&self.receipt)?;
        self.reload_user_manager()?;
        self.status()
    }
    fn unregister_owned(&self) -> Result<()> {
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
        Ok(())
    }
    fn reload_user_manager(&self) -> Result<()> {
        if self.platform == Platform::Linux {
            checked(&self.systemctl(&["daemon-reload"]))?;
        }
        Ok(())
    }
    pub(crate) fn legacy_paths(&self) -> Result<(&Path, &Path)> {
        ensure!(self.legacy_review, "legacy removal role required");
        Ok((&self.definition, &self.receipt))
    }
    pub(crate) fn disable_reviewed_legacy(&self) -> Result<()> {
        ensure!(self.legacy_review, "legacy removal role required");
        self.unregister_owned()
    }
    pub(crate) fn legacy_manager_state(&self) -> Result<(Option<bool>, Option<bool>)> {
        ensure!(self.legacy_review, "legacy removal role required");
        self.require_legacy_domain()?;
        Ok(self.observed_state(&execute(&self.query(false)?)?))
    }

    fn require_legacy_domain(&self) -> Result<()> {
        if self.platform == Platform::Macos && self.legacy_review {
            checked(&self.launchctl(&["print", &self.domain()]))?;
        }
        Ok(())
    }
    fn observed_state(&self, output: &Output) -> (Option<bool>, Option<bool>) {
        if self.platform == Platform::Macos && self.legacy_review && !output.success {
            return if output
                .text
                .contains(&format!("Could not find service \"{}\"", self.id))
            {
                (Some(false), Some(false))
            } else {
                (None, None)
            };
        }
        parse_status(self.platform, output)
    }
    #[cfg(any(target_os = "macos", test))]
    fn verify_legacy_mac_job(&self, text: &str) -> Result<()> {
        ensure!(
            self.legacy_review && self.platform == Platform::Macos,
            "legacy Mac ownership required"
        );
        ensure!(
            text.lines()
                .next()
                .is_some_and(|line| line.trim() == format!("{} = {{", self.target())),
            "Mac legacy label or user domain differs"
        );
        let executable = self
            .executable
            .to_str()
            .context("non-UTF-8 legacy executable")?;
        let arguments = std::iter::once(executable)
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>();
        crate::runtime_install::macos_login::verify_job_scope(
            text,
            &self.executable,
            &self.definition,
            &arguments,
        )
    }
    pub(crate) fn reload_reviewed_legacy(&self) -> Result<()> {
        ensure!(self.legacy_review, "legacy removal role required");
        self.reload_user_manager()
    }

    pub fn owned_registration_exists(&self) -> Result<bool> {
        #[cfg(target_os = "macos")]
        if self.platform == Platform::Macos && self.id.starts_with("airksvc-") {
            return macos::Manager::new(self).owned();
        }
        self.check_owned()
    }

    fn require_registration_adapter(&self) -> Result<()> {
        ensure!(
            !(self.id.starts_with("airksvc-") && self.platform == Platform::Macos),
            "shared LaunchAgent configuration proof is unavailable; retain on-demand mode until the native adapter is implemented and qualified"
        );
        Ok(())
    }

    /// Establish actual restart ownership before an upgrade can save an intent.
    /// Absent local receipts do not authorize altering an unrelated OS definition.
    pub(crate) fn owns_restart(&self) -> Result<bool> {
        #[cfg(target_os = "macos")]
        if self.platform == Platform::Macos && self.id.starts_with("airksvc-") {
            return macos::Manager::new(self).owns_restart();
        }
        if !self.check_owned()? {
            return Ok(false);
        }
        self.require_registration_adapter()?;
        let status = self.status()?;
        ensure!(
            status.loaded.is_some(),
            "startup manager state is unavailable"
        );
        if status.loaded == Some(true) {
            self.require_registered(&status)?;
        }
        Ok(true)
    }

    pub(crate) fn require_stopped_for_maintenance(&self) -> Result<()> {
        #[cfg(target_os = "macos")]
        if self.platform == Platform::Macos && self.id.starts_with("airksvc-") {
            return macos::Manager::new(self).require_stopped();
        }
        if !self.check_owned()? {
            return Ok(());
        }
        let status = self.status()?;
        ensure!(
            status.loaded.is_some(),
            "startup manager state is unavailable; stop and inspect it before launcher maintenance"
        );
        if status.loaded == Some(true) {
            self.require_registered(&status)?;
            ensure!(
                status.running == Some(false) && self.platform != Platform::Macos,
                "stop the owned login service before launcher maintenance"
            );
        }
        Ok(())
    }

    fn check_launchable(&self) -> Result<()> {
        ensure!(
            !self.legacy_review,
            "legacy review/removal never starts or executes the old command"
        );
        if self.id.starts_with("airksvc-") {
            let root = Path::new(&self.data);
            let launcher = crate::runtime_install::verified_launcher(root)?;
            let (selected, _) = crate::runtime_install::startup_selected(root)?;
            ensure!(
                crate::runtime_pair::digest_file(&launcher)?
                    == crate::runtime_pair::digest_file(&selected)?,
                "login startup requires the selected runtime's launcher; stop owners and run repair-launcher before starting it"
            );
        }
        Ok(())
    }
    fn require_registered(&self, status: &ServiceStatus) -> Result<()> {
        if self.id.starts_with("airksvc-") || self.legacy_review {
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
        if self.legacy_review {
            ensure!(
                definition == self.definition_bytes(&self.plan()?.definition),
                "legacy definition differs from the reviewed executable, source, data or collection"
            );
        }
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
fn config_directory(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        // SAFETY: geteuid takes no pointers and observes this process's user identity.
        let uid = unsafe { libc::geteuid() };
        if metadata.file_type().is_symlink() {
            ensure!(
                metadata.uid() == 0,
                "redirected user startup configuration directory"
            );
            return Ok(()); // Root-owned host aliases are checked again after canonicalization.
        }
        let mode = metadata.permissions().mode();
        ensure!(
            metadata.is_dir()
                && (metadata.uid() == uid || metadata.uid() == 0)
                && (mode & 0o022 == 0 || (metadata.uid() == 0 && mode & 0o1000 != 0)),
            "unsafe startup configuration directory: {}",
            path.display()
        );
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.is_dir() && metadata.file_attributes() & 0x400 == 0,
            "redirected startup configuration directory: {}",
            path.display()
        );
    }
    Ok(())
}

fn config_ancestry(path: &Path, create: bool) -> Result<()> {
    let parent = path
        .parent()
        .context("service configuration has no parent")?;
    ensure!(
        parent.is_absolute(),
        "service configuration requires an absolute parent"
    );
    let ancestors = parent
        .ancestors()
        .filter(|path| !path.as_os_str().is_empty())
        .collect::<Vec<_>>();
    let mut missing = Vec::new();
    for directory in ancestors.iter().rev() {
        match fs::symlink_metadata(directory) {
            Ok(metadata) => config_directory(directory, &metadata)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => missing.push(*directory),
            Err(error) => return Err(error.into()),
        }
    }
    // Resolve trusted host aliases, then check their actual target ancestry too.
    if let Some(existing) = ancestors.iter().find(|directory| directory.exists()) {
        for directory in existing.canonicalize()?.ancestors() {
            config_directory(directory, &fs::symlink_metadata(directory)?)?;
        }
    }
    if create {
        for directory in missing {
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                fs::DirBuilder::new().mode(0o700).create(directory)?;
            }
            #[cfg(windows)]
            crate::local_ipc::prepare_root(directory)?;
            config_directory(directory, &fs::symlink_metadata(directory)?)?;
        }
    }
    Ok(())
}

fn read_config(path: &Path) -> Result<Option<Vec<u8>>> {
    config_ancestry(path, false)?;
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= MAX_CONFIG,
        "invalid, linked or oversized service configuration"
    );
    crate::runtime_install::safe_file(path)?;
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
    config_ancestry(path, true)?;
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
    execute_with_timeout(invocation, Duration::from_secs(30))
}
#[cfg(any(target_os = "macos", test))]
pub(crate) fn execute_native_program(program: &Path, args: &[&str]) -> Result<bool> {
    Ok(execute(&Invocation::new(program, args))?.success)
}
#[cfg(any(target_os = "macos", test))]
pub(crate) fn execute_native_observation(program: &Path, args: &[&str]) -> Result<(bool, String)> {
    let output = execute(&Invocation::new(program, args))?;
    Ok((output.success, output.text))
}
#[cfg(any(target_os = "macos", test))]
pub(crate) fn verify_macos_login_intent(
    root: &Path,
    identity: &str,
    channel: crate::runtime_install::RuntimeChannel,
) -> Result<()> {
    macos::verify_intent(root, identity, channel)
}
fn execute_with_timeout(invocation: &Invocation, timeout: Duration) -> Result<Output> {
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
    let deadline = Instant::now() + timeout;
    let mut child = crate::native_process::ContainedChild::spawn(&mut command)
        .with_context(|| format!("start service manager {}", invocation.program.display()))?;
    let input = if let Some(bytes) = &invocation.input {
        let bytes = bytes.clone();
        let mut input = child.stdin.take().context("missing manager input")?;
        let (sender, completion) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("airk-manager-input".into())
            .spawn(move || {
                let _ = sender.send(input.write_all(&bytes));
            })?;
        Some(completion)
    } else {
        None
    };
    let (sender, receiver) = mpsc::channel();
    for mut stream in [
        Box::new(child.stdout.take().context("missing manager stdout")?) as Box<dyn Read + Send>,
        Box::new(child.stderr.take().context("missing manager stderr")?),
    ] {
        let sender = sender.clone();
        thread::Builder::new()
            .name("airk-manager-output".into())
            .spawn(move || {
                let mut retained = Vec::new();
                let mut block = [0; 4096];
                let result = loop {
                    match stream.read(&mut block) {
                        Ok(0) => break Ok(retained),
                        Ok(read) if retained.len() + read <= 65536 => {
                            retained.extend_from_slice(&block[..read])
                        }
                        Ok(_) => break Err(std::io::Error::other("manager output exceeds 64 KiB")),
                        Err(error) => break Err(error),
                    }
                };
                let _ = sender.send(result);
            })?;
    }
    drop(sender);
    let success = loop {
        if let Some(status) = child.try_wait()? {
            break status.success();
        }
        if Instant::now() >= deadline {
            bail!(
                "service manager exceeded its command deadline; inspect OS service status before retrying"
            );
        }
        thread::sleep(Duration::from_millis(20));
    };
    if let Some(input) = input {
        input
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .context("service manager input did not complete before its deadline")??;
    }
    let mut text = String::new();
    for _ in 0..2 {
        let bytes = receiver
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .context("service manager output did not close before its deadline")??;
        text.push_str(&String::from_utf8_lossy(&bytes));
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
    #[cfg(windows)]
    #[test]
    #[ignore = "requires explicit owned native Task Scheduler qualification"]
    fn reviewed_legacy_task_removal_preserves_sources_and_replays_receipt() -> Result<()> {
        use crate::legacy_startup::{Target, remove_target, review_target};
        let owner = tempfile::tempdir()?;
        let root = owner.path().canonicalize()?;
        let source = root.join("source");
        let data = root.join("old-data");
        let control = root.join("control");
        for path in [&source, &data, &control] {
            fs::create_dir(path)?;
        }
        fs::write(source.join("original.txt"), b"original source")?;
        fs::write(data.join("settings.json"), b"saved settings")?;
        fs::write(data.join("index.dat"), b"retained old index")?;
        let executable = root.join("airk.exe");
        fs::write(&executable, b"deliberately invalid executable; never run")?;
        let target = Target {
            root: source.clone(),
            data_dir: data.clone(),
            collection: "owned-legacy-fixture".into(),
            executable,
        };
        airk_store::service::registry::ServiceRegistry::open(
            &control,
            airk_core::identity::Incarnation::from_bytes([61; 16]),
        )?;
        let manager = ServiceManager::for_reviewed_legacy(
            &target.root,
            &target.data_dir,
            &target.collection,
            &target.executable,
        )?;
        ensure!(
            manager.status()?.loaded == Some(false),
            "fixture task already exists"
        );
        let plan = manager.plan()?;
        let definition = manager.definition_bytes(&plan.definition);
        write_new(&manager.definition, &definition)?;
        let receipt = Receipt {
            schema: 1,
            id: manager.id.clone(),
            data_root: manager.data.clone(),
            definition_sha256: format!("{:x}", Sha256::digest(&definition)),
        };
        write_new(&manager.receipt, &serde_json::to_vec(&receipt)?)?;
        struct Cleanup(ServiceManager);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                if let Err(error) = self.0.uninstall()
                    && self.0.definition.exists()
                {
                    eprintln!("owned legacy fixture cleanup failed: {error:#}");
                }
            }
        }
        let cleanup = Cleanup(manager);
        let manager = &cleanup.0;
        checked(&manager.schtasks(&[
            "/Create",
            "/TN",
            &manager.id,
            "/XML",
            manager.definition.to_str().context("fixture task path")?,
        ])?)?;
        let status = manager.status()?;
        assert_eq!(status.running, Some(false));
        assert_eq!(status.registered_definition_matches, Some(true));
        let review = serde_json::to_value(review_target(&control, &target)?)?;
        let approval = serde_json::from_value(review["approval"].clone())?;
        let result = remove_target(&control, &target, approval)?;
        assert_eq!(result["startup_removed"], true);
        assert_eq!(manager.status()?.loaded, Some(false));
        assert!(!manager.definition.exists() && !manager.receipt.exists());
        assert_eq!(remove_target(&control, &target, approval)?, result);
        assert_eq!(fs::read(source.join("original.txt"))?, b"original source");
        assert_eq!(fs::read(data.join("settings.json"))?, b"saved settings");
        assert_eq!(fs::read(data.join("index.dat"))?, b"retained old index");
        Ok(())
    }
    #[test]
    fn manager_command_fixture() -> Result<()> {
        let Ok(mode) = std::env::var("AIRK_MANAGER_COMMAND_FIXTURE") else {
            return Ok(());
        };
        match mode.as_str() {
            "blocked-input" => thread::sleep(Duration::from_secs(60)),
            #[cfg(target_os = "linux")]
            "leave-descendant" => {
                let descendant = Command::new(std::env::current_exe()?)
                    .args([
                        "--exact",
                        "service_manager::tests::manager_command_fixture",
                        "--nocapture",
                    ])
                    .env("AIRK_MANAGER_COMMAND_FIXTURE", "blocked-input")
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()?;
                fs::write(
                    std::env::var_os("AIRK_MANAGER_TEST_PID").unwrap(),
                    descendant.id().to_string(),
                )?;
                let release = std::env::var_os("AIRK_MANAGER_TEST_RELEASE").unwrap();
                let until = Instant::now() + Duration::from_secs(10);
                while !Path::new(&release).exists() {
                    ensure!(Instant::now() < until, "fixture release expired");
                    thread::sleep(Duration::from_millis(5));
                }
                // Test owner must prove this child's death despite closed pipes.
            }
            "oversized-output" => std::io::stdout().write_all(&vec![b'x'; 70 * 1024])?,
            "echo" => {
                let mut bytes = Vec::new();
                std::io::stdin().read_to_end(&mut bytes)?;
                ensure!(bytes == b"owned fixture input", "fixture input changed");
                std::io::stdout().write_all(b"owned fixture output")?;
            }
            _ => bail!("unknown command fixture"),
        }
        Ok(())
    }

    fn command_fixture(mode: &str) -> Result<Invocation> {
        let mut invocation = Invocation::new(
            std::env::current_exe()?,
            &[
                "--exact",
                "service_manager::tests::manager_command_fixture",
                "--nocapture",
            ],
        );
        invocation
            .env
            .insert("AIRK_MANAGER_COMMAND_FIXTURE".into(), mode.into());
        Ok(invocation)
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn manager_command_completion_drains_descendants_with_closed_pipes() -> Result<()> {
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
        let directory = tempfile::tempdir()?;
        let pid_path = directory.path().join("pid");
        let release = directory.path().join("release");
        let mut invocation = command_fixture("leave-descendant")?;
        invocation.env.insert(
            "AIRK_MANAGER_TEST_PID".into(),
            pid_path.display().to_string(),
        );
        invocation.env.insert(
            "AIRK_MANAGER_TEST_RELEASE".into(),
            release.display().to_string(),
        );
        let execution =
            thread::spawn(move || execute_with_timeout(&invocation, Duration::from_secs(10)));
        let until = Instant::now() + Duration::from_secs(5);
        let pid = loop {
            if let Ok(text) = fs::read_to_string(&pid_path)
                && let Ok(pid) = text.parse::<libc::pid_t>()
            {
                break pid;
            }
            ensure!(Instant::now() < until, "fixture did not create descendant");
            thread::sleep(Duration::from_millis(5));
        };
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
        ensure!(fd >= 0, "native descendant handle unavailable");
        let handle = unsafe { OwnedFd::from_raw_fd(fd as i32) };
        let mut observation = libc::pollfd {
            fd: handle.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        assert_eq!(unsafe { libc::poll(&mut observation, 1, 0) }, 0);
        fs::write(release, b"release")?;
        assert!(execution.join().unwrap()?.success);
        assert_eq!(unsafe { libc::poll(&mut observation, 1, 0) }, 1);
        assert_ne!(observation.revents & libc::POLLIN, 0);
        Ok(())
    }

    #[test]
    fn manager_command_deadline_covers_blocked_stdin_and_output_is_not_silently_truncated()
    -> Result<()> {
        fn diagnostic(stage: &str) {
            if std::env::var_os("AIRK_PROCESS_DIAGNOSTICS").is_some() {
                eprintln!("airk-manager fixture: {stage}");
            }
        }
        let mut blocked = command_fixture("blocked-input")?;
        blocked.input = Some(vec![b'x'; MAX_CONFIG as usize]);
        diagnostic("blocked-input command started");
        let began = Instant::now();
        assert!(execute_with_timeout(&blocked, Duration::from_millis(250)).is_err());
        diagnostic("blocked-input command returned");
        assert!(began.elapsed() < Duration::from_secs(5));
        diagnostic("oversized-output command started");
        assert!(
            execute_with_timeout(
                &command_fixture("oversized-output")?,
                Duration::from_secs(5)
            )
            .is_err()
        );
        diagnostic("oversized-output command returned");
        let mut echo = command_fixture("echo")?;
        echo.input = Some(b"owned fixture input".to_vec());
        diagnostic("echo command started");
        let output = execute_with_timeout(&echo, Duration::from_secs(5))?;
        diagnostic("echo command returned");
        assert!(output.success && output.text.contains("owned fixture output"));
        Ok(())
    }

    #[test]
    fn startup_configuration_preserves_aliases_and_creates_only_private_missing_parents()
    -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().canonicalize()?;
        let path = root.join("new/configuration/fixture.conf");
        write_new(&path, b"Owned configuration fixture.")?;
        assert_eq!(
            read_config(&path)?.unwrap(),
            b"Owned configuration fixture."
        );
        let alias = root.join("alias.conf");
        fs::hard_link(&path, &alias)?;
        assert!(read_config(&alias).is_err());
        assert!(write_new(&alias, b"Do not replace.").is_err());
        assert_eq!(fs::read(&path)?, b"Owned configuration fixture.");
        #[cfg(unix)]
        {
            use std::os::unix::fs::{PermissionsExt, symlink};
            assert_eq!(
                fs::metadata(path.parent().unwrap())?.permissions().mode() & 0o777,
                0o700
            );
            let existing = root.join("existing-user-units");
            fs::create_dir(&existing)?;
            fs::set_permissions(&existing, fs::Permissions::from_mode(0o755))?;
            write_new(
                &existing.join("unit.service"),
                b"Owned existing-directory fixture.",
            )?;
            assert_eq!(fs::metadata(&existing)?.permissions().mode() & 0o777, 0o755);
            // Ordinary users cannot redirect startup writes into another source.
            if unsafe { libc::geteuid() } != 0 {
                let redirected = root.join("redirected");
                symlink(&existing, &redirected)?;
                assert!(write_new(&redirected.join("nested/new.service"), b"Refused.").is_err());
                assert!(!existing.join("nested").exists());
            }
            fs::set_permissions(&existing, fs::Permissions::from_mode(0o777))?;
            assert!(write_new(&existing.join("refused.service"), b"Refused.").is_err());
            assert!(!existing.join("refused.service").exists());
        }
        Ok(())
    }

    #[test]
    fn shared_on_demand_ownership_is_read_from_the_private_installation() -> Result<()> {
        let root = tempfile::tempdir()?;
        let mut service = ServiceManager::for_runtime(root.path())?;
        assert_eq!(
            service.receipt,
            root.path().canonicalize()?.join("startup-owner.json")
        );
        // An absent install-side opt-in must not inspect unavailable OS config.
        service.definition = root.path().join("unavailable/definition");
        fs::write(
            root.path().join("unavailable"),
            b"Not an accessible directory.",
        )?;
        assert!(!service.owns_restart()?);
        Ok(())
    }
    #[test]
    fn legacy_mac_job_requires_exact_user_label_origin_program_and_all_arguments() -> Result<()> {
        let root = tempfile::tempdir()?;
        let mut service = fixture(Platform::Macos, root.path());
        service.legacy_review = true;
        service.args = vec![
            "--root".into(),
            "/fixture source/é".into(),
            "--data-dir".into(),
            "/fixture data".into(),
            "--collection".into(),
            "example".into(),
            "schedule".into(),
            "run".into(),
            "--watch".into(),
        ];
        let argv = std::iter::once(service.executable.to_str().unwrap())
            .chain(service.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join("\n\t\t");
        let text = format!(
            "{} = {{\n\tpath = {}\n\tprogram = {}\n\targuments = {{\n\t\t{argv}\n\t}}\n\tstate = running\n}}\n",
            service.target(),
            service.definition.display(),
            service.executable.display()
        );
        let observed = Output {
            success: true,
            text: text.clone(),
        };
        assert_eq!(service.verify_registration(&observed)?, Some(true));
        assert_eq!(service.observed_state(&observed), (Some(true), Some(true)));
        for changed in [
            text.replace("gui/1000/", "gui/1001/"),
            text.replace("airk-fixture =", "foreign-fixture ="),
            text.replace(
                &format!("path = {}", service.definition.display()),
                "path = /foreign.plist",
            ),
            text.replace("program = /fixture", "program = /foreign"),
            text.replace("\t\t--watch", "\t\t--foreign"),
            text.replace(
                "\tstate = running",
                "\tprogram = /duplicate\n\tstate = running",
            ),
            text.replace("\targuments = {", "\tignored = {\n\targuments = {") + "}\n",
            "x".repeat(65_537),
        ] {
            assert!(service.verify_legacy_mac_job(&changed).is_err());
        }
        for (message, expected) in [
            (
                "Could not find service \"airk-fixture\" in domain",
                (Some(false), Some(false)),
            ),
            ("Could not find service \"foreign\" in domain", (None, None)),
            ("Could not find domain for user", (None, None)),
        ] {
            assert_eq!(
                service.observed_state(&Output {
                    success: false,
                    text: message.into()
                }),
                expected
            );
        }
        service.legacy_review = false;
        assert!(service.verify_legacy_mac_job(&text).is_err());
        Ok(())
    }
    #[test]
    fn unavailable_shared_launchagent_proof_refuses_registration_before_any_write() -> Result<()> {
        let root = tempfile::tempdir()?;
        let mut service = fixture(Platform::Macos, root.path());
        service.id = "airksvc-native-proof-fixture".into();
        assert!(!service.owns_restart()?);
        assert!(service.install().is_err());
        assert!(!service.definition.exists() && !service.receipt.exists());
        Ok(())
    }
    pub(super) fn fixture(platform: Platform, root: &Path) -> ServiceManager {
        ServiceManager {
            legacy_review: false,
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
    fn reviewed_legacy_role_refuses_launch_and_requires_exact_saved_and_os_configuration()
    -> Result<()> {
        let root = tempfile::tempdir()?;
        let mut service = fixture(Platform::Linux, root.path());
        service.legacy_review = true;
        assert!(service.install().is_err());
        assert!(service.start().is_err());
        assert!(!service.definition.exists() && !service.receipt.exists());
        let definition = service.definition_bytes(&service.plan()?.definition);
        write_new(&service.definition, &definition)?;
        let mut receipt = Receipt {
            schema: 1,
            id: service.id.clone(),
            data_root: service.data.clone(),
            definition_sha256: format!("{:x}", Sha256::digest(&definition)),
        };
        write_new(&service.receipt, &serde_json::to_vec(&receipt)?)?;
        assert!(service.legacy_definition_digest()?.is_some());
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
        assert!(service.require_registered(&status).is_err());
        status.registered_definition_matches = Some(true);
        service.require_registered(&status)?;
        let foreign = b"foreign source or collection definition";
        fs::write(&service.definition, foreign)?;
        receipt.definition_sha256 = format!("{:x}", Sha256::digest(foreign));
        fs::write(&service.receipt, serde_json::to_vec(&receipt)?)?;
        assert!(service.legacy_definition_digest().is_err());
        assert_eq!(fs::read(&service.definition)?, foreign);
        Ok(())
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
$airkFixtureDefinition.XmlText = $env:AIRK_FIXTURE_XML.Replace('__AIRK_CURRENT_ACCOUNT__', [System.Security.Principal.WindowsIdentity]::GetCurrent().Name)
$airkFixtureTask = [pscustomobject]@{ State = [int] $env:AIRK_FIXTURE_STATE; Definition = $airkFixtureDefinition }
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
        for (xml, matches, state, running) in [
            (expected.clone(), true, 3, Some(false)),
            (
                expected.replacen(
                    &format!("<UserId>{}</UserId>", service.principal),
                    "<UserId>__AIRK_CURRENT_ACCOUNT__</UserId>",
                    1,
                ),
                true,
                3,
                Some(false),
            ),
            (
                expected.replacen(
                    &format!("<UserId>{}</UserId>", service.principal),
                    if service.principal == "S-1-5-18" {
                        "<UserId>S-1-5-19</UserId>"
                    } else {
                        "<UserId>S-1-5-18</UserId>"
                    },
                    1,
                ),
                false,
                3,
                Some(false),
            ),
            (expected.clone(), true, 2, Some(true)),
            (expected.clone(), true, 0, None),
            (
                expected.replace(
                    "<Description>",
                    "<Author>generated by OS</Author><Description>",
                ),
                true,
                3,
                Some(false),
            ),
            (
                expected.replace("<Command>", "<Command>changed-"),
                false,
                3,
                Some(false),
            ),
            (
                expected.replace(
                    "<RunLevel>LeastPrivilege</RunLevel>",
                    "<RunLevel>HighestAvailable</RunLevel>",
                ),
                false,
                3,
                Some(false),
            ),
            (
                expected.replace(
                    "</Actions>",
                    "<Exec><Command>other.exe</Command></Exec></Actions>",
                ),
                false,
                3,
                Some(false),
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
            invocation
                .env
                .insert("AIRK_FIXTURE_STATE".into(), state.to_string());
            invocation.input = Some(expected.as_bytes().to_vec());
            let output = execute(&invocation)?;
            ensure!(
                output.success,
                "native schema fixture failed: {}",
                output.text
            );
            let result: serde_json::Value = serde_json::from_str(&output.text)?;
            assert_eq!(result["loaded"], true);
            assert_eq!(result["running"], serde_json::to_value(running)?);
            assert_eq!(result["definition_matches"], matches);
        }
        Ok(())
    }
}
