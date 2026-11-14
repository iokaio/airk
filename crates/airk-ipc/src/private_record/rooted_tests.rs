// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_process::directory::OpenedDirectory;

#[test]
fn rooted_private_publication_is_bounded_and_never_clobbers_without_permission() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("private");
    crate::endpoint::prepare_root(&path).unwrap();
    let root = OpenedDirectory::open(&path).unwrap();
    write_private_at(&root, "record.json", b"original", 8, false).unwrap();
    assert!(write_private_at(&root, "record.json", b"changed", 8, false).is_err());
    assert_eq!(
        read_private_at(&root, "record.json", 8).unwrap().unwrap(),
        b"original"
    );
    assert!(write_private_at(&root, "record.json", b"too many bytes", 8, true).is_err());
    write_private_at(&root, "record.json", b"next", 8, true).unwrap();
    assert_eq!(
        read_private_at(&root, "record.json", 8).unwrap().unwrap(),
        b"next"
    );
    assert!(write_private_at(&root, "../foreign", b"x", 8, true).is_err());
    assert!(remove_private_at(&root, "record.json", b"wrong").is_err());
    assert_eq!(
        read_private_at(&root, "record.json", 8).unwrap().unwrap(),
        b"next"
    );
    assert!(remove_private_at(&root, "record.json", b"next").unwrap());
    assert!(!remove_private_at(&root, "record.json", b"next").unwrap());
}

#[cfg(unix)]
#[test]
fn rooted_private_publication_stays_with_original_directory_after_rebinding() {
    let temporary = tempfile::tempdir().unwrap();
    let original = temporary.path().join("original");
    crate::endpoint::prepare_root(&original).unwrap();
    let root = OpenedDirectory::open(&original).unwrap();
    std::fs::rename(&original, temporary.path().join("moved")).unwrap();
    crate::endpoint::prepare_root(&original).unwrap();
    write_private_at(&root, "record.json", b"owned", 8, false).unwrap();
    assert!(!original.join("record.json").exists());
    assert_eq!(
        std::fs::read(temporary.path().join("moved/record.json")).unwrap(),
        b"owned"
    );
}

#[cfg(windows)]
#[test]
fn rooted_private_publication_keeps_native_unicode_and_long_paths() {
    let temporary = tempfile::tempdir().unwrap();
    let mut path = temporary.path().join("private-é-日本語-🙂");
    crate::endpoint::prepare_root(&path).unwrap();
    for _ in 0..5 {
        path = path.join("segment-01234567890123456789012345678901234567890123456789");
        crate::endpoint::prepare_root(&path).unwrap();
    }
    assert!(path.as_os_str().len() > 260);
    let root = OpenedDirectory::open(&path).unwrap();
    write_private_at(&root, "record-é.json", b"native", 32, false).unwrap();
    assert_eq!(
        read_private_at(&root, "record-é.json", 32)
            .unwrap()
            .unwrap(),
        b"native"
    );
}

#[cfg(unix)]
#[test]
fn inherited_kernel_lock_survives_outer_shell_exit_until_last_producer_exits() {
    use std::{
        io::{BufRead, Write},
        os::fd::AsRawFd,
        process::{Command, Stdio},
        sync::mpsc,
        time::Duration,
    };
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("private");
    crate::endpoint::prepare_root(&path).unwrap();
    let root = OpenedDirectory::open(&path).unwrap();
    let lock =
        crate::endpoint::create_private_file_at(&root, std::ffi::OsStr::new("producer.lock"))
            .unwrap();
    lock.lock().unwrap();
    let fd = lock.as_raw_fd();
    // SAFETY: this exact live owned descriptor is the only descriptor changed.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    assert!(flags >= 0);
    assert_eq!(
        unsafe { libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) },
        0
    );
    let input_fd = if fd == 8 { 9 } else { 8 };
    let script = format!(
        "exec {input_fd}<&0; (printf 'ready\\n'; read release <&{input_fd}; printf 'done\\n') & exit 0"
    );
    let mut outer = Command::new("/bin/sh")
        .args(["-c", &script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    assert_eq!(unsafe { libc::fcntl(fd, libc::F_SETFD, flags) }, 0);
    let mut release = outer.stdin.take().unwrap();
    let output = outer.stdout.take().unwrap();
    let (send, receive) = mpsc::channel();
    let observer = std::thread::spawn(move || {
        for line in std::io::BufReader::new(output).lines() {
            send.send(line.unwrap()).unwrap();
        }
    });
    assert_eq!(
        receive.recv_timeout(Duration::from_secs(5)).unwrap(),
        "ready"
    );
    assert!(outer.wait().unwrap().success());
    // Dropping this File closes only our descriptor. No explicit unlock may
    // release the open-description lock still held by the active producer.
    drop(lock);
    let competing = root
        .open_file(std::ffi::OsStr::new("producer.lock"))
        .unwrap();
    assert!(
        competing.try_lock().is_err(),
        "producer survives its outer shell and still excludes a second wrapper"
    );
    writeln!(release, "finish").unwrap();
    drop(release);
    assert_eq!(
        receive.recv_timeout(Duration::from_secs(5)).unwrap(),
        "done"
    );
    observer.join().unwrap();
    let (send, receive) = mpsc::channel();
    let owner = std::thread::spawn(move || {
        competing.lock().unwrap();
        send.send(()).unwrap();
    });
    receive
        .recv_timeout(Duration::from_secs(5))
        .expect("last producer close releases the actual kernel lock");
    owner.join().unwrap();
}
