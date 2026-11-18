// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::path::PathBuf;

struct Fixture {
    program: OpenedDirectory,
    bin: OpenedDirectory,
    home: PathBuf,
    program_path: PathBuf,
    bin_path: PathBuf,
    target: PathBuf,
    id: Incarnation,
    // Drop held directory handles before the temporary root cleanup.
    _temporary: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let home = temporary.path().to_owned();
        let path = home.join("program");
        endpoint::prepare_root(&path).unwrap();
        let program = OpenedDirectory::open(&path).unwrap();
        let bin = endpoint::create_root_at(&program, OsStr::new("bin")).unwrap();
        let bin_path = path.join("bin");
        let target = bin_path.join("airkctl.exe");
        Self {
            _temporary: temporary,
            program,
            bin,
            home,
            program_path: path,
            bin_path,
            target,
            id: Incarnation::from_bytes([0x43; 16]),
        }
    }
    fn install(&self) -> Result<String> {
        install(
            &self.program,
            &self.home,
            &self.target,
            self.id,
            "rk",
            |_| Ok(()),
        )
    }
    fn remove(&self) -> Result<String> {
        remove(&self.program, &self.home, &self.target, self.id)
    }
    fn available(&self, alias: &str) -> Result<bool> {
        available(&self.program, &self.home, &self.target, self.id, alias)
    }
}
fn create(root: &OpenedDirectory, name: &str, bytes: &[u8]) {
    let mut file = endpoint::create_private_file_at(root, OsStr::new(name)).unwrap();
    file.write_all(bytes).unwrap();
    file.sync_all().unwrap();
}

#[test]
fn windows_alias_recovers_each_publication_boundary_and_removes_exact_owned_files() {
    for boundary in ["receipt", "staged", "published"] {
        let f = Fixture::new();
        let error = install(&f.program, &f.home, &f.target, f.id, "rk", |stage| {
            ensure!(stage != boundary, "injected failure");
            Ok(())
        });
        assert!(error.is_err());
        assert!(read(&f.program, RECEIPT).unwrap().is_some());
        assert!(f.available("rk").unwrap());
        f.install().unwrap();
        assert_eq!(
            read(&f.bin, "rk.cmd").unwrap(),
            Some(launcher(&f.target).unwrap())
        );
        f.remove().unwrap();
        assert!(!exists(&f.bin_path, "rk.cmd").unwrap());
        assert!(read(&f.program, RECEIPT).unwrap().is_none());
        assert!(f.bin_path.is_dir());
        f.remove().unwrap();
    }
}

#[test]
fn windows_alias_preserves_identical_unowned_wrapper_and_stage_without_receipt() {
    for stage in [false, true] {
        let f = Fixture::new();
        let name = if stage {
            stage_name(f.id, "rk")
        } else {
            "rk.cmd".into()
        };
        let body = launcher(&f.target).unwrap();
        create(&f.bin, &name, &body);
        assert!(!f.available("rk").unwrap());
        assert!(f.install().is_err());
        assert_eq!(read(&f.bin, &name).unwrap(), Some(body));
        assert!(read(&f.program, RECEIPT).unwrap().is_none());
    }
}

#[test]
fn windows_alias_fresh_receipt_does_not_adopt_a_raced_stage() {
    let f = Fixture::new();
    let body = launcher(&f.target).unwrap();
    let stage_name = stage_name(f.id, "rk");
    let result = install(&f.program, &f.home, &f.target, f.id, "rk", |stage| {
        if stage == "receipt" {
            create(&f.bin, &stage_name, &body);
        }
        Ok(())
    });
    assert!(result.is_err());
    assert!(!exists(&f.bin_path, "rk.cmd").unwrap());
    assert_eq!(read(&f.bin, &stage_name).unwrap(), Some(body));
}

#[test]
fn windows_alias_repairs_owned_changes_but_uninstall_preserves_them() {
    let f = Fixture::new();
    f.install().unwrap();
    let receipt = read(&f.program, RECEIPT).unwrap().unwrap();
    std::fs::write(f.bin_path.join("rk.cmd"), b"changed").unwrap();
    assert!(f.available("rk").unwrap());
    assert!(f.remove().is_err());
    assert_eq!(read(&f.bin, "rk.cmd").unwrap(), Some(b"changed".to_vec()));
    assert_eq!(read(&f.program, RECEIPT).unwrap(), Some(receipt));
    f.install().unwrap();
    assert_eq!(
        read(&f.bin, "rk.cmd").unwrap(),
        Some(launcher(&f.target).unwrap())
    );
    std::fs::write(f.program_path.join(RECEIPT), b"{}").unwrap();
    assert!(f.remove().is_err());
    assert!(exists(&f.bin_path, "rk.cmd").unwrap());
}

#[test]
fn windows_alias_preserves_ads_and_hardlinked_wrappers() {
    for hardlink in [false, true] {
        let f = Fixture::new();
        f.install().unwrap();
        if hardlink {
            std::fs::hard_link(f.bin_path.join("rk.cmd"), f.bin_path.join("alias-link")).unwrap();
        } else {
            std::fs::write(f.bin_path.join("rk.cmd:foreign"), b"retained stream").unwrap();
        }
        assert!(f.remove().is_err());
        assert!(exists(&f.bin_path, "rk.cmd").unwrap());
        assert!(read(&f.program, RECEIPT).unwrap().is_some());
    }
}

#[test]
fn windows_alias_refuses_changed_stage_after_receipt_and_selected_name_changes() {
    let f = Fixture::new();
    let result = install(&f.program, &f.home, &f.target, f.id, "rk", |stage| {
        ensure!(stage != "receipt", "stop before staging");
        Ok(())
    });
    assert!(result.is_err());
    let stage = stage_name(f.id, "rk");
    create(&f.bin, &stage, b"partial");
    assert!(f.install().is_err());
    assert!(f.remove().is_err());
    assert_eq!(read(&f.bin, &stage).unwrap(), Some(b"partial".to_vec()));
    assert!(
        install(
            &f.program,
            &f.home,
            &f.target,
            f.id,
            "alternate",
            |_| Ok(())
        )
        .is_err()
    );
    assert!(!exists(&f.bin_path, "alternate.cmd").unwrap());
}

#[test]
fn windows_alias_checks_selection_names_and_installation_boundaries() {
    let f = Fixture::new();
    assert!(f.available("rk1").unwrap());
    for alias in ["../other", "airkctl", "CON", "with space"] {
        assert!(f.available(alias).is_err());
        assert!(install(&f.program, &f.home, &f.target, f.id, alias, |_| Ok(())).is_err());
    }
    let other = Fixture::new();
    assert!(available(&f.program, &f.home, &other.target, f.id, "rk").is_err());
    assert!(
        available(
            &f.program,
            &f.home,
            &f.home.join("absent/bin/airkctl.exe"),
            f.id,
            "rk"
        )
        .is_err()
    );
    let external = f.home.join("external/airkctl.exe");
    assert!(install(&f.program, &f.home, &external, f.id, "rk", |_| Ok(())).is_err());
    assert!(read(&f.program, RECEIPT).unwrap().is_none());
}

#[test]
fn windows_alias_probe_allows_absent_bin_without_creating_it() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path();
    let path = home.join("program");
    endpoint::prepare_root(&path).unwrap();
    let program = OpenedDirectory::open(&path).unwrap();
    let target = path.join("bin/airkctl.exe");
    let id = Incarnation::from_bytes([9; 16]);
    assert!(available(&program, home, &target, id, "rk").unwrap());
    assert!(!path.join("bin").exists());
    assert!(read(&program, RECEIPT).unwrap().is_none());
    assert!(
        available(
            &program,
            home,
            &path.join("missing/../other/airkctl.exe"),
            id,
            "rk"
        )
        .is_err()
    );
    create(&program, RECEIPT, b"{}");
    assert!(available(&program, home, &target, id, "rk").is_err());
}

#[test]
fn windows_alias_repair_refuses_destination_or_stage_drift_before_replace() {
    for drift_stage in [false, true] {
        let f = Fixture::new();
        f.install().unwrap();
        std::fs::write(f.bin_path.join("rk.cmd"), b"owned modification").unwrap();
        let result = install(&f.program, &f.home, &f.target, f.id, "rk", |phase| {
            if phase == "staged" {
                let name = if drift_stage {
                    stage_name(f.id, "rk")
                } else {
                    "rk.cmd".into()
                };
                std::fs::write(f.bin_path.join(name), b"raced bytes")?;
            }
            Ok(())
        });
        assert!(result.is_err());
        let retained = if drift_stage {
            b"owned modification".as_slice()
        } else {
            b"raced bytes".as_slice()
        };
        assert_eq!(read(&f.bin, "rk.cmd").unwrap(), Some(retained.to_vec()));
        assert!(read(&f.program, RECEIPT).unwrap().is_some());
    }
}

#[test]
#[ignore = "requires explicitly provisioned AIRK_RK_TEST_PYTHON and AIRK_RK_TEST_PYTHON_PREFIX; no production binary is added"]
fn windows_alias_cmd_forwards_quoted_arguments_stdio_exit_and_literal_target_path() -> Result<()> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    let python = PathBuf::from(
        std::env::var_os("AIRK_RK_TEST_PYTHON")
            .context("set AIRK_RK_TEST_PYTHON to the reviewed Windows Python executable")?,
    );
    let prefix = PathBuf::from(
        std::env::var_os("AIRK_RK_TEST_PYTHON_PREFIX")
            .context("set AIRK_RK_TEST_PYTHON_PREFIX to its reviewed base installation")?,
    );
    ensure!(
        python.is_absolute() && prefix.is_absolute(),
        "Python fixture paths must be absolute"
    );
    private_record::require_regular_unlinked(&python)?;
    ensure!(
        prefix.join("Lib/encodings/__init__.py").is_file(),
        "Python fixture stdlib missing"
    );
    let temporary = tempfile::tempdir()?;
    let home = temporary.path();
    let path = home.join("program %NAME% ! & 雪 space");
    endpoint::prepare_root(&path)?;
    let program = OpenedDirectory::open(&path)?;
    let bin = endpoint::create_root_at(&program, OsStr::new("bin"))?;
    let bin_path = path.join("bin");
    let target = bin_path.join("airkctl.exe");
    std::fs::copy(&python, &target)?;
    let mut python_dll = false;
    for entry in std::fs::read_dir(&prefix)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_text = name.to_str().context("non-UTF8 Python DLL filename")?;
        if name_text.to_ascii_lowercase().ends_with(".dll") {
            private_record::require_regular_unlinked(&entry.path())?;
            std::fs::copy(entry.path(), bin_path.join(&name))?;
            python_dll |= name_text.to_ascii_lowercase().starts_with("python");
        }
    }
    ensure!(python_dll, "Python fixture has no adjacent interpreter DLL");
    create(
        &bin,
        "probe.py",
        br#"import json,os,sys
body={'argv':sys.argv[1:],'stdin':sys.stdin.read(),'executable':os.path.basename(sys.executable)}
print(json.dumps(body,ensure_ascii=True))
sys.stderr.buffer.write(b'owned stderr\n')
sys.exit(7)
"#,
    );
    install(
        &program,
        home,
        &target,
        Incarnation::from_bytes([0x64; 16]),
        "rk",
        |_| Ok(()),
    )?;
    let system =
        PathBuf::from(std::env::var_os("SystemRoot").context("Windows system root missing")?);
    ensure!(system.is_absolute(), "Windows system root is not absolute");
    let mut command = Command::new(system.join("System32").join("cmd.exe"));
    command
        .args(["/d", "/e:off", "/v:off", "/s", "/c"])
        .raw_arg(
            r#"".\rk.cmd -B -S probe.py "" "two words" "雪" "A&B" "bang!" "100%" "quote\"inside"""#,
        )
        .current_dir(&bin_path)
        .env("NAME", "must-not-expand-in-target")
        .env("ERRORLEVEL", "99")
        .env("PYTHONHOME", &prefix)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env_remove("PYTHONPATH")
        .env_remove("PYTHONSTARTUP")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    child
        .stdin
        .take()
        .context("fixture stdin missing")?
        .write_all(b"input body\n")?;
    let output = child.wait_with_output()?;
    ensure!(
        output.status.code() == Some(7),
        "CMD forwarding failed: {:?}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(
        actual,
        serde_json::json!({"argv":["","two words","雪","A&B","bang!","100%","quote\"inside"], "stdin":"input body\n", "executable":"airkctl.exe"})
    );
    assert_eq!(output.stderr, b"owned stderr\n");
    Ok(())
}
