// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::time::Duration;

#[test]
fn current_static_image_handle_matches_kernel_executable() -> Result<()> {
    let main = Handle::main_image()?;
    let expected = ImageId::file(&File::open("/proc/self/exe")?)?;
    assert_eq!(main.id()?, expected);
    assert!(mappings::executable_images()?.contains(&expected));
    Ok(())
}

#[test]
fn no_load_resolves_exact_resident_libc_and_refuses_other_symbol_owner() -> Result<()> {
    let libc = Handle::no_load("libc.so.6")?.context("GNU/Linux libc not resident")?;
    let canonical = Held::system_path("libc.so.6")?;
    let file = File::open(canonical)?;
    let actual = ImageId::file(&file)?;
    assert_eq!(libc.id()?, actual);
    let wrong = ImageId::file(&File::open("/proc/self/exe")?)?;
    type GetPid = unsafe extern "C" fn() -> i32;
    assert!(unsafe { libc.symbol::<GetPid>(c"getpid", wrong) }.is_err());
    let function = unsafe { libc.symbol::<GetPid>(c"getpid", actual) }?;
    assert_eq!(unsafe { function() } as u32, std::process::id());
    assert!(Handle::no_load("airk-absent-no-load-control.so")?.is_none());
    Ok(())
}

#[test]
fn pinned_host_elf_metadata_uses_real_loaded_libc_image() -> Result<()> {
    let path = Held::system_path("libc.so.6")?;
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let dynamic =
        super::super::elf::inspect(&mut file, len, Instant::now() + Duration::from_secs(2))?;
    assert_eq!(dynamic.soname.as_deref(), Some("libc.so.6"));
    assert!(dynamic.needed.contains(&"ld-linux-x86-64.so.2".to_owned()));
    Ok(())
}

#[test]
fn private_image_is_write_sealed_before_read_execute_mapping() -> Result<()> {
    use std::io::{Read, Seek, Write};
    use std::os::fd::AsRawFd;
    let mut file = held::private_image()?;
    let data = b"AIRK native loader seal control";
    file.write_all(data)?;
    held::seal_image(&file)?;
    let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GET_SEALS) };
    let required = libc::F_SEAL_WRITE | libc::F_SEAL_GROW | libc::F_SEAL_SHRINK | libc::F_SEAL_SEAL;
    assert!(flags >= 0);
    assert_eq!(flags & required, required);
    assert!(file.set_len(0).is_err());
    assert!(file.set_len(512).is_err());
    file.rewind()?;
    assert!(file.write_all(b"X").is_err());
    let mut actual = Vec::new();
    file.read_to_end(&mut actual)?;
    assert_eq!(actual, data);
    // Query actual kernel mapping permission. No bytes in the mapping are executed.
    let address = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            data.len(),
            libc::PROT_READ | libc::PROT_EXEC,
            libc::MAP_PRIVATE,
            file.as_raw_fd(),
            0,
        )
    };
    ensure!(
        address != libc::MAP_FAILED,
        "sealed CUDA image could not be mapped read/execute"
    );
    let seen = mappings::identity_at(address as usize);
    let status = unsafe { libc::munmap(address, data.len()) };
    assert_eq!(status, 0);
    assert_eq!(seen?, ImageId::file(&file)?);
    Ok(())
}
