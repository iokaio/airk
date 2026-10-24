// SPDX-License-Identifier: Apache-2.0
#![cfg(unix)]
use airk_process::GroupChild;
use std::{
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn fixture(mode: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "group_fixture", "--nocapture"])
        .env("AIRK_GROUP_TEST_MODE", mode)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

#[test]
fn group_fixture() {
    let Ok(mode) = std::env::var("AIRK_GROUP_TEST_MODE") else {
        return;
    };
    match mode.as_str() {
        "sleep" => {
            for _ in 0..3 {
                std::thread::spawn(|| std::thread::sleep(Duration::from_secs(60)));
            }
            std::thread::sleep(Duration::from_secs(60));
        }
        "auto_reap" | "custom_reap" => {
            // Signal policy changes are confined to this isolated test process.
            let mut action = unsafe { std::mem::zeroed::<libc::sigaction>() };
            extern "C" fn handler(_: libc::c_int) {}
            action.sa_sigaction = if mode == "auto_reap" {
                libc::SIG_IGN
            } else {
                handler as *const () as usize
            };
            assert_eq!(
                unsafe { libc::sigaction(libc::SIGCHLD, &action, std::ptr::null_mut()) },
                0
            );
            assert!(GroupChild::spawn(&mut fixture("sleep")).is_err());
        }
        "descendants" => {
            let mut children = Vec::new();
            for _ in 0..2 {
                // These deliberately inherit the provider group and close all
                // output pipes: EOF alone cannot prove descendant termination.
                children.push(fixture("sleep").spawn().unwrap());
            }
            let pids = children
                .iter()
                .map(|child| child.id().to_string())
                .collect::<Vec<_>>();
            std::fs::write(
                std::env::var_os("AIRK_GROUP_TEST_PIDS").unwrap(),
                pids.join("\n"),
            )
            .unwrap();
            let release = std::env::var_os("AIRK_GROUP_TEST_RELEASE").unwrap();
            while !std::path::Path::new(&release).exists() {
                std::thread::sleep(Duration::from_millis(5));
            }
            // Leave both descendants alive when the group leader exits.
        }
        _ => panic!("unknown fixture mode"),
    }
}

#[test]
fn exited_leader_keeps_pid_reserved_until_descendants_are_dead() {
    let directory = tempfile::tempdir().unwrap();
    let pids_file = directory.path().join("pids");
    let release = directory.path().join("release");
    let mut command = fixture("descendants");
    command
        .env("AIRK_GROUP_TEST_PIDS", &pids_file)
        .env("AIRK_GROUP_TEST_RELEASE", &release)
        .stdout(Stdio::piped());
    let mut child = GroupChild::spawn(&mut command).unwrap();
    let group = child.id();
    let deadline = Instant::now() + Duration::from_secs(10);
    let pids = loop {
        if let Ok(text) = std::fs::read_to_string(&pids_file) {
            let pids = text
                .lines()
                .map(|line| line.parse::<libc::pid_t>().unwrap())
                .collect::<Vec<_>>();
            if pids.len() == 2 {
                break pids;
            }
        }
        assert!(
            Instant::now() < deadline,
            "fixture did not start descendants"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    for pid in &pids {
        assert_eq!(unsafe { libc::getpgid(*pid) }, group as libc::pid_t);
    }
    #[cfg(target_os = "linux")]
    let handles = pids
        .iter()
        .map(|pid| {
            use std::os::fd::FromRawFd;
            let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, *pid, 0) };
            assert!(fd >= 0, "pidfd_open: {}", std::io::Error::last_os_error());
            unsafe { std::os::fd::OwnedFd::from_raw_fd(fd as i32) }
        })
        .collect::<Vec<_>>();
    #[cfg(target_os = "linux")]
    for handle in &handles {
        assert!(!pidfd_exited(handle));
    }
    std::fs::write(release, b"release").unwrap();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline, "fixture leader did not exit");
        std::thread::sleep(Duration::from_millis(5));
    }
    for _ in 0..3 {
        assert!(child.try_wait().unwrap().unwrap().success());
        assert_eq!(
            unsafe { libc::getpgid(group as libc::pid_t) },
            group as libc::pid_t
        );
    }
    let mut output = Vec::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut output)
        .unwrap();
    #[cfg(target_os = "linux")]
    for handle in &handles {
        assert!(
            !pidfd_exited(handle),
            "pipe EOF must not imply descendant exit"
        );
    }
    assert!(
        child
            .terminate_and_wait(Duration::from_secs(5))
            .unwrap()
            .success()
    );
    #[cfg(target_os = "linux")]
    for handle in &handles {
        assert!(
            pidfd_exited(handle),
            "capacity released before native process exit"
        );
    }
    assert_eq!(
        child.kill().unwrap_err().kind(),
        std::io::ErrorKind::InvalidInput
    );
    // Cached completion never signals the now-released numerical group again.
    child.drain();
    assert!(child.wait().unwrap().success());
}

#[cfg(target_os = "linux")]
fn pidfd_exited(handle: &std::os::fd::OwnedFd) -> bool {
    use std::os::fd::AsRawFd;
    let mut poll = libc::pollfd {
        fd: handle.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    assert!(unsafe { libc::poll(&mut poll, 1, 0) } >= 0);
    poll.revents & libc::POLLIN != 0
}

#[test]
fn timeout_retains_unreaped_leader_for_later_drain() {
    let mut child = GroupChild::spawn(&mut fixture("sleep")).unwrap();
    let pid = child.id() as libc::pid_t;
    assert!(child.terminate_and_wait(Duration::ZERO).is_err());
    assert_eq!(unsafe { libc::getpgid(pid) }, pid);
    child.terminate_and_wait(Duration::from_secs(5)).unwrap();
    assert_eq!(
        child.kill().unwrap_err().kind(),
        std::io::ErrorKind::InvalidInput
    );
}

#[test]
fn refuses_auto_reaping_host_before_starting_provider() {
    assert!(fixture("auto_reap").status().unwrap().success());
}

#[test]
fn custom_child_reaper_is_rejected_before_provider_spawn() {
    assert!(fixture("custom_reap").status().unwrap().success());
}
