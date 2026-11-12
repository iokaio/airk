// SPDX-License-Identifier: Apache-2.0
//! `airkctl admin url` and `airkctl admin open` (split plan §7.1; contracts
//! §2.7 A1 and §4.5 as amended by the adopted security review H1, H2, M5).
//!
//! Both ask the service for a single-use launch code over the owner pipe
//! and build `https://127.0.0.1:<port>/admin/launch#c=<code>`; the code is
//! only ever in the fragment. `url` prints it for manual use. `open` never
//! puts it in a process argument (H1): it writes an owner-only redirect file
//! in the service's owner-private control folder and opens that file.
use crate::contract::{ErrorLine, ExitClass};
use airk_core::service::{ControlMethod, ControlResponse};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The owner-private folder, inside the control root, for redirect files.
const REDIRECT_FOLDER: &str = "admin-launch";

/// The launch URL (`admin_contract::launch_url`): the code in the fragment.
pub fn launch_url(port: u16, code: &str) -> String {
    format!("https://127.0.0.1:{port}/admin/launch#c={code}")
}

/// The admin pages a route may open (contracts §4.5, U97).
pub const PAGES: [&str; 11] = [
    "overview",
    "index-library",
    "activity",
    "agents",
    "storage",
    "recovery",
    "service",
    "logs",
    "usage",
    "search",
    "help",
];

/// Percent-encoding of everything but the RFC 3986 unreserved characters.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// A page route, `<page>[?<name>=<value>…]`, with each value encoded. A
/// route only selects a page and prefills fields; it never starts an action.
pub fn route(page: &str, prefill: &[(&str, &str)]) -> String {
    let query: Vec<String> = prefill
        .iter()
        .map(|(name, value)| format!("{name}={}", encode(value)))
        .collect();
    if query.is_empty() {
        page.to_owned()
    } else {
        format!("{page}?{}", query.join("&"))
    }
}

/// The launch URL that opens `route` once signed in: `#c=<code>&route=<encoded>`.
/// The page drops the code from history and keeps only the route.
pub fn with_route(url: String, route: Option<&str>) -> String {
    match route {
        Some(route) => format!("{url}&route={}", encode(route)),
        None => url,
    }
}

/// The stderr warning `admin url` writes (M5).
pub const URL_WARNING: &str = "warning: this URL is a single-use AIRK Admin sign-in secret, valid for 60 seconds. Open it only in your own browser on this computer; never paste it into a page or share it.";

fn failed(code: &str, message: &str) -> ErrorLine {
    ErrorLine::new(ExitClass::Refused, code, false, message)
}

/// A1 over the owner pipe; the launch URL.
pub fn url(control: &Path, version: &str) -> Result<String, ErrorLine> {
    match crate::service::request(control, ControlMethod::AdminLaunchCode {}, version)? {
        ControlResponse::AdminLaunchCode { code, port, .. } => Ok(launch_url(port, code.as_str())),
        // A Failure carries only its code; no launch code reaches an error.
        other => Err(crate::service::refusal(&other)),
    }
}

/// H2: `admin open` runs only in a local graphical session. `variable`
/// reads the environment (injected for tests); `needs_display` is true where
/// a desktop session is announced by `DISPLAY`/`WAYLAND_DISPLAY` (Linux and
/// other Unix desktops, not macOS or Windows).
pub fn local_desktop(
    variable: impl Fn(&str) -> Option<OsString>,
    needs_display: bool,
) -> Result<(), ErrorLine> {
    let set = |name: &str| variable(name).is_some_and(|value| !value.is_empty());
    let remote = ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"]
        .iter()
        .any(|name| set(name));
    let display = !needs_display || set("DISPLAY") || set("WAYLAND_DISPLAY");
    if remote || !display {
        return Err(failed(
            "admin_no_local_desktop",
            "airkctl admin open needs a local graphical session on this computer; AIRK Admin is not opened over SSH or port forwarding",
        ));
    }
    Ok(())
}

/// The redirect document: navigates to the launch URL without a referrer.
pub fn redirect_document(url: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"referrer\" content=\"no-referrer\">\n<meta http-equiv=\"refresh\" content=\"0;url={url}\">\n<title>AIRK Admin</title>\n</head>\n<body></body>\n</html>\n"
    )
}

/// Write the redirect file, owner-only, under `control`. Earlier redirect
/// files are removed first ("deleted on the next run", H1).
fn write_redirect(control: &Path, url: &str) -> Result<PathBuf, ErrorLine> {
    let unavailable = |error: &dyn std::fmt::Display| {
        failed(
            "admin_launch_unprotected",
            &format!("the owner-only launch file could not be written: {error}"),
        )
    };
    let folder = control.join(REDIRECT_FOLDER);
    airk_ipc::endpoint::prepare_root(&folder).map_err(|error| unavailable(&error))?;
    if let Ok(entries) = std::fs::read_dir(&folder) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("launch-") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).map_err(|error| unavailable(&error))?;
    let name: String = nonce.iter().map(|byte| format!("{byte:02x}")).collect();
    let path = folder.join(format!("launch-{name}.html"));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path).map_err(|error| unavailable(&error))?;
    std::io::Write::write_all(&mut file, redirect_document(url).as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|error| unavailable(&error))?;
    Ok(path)
}

/// Open a local file with the desktop's default handler. Only the redirect
/// file's path is an argument, never the launch URL.
fn browse(path: &Path) -> Result<(), ErrorLine> {
    #[cfg(windows)]
    let mut command = {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", ""]).arg(path);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(path);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(path);
        command
    };
    let status = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|error| {
            failed(
                "admin_open_failed",
                &format!("the browser could not be started: {error}"),
            )
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(failed(
            "admin_open_failed",
            "the desktop could not open the AIRK Admin launch file",
        ))
    }
}

/// `admin open`: refuse without a local desktop, then A1, the redirect
/// file and the default browser. Returns the redirect file's path.
pub fn open(control: &Path, version: &str, route: Option<&str>) -> Result<PathBuf, ErrorLine> {
    local_desktop(
        |name| std::env::var_os(name),
        cfg!(all(unix, not(target_os = "macos"))),
    )?;
    let url = with_route(url(control, version)?, route);
    let path = write_redirect(control, &url)?;
    browse(&path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_encode_their_prefill_and_ride_after_the_code() {
        let route = route("index-library", &[("root", "C:\\Users\\me\\My Docs")]);
        assert_eq!(route, "index-library?root=C%3A%5CUsers%5Cme%5CMy%20Docs");
        assert_eq!(
            with_route(
                "https://127.0.0.1:1/admin/launch#c=abc".into(),
                Some(&route)
            ),
            "https://127.0.0.1:1/admin/launch#c=abc&route=index-library%3Froot%3DC%253A%255CUsers%255Cme%255CMy%2520Docs"
        );
        assert_eq!(with_route("u".into(), None), "u");
    }

    /// `fixtures/split/i1/admin_launch_code.json`, case `one_shot_url`.
    #[test]
    fn the_url_carries_the_code_only_in_its_fragment() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/split/i1/admin_launch_code.json"
        ))
        .unwrap();
        let case = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == "one_shot_url")
            .unwrap();
        let transcript = case["transcript"].as_array().unwrap();
        let reply = &transcript[1]["reply"];
        let url = launch_url(
            u16::try_from(reply["port"].as_u64().unwrap()).unwrap(),
            reply["code"].as_str().unwrap(),
        );
        assert_eq!(format!("{url}\n"), transcript[3]["write"].as_str().unwrap());
        let document = redirect_document(&url);
        assert!(document.contains(&format!("content=\"0;url={url}\"")));
    }

    #[test]
    fn open_needs_a_local_desktop() {
        let environment = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, value)| OsString::from(value))
            }
        };
        assert!(local_desktop(environment(&[]), false).is_ok());
        assert!(local_desktop(environment(&[("DISPLAY", ":0")]), true).is_ok());
        assert!(local_desktop(environment(&[("WAYLAND_DISPLAY", "wayland-0")]), true).is_ok());
        for refused in [
            local_desktop(environment(&[]), true),
            local_desktop(environment(&[("DISPLAY", "")]), true),
            local_desktop(environment(&[("SSH_CONNECTION", "1 2 3 4")]), false),
            local_desktop(
                environment(&[("DISPLAY", ":0"), ("SSH_CLIENT", "1 2 3")]),
                true,
            ),
        ] {
            assert_eq!(refused.unwrap_err().code, "admin_no_local_desktop");
        }
    }
}
