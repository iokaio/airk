// SPDX-License-Identifier: Apache-2.0
use super::*;
#[test]
fn actual_macos_system_control_anchor_is_metadata_only_and_generic_source_stays_strict()
-> Result<()> {
    let anchor = SystemEtc::capture()?.context("actual macOS /etc OS alias required")?;
    anchor.recheck()?;
    anchor.qualify(Path::new("gitconfig"), false)?;
    assert!(crate::native_name_boundary::qualify(Path::new("/"), "etc/gitconfig", false).is_err());
    anchor.recheck()
}
#[test]
fn macos_private_control_hook_preserves_bound_bytes_and_refuses_leaf_and_directory_aliases()
-> Result<()> {
    let owned = tempfile::tempdir()?;
    let root = owned.path().canonicalize()?;
    let parent = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&root)?;
    let file = root.join("control");
    std::fs::write(&file, b"actual\n")?;
    assert_eq!(
        crate::source_read::read_control_at(&parent, Path::new("control"), &file, 64, 65)?,
        b"actual\n"
    );
    std::os::unix::fs::symlink(&file, root.join("leaf-alias"))?;
    assert!(
        crate::source_read::read_control_at(
            &parent,
            Path::new("leaf-alias"),
            &root.join("leaf-alias"),
            64,
            65
        )
        .is_err()
    );
    let directory = root.join("directory");
    std::fs::create_dir(&directory)?;
    std::fs::write(directory.join("control"), b"outside\n")?;
    std::os::unix::fs::symlink(&directory, root.join("directory-alias"))?;
    assert!(
        crate::source_read::read_control_at(
            &parent,
            Path::new("directory-alias/control"),
            &root.join("directory-alias/control"),
            64,
            65
        )
        .is_err()
    );
    assert!(
        crate::source_read::read_control_at(&parent, Path::new("../control"), &file, 64, 65)
            .is_err()
    );
    // O_NONBLOCK must return a held FIFO descriptor without waiting for a
    // writer; the shared regular-file guard refuses BEFORE any byte read.
    let fifo = root.join("fifo-control");
    let fifo_name = CString::new(fifo.as_os_str().as_bytes())?;
    // SAFETY: fresh owned temporary name, no privileges or existing path changes.
    assert_eq!(unsafe { libc::mkfifo(fifo_name.as_ptr(), 0o600) }, 0);
    assert!(
        crate::source_read::read_control_at(&parent, Path::new("fifo-control"), &fifo, 64, 65)
            .is_err()
    );
    Ok(())
}
