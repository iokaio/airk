// SPDX-License-Identifier: Apache-2.0
//! Isolated native fixture, never an actual user config or ignore file.
use airk_process::bounded_command::{Invocation, execute_with_timeout};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub(super) struct Editable {
    _temp: tempfile::TempDir,
    path: PathBuf,
}
impl Editable {
    pub fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().canonicalize().unwrap().join("private");
        airk_ipc::endpoint::prepare_root(&path).unwrap();
        let value = Self { _temp: temp, path };
        value.git(&["init", "--quiet", "--template="]);
        let excludes = value.path.join("empty-excludes");
        fs::write(&excludes, b"").unwrap();
        let exclude_path = excludes.to_str().unwrap();
        value.git(&[
            "config",
            "core.excludesFile",
            exclude_path.strip_prefix(r"\\?\").unwrap_or(exclude_path),
        ]);
        fs::write(value.path.join(".gitignore"), b".airk/\n").unwrap();
        fs::write(value.path.join("package.json"), br#"{"name":"airk-retention-fixture","version":"0.0.0","packageManager":"npm@11.12.1","files":["payload"],"main":"payload/main.js"}"#).unwrap();
        fs::create_dir(value.path.join("payload")).unwrap();
        fs::write(value.path.join("payload/main.js"), b"fixture\n").unwrap();
        value
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn git(&self, arguments: &[&str]) {
        let path = self.path.to_str().unwrap();
        let path = path.strip_prefix(r"\\?\").unwrap_or(path);
        let mut args = vec![
            "--no-optional-locks",
            "-c",
            "core.fsmonitor=false",
            "-C",
            path,
        ];
        args.extend_from_slice(arguments);
        let output = execute_with_timeout(
            &Invocation::new(super::eligibility::git_program(&self.path).unwrap(), &args),
            Duration::from_secs(5),
        )
        .unwrap();
        assert!(output.success, "fixture Git setup failed: {}", output.text);
    }
}
