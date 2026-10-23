// SPDX-License-Identifier: Apache-2.0
//! Optional systemd identity inspection through its typed local user-bus API.
//! The SDK is loaded from protected OS paths only; on-demand service needs none of it.
use super::*;
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
};

type Pointer = *mut c_void;
type New = unsafe extern "C" fn(*mut Pointer) -> c_int;
type SetAddress = unsafe extern "C" fn(Pointer, *const c_char) -> c_int;
type SetClient = unsafe extern "C" fn(Pointer, c_int) -> c_int;
type SetTimeout = unsafe extern "C" fn(Pointer, u64) -> c_int;
type Start = unsafe extern "C" fn(Pointer) -> c_int;
type Release = unsafe extern "C" fn(Pointer) -> Pointer;
type Call = unsafe extern "C" fn(
    Pointer,
    *const c_char,
    *const c_char,
    *const c_char,
    *const c_char,
    Pointer,
    *mut Pointer,
    *const c_char,
    ...
) -> c_int;
type ReadBasic = unsafe extern "C" fn(Pointer, c_char, Pointer) -> c_int;
type Enter = unsafe extern "C" fn(Pointer, c_char, *const c_char) -> c_int;
type Exit = unsafe extern "C" fn(Pointer) -> c_int;
type AtEnd = unsafe extern "C" fn(Pointer, c_int) -> c_int;
type Skip = unsafe extern "C" fn(Pointer, *const c_char) -> c_int;

struct Library(Pointer);
impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: this object owns the successful dlopen reference. All API borrows ended.
        unsafe {
            libc::dlclose(self.0);
        }
    }
}
impl Library {
    fn open() -> Result<Self> {
        let triplet = match std::env::consts::ARCH {
            "x86_64" => "x86_64-linux-gnu",
            "aarch64" => "aarch64-linux-gnu",
            _ => bail!("systemd SDK unsupported on this architecture"),
        };
        for path in [
            format!("/usr/lib/{triplet}/libsystemd.so.0"),
            format!("/lib/{triplet}/libsystemd.so.0"),
            "/usr/lib/libsystemd.so.0".into(),
            "/lib/libsystemd.so.0".into(),
        ] {
            let Ok(path) = Path::new(&path).canonicalize() else {
                continue;
            };
            let metadata = fs::symlink_metadata(&path)?;
            ensure!(
                metadata.is_file()
                    && metadata.uid() == 0
                    && metadata.permissions().mode() & 0o022 == 0,
                "untrusted systemd SDK file"
            );
            for parent in path.parent().unwrap().ancestors() {
                let metadata = fs::symlink_metadata(parent)?;
                ensure!(
                    metadata.is_dir()
                        && metadata.uid() == 0
                        && metadata.permissions().mode() & 0o022 == 0,
                    "untrusted systemd SDK ancestry"
                );
            }
            let path = CString::new(path.to_str().context("non-UTF-8 systemd SDK path")?)?;
            // SAFETY: protected absolute OS library, terminated name and standard loader flags.
            let library = unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
            ensure!(!library.is_null(), "systemd SDK could not load");
            return Ok(Self(library));
        }
        bail!("systemd SDK is unavailable; on-demand operation remains independent")
    }
    fn symbol<T: Copy>(&self, name: &CStr) -> Result<T> {
        // SAFETY: live dlopen reference and terminated symbol name.
        let symbol = unsafe { libc::dlsym(self.0, name.as_ptr()) };
        ensure!(
            !symbol.is_null() && std::mem::size_of::<T>() == std::mem::size_of_val(&symbol),
            "required systemd SDK symbol unavailable"
        );
        // SAFETY: private callers supply the documented C function pointer type for
        // each exact SDK symbol. POSIX specifies conversion of dlsym function addresses.
        Ok(unsafe { std::mem::transmute_copy(&symbol) })
    }
}
struct Api {
    new: New,
    address: SetAddress,
    client: SetClient,
    timeout: SetTimeout,
    start: Start,
    release: Release,
    call: Call,
    read: ReadBasic,
    enter: Enter,
    exit: Exit,
    end: AtEnd,
    skip: Skip,
    message_release: Release,
    _library: Library,
}
impl Api {
    fn open() -> Result<Self> {
        let library = Library::open()?;
        Ok(Self {
            new: library.symbol(c"sd_bus_new")?,
            address: library.symbol(c"sd_bus_set_address")?,
            client: library.symbol(c"sd_bus_set_bus_client")?,
            timeout: library.symbol(c"sd_bus_set_method_call_timeout")?,
            start: library.symbol(c"sd_bus_start")?,
            release: library.symbol(c"sd_bus_unref")?,
            call: library.symbol(c"sd_bus_call_method")?,
            read: library.symbol(c"sd_bus_message_read_basic")?,
            enter: library.symbol(c"sd_bus_message_enter_container")?,
            exit: library.symbol(c"sd_bus_message_exit_container")?,
            end: library.symbol(c"sd_bus_message_at_end")?,
            skip: library.symbol(c"sd_bus_message_skip")?,
            message_release: library.symbol(c"sd_bus_message_unref")?,
            _library: library,
        })
    }
}
fn checked_result(result: c_int) -> Result<c_int> {
    if result < 0 {
        return Err(std::io::Error::from_raw_os_error(-result))
            .context("systemd identity inspection failed");
    }
    Ok(result)
}
struct Bus<'a> {
    api: &'a Api,
    pointer: Pointer,
    deadline: Instant,
}
impl Drop for Bus<'_> {
    fn drop(&mut self) {
        // SAFETY: owned bus reference; disconnect also releases this client's unit references.
        unsafe {
            (self.api.release)(self.pointer);
        }
    }
}
struct Message<'a> {
    api: &'a Api,
    pointer: Pointer,
    remaining: usize,
}
impl Drop for Message<'_> {
    fn drop(&mut self) {
        // SAFETY: owned reply reference while the SDK remains loaded.
        unsafe {
            (self.api.message_release)(self.pointer);
        }
    }
}
impl<'a> Bus<'a> {
    fn open(api: &'a Api) -> Result<Self> {
        // Use the OS user namespace, never an inherited network/session-bus override.
        let uid = unsafe { libc::geteuid() };
        let directory = PathBuf::from(format!("/run/user/{uid}"));
        let metadata = fs::symlink_metadata(&directory)?;
        ensure!(
            metadata.is_dir()
                && !metadata.file_type().is_symlink()
                && metadata.uid() == uid
                && metadata.permissions().mode() & 0o077 == 0,
            "unsafe systemd user runtime directory"
        );
        for parent in directory.parent().unwrap().ancestors() {
            let metadata = fs::symlink_metadata(parent)?;
            ensure!(
                metadata.is_dir()
                    && !metadata.file_type().is_symlink()
                    && metadata.uid() == 0
                    && metadata.permissions().mode() & 0o022 == 0,
                "unsafe systemd user-bus ancestry"
            );
        }
        let socket = directory.join("bus");
        let metadata = fs::symlink_metadata(&socket)?;
        ensure!(
            metadata.file_type().is_socket() && metadata.uid() == uid,
            "unsafe systemd user-bus socket"
        );
        let mut pointer = std::ptr::null_mut();
        // SAFETY: writable pointer for one new SDK bus reference.
        checked_result(unsafe { (api.new)(&mut pointer) })?;
        ensure!(!pointer.is_null(), "systemd SDK returned no bus");
        let bus = Self {
            api,
            pointer,
            deadline: Instant::now() + Duration::from_secs(5),
        };
        let address = CString::new(format!("unix:path={}", socket.display()))?;
        // SAFETY: owned live bus; SDK copies the address. Scalar options use their documented ABI.
        unsafe {
            checked_result((api.address)(pointer, address.as_ptr()))?;
            checked_result((api.client)(pointer, 1))?;
            checked_result((api.timeout)(pointer, 5_000_000))?;
            checked_result((api.start)(pointer))?;
        }
        Ok(bus)
    }
    fn call(
        &self,
        path: &CStr,
        interface: &CStr,
        member: &CStr,
        first: &CStr,
        second: Option<&CStr>,
    ) -> Result<Message<'a>> {
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .context("systemd configuration inspection exceeded five seconds")?;
        let micros = remaining.as_micros().max(1).min(u64::MAX as u128) as u64;
        // SAFETY: live bus; all calls share one bounded inspection deadline.
        checked_result(unsafe { (self.api.timeout)(self.pointer, micros) })?;
        let mut pointer = std::ptr::null_mut();
        // SAFETY: live bus, terminated constant/name arguments and correctly typed
        // string varargs. The SDK owns a returned message reference on success.
        let result = unsafe {
            match second {
                Some(second) => (self.api.call)(
                    self.pointer,
                    c"org.freedesktop.systemd1".as_ptr(),
                    path.as_ptr(),
                    interface.as_ptr(),
                    member.as_ptr(),
                    std::ptr::null_mut(),
                    &mut pointer,
                    c"ss".as_ptr(),
                    first.as_ptr(),
                    second.as_ptr(),
                ),
                None => (self.api.call)(
                    self.pointer,
                    c"org.freedesktop.systemd1".as_ptr(),
                    path.as_ptr(),
                    interface.as_ptr(),
                    member.as_ptr(),
                    std::ptr::null_mut(),
                    &mut pointer,
                    c"s".as_ptr(),
                    first.as_ptr(),
                ),
            }
        };
        checked_result(result)?;
        ensure!(!pointer.is_null(), "systemd reply missing");
        Ok(Message {
            api: self.api,
            pointer,
            remaining: MAX_CONFIG as usize,
        })
    }
    fn property(
        &self,
        path: &CStr,
        interface: &CStr,
        name: &str,
        signature: &str,
    ) -> Result<Message<'a>> {
        let name = CString::new(name)?;
        let signature = CString::new(signature)?;
        let mut reply = self.call(
            path,
            c"org.freedesktop.DBus.Properties",
            c"Get",
            interface,
            Some(&name),
        )?;
        reply.enter(b'v', &signature)?;
        Ok(reply)
    }
}
impl Message<'_> {
    fn enter(&mut self, kind: u8, signature: &CStr) -> Result<()> {
        // SAFETY: typed live message cursor and terminated expected signature.
        ensure!(
            checked_result(unsafe {
                (self.api.enter)(self.pointer, kind as c_char, signature.as_ptr())
            })? > 0,
            "unexpected systemd container"
        );
        Ok(())
    }
    fn exit(&mut self) -> Result<()> {
        // SAFETY: live message cursor, entered by this adapter.
        checked_result(unsafe { (self.api.exit)(self.pointer) })?;
        Ok(())
    }
    fn at_end(&self) -> Result<bool> {
        // SAFETY: observes the current message container without mutating borrowed data.
        Ok(checked_result(unsafe { (self.api.end)(self.pointer, 0) })? != 0)
    }
    fn string(&mut self, kind: u8) -> Result<String> {
        let mut value: *const c_char = std::ptr::null();
        // SAFETY: basic strings/object paths use a borrowed const char ** output.
        ensure!(
            checked_result(unsafe {
                (self.api.read)(
                    self.pointer,
                    kind as c_char,
                    (&mut value as *mut *const c_char).cast(),
                )
            })? > 0
                && !value.is_null(),
            "systemd string missing"
        );
        // SAFETY: SDK strings are terminated and valid for the unchanged message.
        let length = unsafe { libc::strnlen(value, 32_769) };
        ensure!(
            length <= 32_768 && length <= self.remaining,
            "systemd string budget exceeded"
        );
        self.remaining -= length;
        // SAFETY: bounded bytes of the borrowed SDK string, copied before cursor mutation.
        Ok(
            std::str::from_utf8(unsafe { std::slice::from_raw_parts(value.cast(), length) })?
                .to_owned(),
        )
    }
    fn strings(&mut self) -> Result<Vec<String>> {
        self.enter(b'a', c"s")?;
        let mut result = Vec::new();
        while !self.at_end()? {
            ensure!(result.len() < 64, "systemd argument count exceeded");
            result.push(self.string(b's')?);
        }
        self.exit()?;
        Ok(result)
    }
    fn scalar<T: Default>(&mut self, kind: u8) -> Result<T> {
        let mut value = T::default();
        // SAFETY: private call sites use the exact ABI type (int/u32/u64) for kind.
        ensure!(
            checked_result(unsafe {
                (self.api.read)(self.pointer, kind as c_char, (&mut value as *mut T).cast())
            })? > 0,
            "systemd scalar missing"
        );
        Ok(value)
    }
}

pub(super) fn verify(service: &ServiceManager) -> Result<bool> {
    let api = Api::open()?;
    let bus = Bus::open(&api)?;
    let unit = CString::new(service.unit())?;
    let mut loaded = bus.call(
        c"/org/freedesktop/systemd1",
        c"org.freedesktop.systemd1.Manager",
        c"LoadUnit",
        &unit,
        None,
    )?;
    let path = CString::new(loaded.string(b'o')?)?;
    // Pin the loaded configuration on this single connection through all reads.
    let _reference = bus.call(
        c"/org/freedesktop/systemd1",
        c"org.freedesktop.systemd1.Manager",
        c"RefUnit",
        &unit,
        None,
    )?;
    let mut fragment =
        bus.property(&path, c"org.freedesktop.systemd1.Unit", "FragmentPath", "s")?;
    let fragment = PathBuf::from(fragment.string(b's')?);
    if fragment.canonicalize()? != service.definition.canonicalize()? {
        return Ok(false);
    }
    let mut reload = bus.property(
        &path,
        c"org.freedesktop.systemd1.Unit",
        "NeedDaemonReload",
        "b",
    )?;
    if reload.scalar::<c_int>(b'b')? != 0 {
        return Ok(false);
    }
    let mut dropins = bus.property(&path, c"org.freedesktop.systemd1.Unit", "DropInPaths", "as")?;
    if !dropins.strings()?.is_empty() {
        return Ok(false);
    }
    let mut command = bus.property(
        &path,
        c"org.freedesktop.systemd1.Service",
        "ExecStartEx",
        "a(sasasttttuii)",
    )?;
    command.enter(b'a', c"(sasasttttuii)")?;
    if command.at_end()? {
        return Ok(false);
    }
    command.enter(b'r', c"sasasttttuii")?;
    let executable = command.string(b's')?;
    let arguments = command.strings()?;
    let flags = command.strings()?;
    let expected = std::iter::once(
        service
            .executable
            .to_str()
            .context("non-UTF-8 service executable")?
            .to_owned(),
    )
    .chain(service.args.iter().cloned())
    .collect::<Vec<_>>();
    if executable != expected[0] || arguments != expected || flags != ["no-env-expand"] {
        return Ok(false);
    }
    // SAFETY: the remaining fields have the documented timestamp/PID/exit signature.
    checked_result(unsafe { (api.skip)(command.pointer, c"ttttuii".as_ptr()) })?;
    command.exit()?;
    if !command.at_end()? {
        return Ok(false);
    }
    for name in [
        "ExecCondition",
        "ExecStartPre",
        "ExecStartPost",
        "ExecStop",
        "ExecStopPost",
        "ExecReload",
    ] {
        let mut reply = bus.property(
            &path,
            c"org.freedesktop.systemd1.Service",
            name,
            "a(sasbttttuii)",
        )?;
        reply.enter(b'a', c"(sasbttttuii)")?;
        if !reply.at_end()? {
            return Ok(false);
        }
    }
    for name in [
        "User",
        "Group",
        "RootDirectory",
        "RootImage",
        "WorkingDirectory",
    ] {
        let mut reply = bus.property(&path, c"org.freedesktop.systemd1.Service", name, "s")?;
        if !reply.string(b's')?.is_empty() {
            return Ok(false);
        }
    }
    for name in ["Environment", "SupplementaryGroups"] {
        let mut reply = bus.property(&path, c"org.freedesktop.systemd1.Service", name, "as")?;
        if !reply.strings()?.is_empty() {
            return Ok(false);
        }
    }
    let mut environments = bus.property(
        &path,
        c"org.freedesktop.systemd1.Service",
        "EnvironmentFiles",
        "a(sb)",
    )?;
    environments.enter(b'a', c"(sb)")?;
    if !environments.at_end()? {
        return Ok(false);
    }
    for (name, expected) in [
        ("Type", "simple"),
        ("Restart", "on-failure"),
        ("KillMode", "control-group"),
    ] {
        let mut reply = bus.property(&path, c"org.freedesktop.systemd1.Service", name, "s")?;
        if reply.string(b's')? != expected {
            return Ok(false);
        }
    }
    for (name, expected) in [
        ("RestartUSec", 10_000_000u64),
        ("TimeoutStopUSec", 30_000_000),
    ] {
        let mut reply = bus.property(&path, c"org.freedesktop.systemd1.Service", name, "t")?;
        if reply.scalar::<u64>(b't')? != expected {
            return Ok(false);
        }
    }
    let mut privileges = bus.property(
        &path,
        c"org.freedesktop.systemd1.Service",
        "NoNewPrivileges",
        "b",
    )?;
    let mut mask = bus.property(&path, c"org.freedesktop.systemd1.Service", "UMask", "u")?;
    if privileges.scalar::<c_int>(b'b')? != 1 || mask.scalar::<u32>(b'u')? != 0o077 {
        return Ok(false);
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_native_sdk_loads_and_reports_an_unavailable_local_bus() -> Result<()> {
        let api = match Api::open() {
            Ok(api) => api,
            Err(error) => {
                if std::env::var_os("AIRK_TEST_SYSTEMD_SDK").is_some() {
                    return Err(error);
                }
                // Linux without systemd is a supported on-demand environment.
                eprintln!("optional systemd SDK unavailable: {error}");
                return Ok(());
            }
        };
        let mut pointer = std::ptr::null_mut();
        // SAFETY: writable output, with the successful reference owned below.
        checked_result(unsafe { (api.new)(&mut pointer) })?;
        ensure!(!pointer.is_null(), "SDK did not create a bus");
        let mut bus = Bus {
            api: &api,
            pointer,
            deadline: Instant::now() + Duration::from_secs(5),
        };
        let absent = tempfile::tempdir()?.path().join("absent-user-bus");
        let address = CString::new(format!("unix:path={}", absent.display()))?;
        // SAFETY: private disconnected bus, valid address and SDK scalar ABI.
        unsafe {
            checked_result((api.address)(bus.pointer, address.as_ptr()))?;
            checked_result((api.client)(bus.pointer, 1))?;
            checked_result((api.timeout)(bus.pointer, 50_000))?;
            assert!(checked_result((api.start)(bus.pointer)).is_err());
        }
        bus.deadline = Instant::now() - Duration::from_secs(1);
        let error = match bus.call(
            c"/org/freedesktop/systemd1",
            c"org.freedesktop.systemd1.Manager",
            c"LoadUnit",
            c"fixture.service",
            None,
        ) {
            Ok(_) => bail!("expired inspection attempted another SDK call"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("five seconds"));
        Ok(())
    }

    #[test]
    fn systemctl_and_sdk_select_the_same_native_user_namespace() -> Result<()> {
        let root = tempfile::tempdir()?;
        let service = super::super::tests::fixture(Platform::Linux, root.path());
        let invocation = service.systemctl(&["show"]);
        // SAFETY: geteuid takes no pointers and observes this process's OS identity.
        let directory = format!("/run/user/{}", unsafe { libc::geteuid() });
        assert_eq!(invocation.env["XDG_RUNTIME_DIR"], directory);
        assert_eq!(
            invocation.env["DBUS_SESSION_BUS_ADDRESS"],
            format!("unix:path={directory}/bus")
        );
        Ok(())
    }
}
