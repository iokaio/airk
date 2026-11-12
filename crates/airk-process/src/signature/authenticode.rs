// SPDX-License-Identifier: Apache-2.0
//! Windows Authenticode observation (thin FFI; the verdict is the pure
//! `super::authenticode_verdict`). `WinVerifyTrust` with
//! `WINTRUST_ACTION_GENERIC_VERIFY_V2` verifies the embedded signature and its
//! chain without UI and without fetching revocation data; the signer
//! certificate and the timestamp countersigner are read from the same
//! provider state that was trusted, then the state is closed.
use super::{AuthenticodeObservation, Signer};
use std::{
    ffi::{CStr, c_void},
    os::windows::ffi::OsStrExt,
    path::Path,
    ptr::null_mut,
};
use windows_sys::Win32::Security::{
    Cryptography::{
        CERT_CONTEXT, CERT_FIND_EXT_ONLY_ENHKEY_USAGE_FLAG, CERT_NAME_STR_REVERSE_FLAG,
        CERT_X500_NAME_STR, CTL_USAGE, CertGetEnhancedKeyUsage, CertNameToStrW, X509_ASN_ENCODING,
    },
    WinTrust::{
        WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO,
        WTD_CACHE_ONLY_URL_RETRIEVAL, WTD_CHOICE_FILE, WTD_REVOCATION_CHECK_NONE, WTD_REVOKE_NONE,
        WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE, WTD_UICONTEXT_EXECUTE,
        WTHelperGetProvCertFromChain, WTHelperGetProvSignerFromChain,
        WTHelperProvDataFromStateData, WinVerifyTrust,
    },
};

/// The largest subject or EKU buffer read from a certificate.
const MAX_FIELD_BYTES: u32 = 64 * 1024;

/// Observe the Authenticode signature of `path`.
pub(super) fn observe(path: &Path) -> AuthenticodeObservation {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut file = WINTRUST_FILE_INFO {
        cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: wide.as_ptr(),
        hFile: null_mut(),
        pgKnownSubject: null_mut(),
    };
    let mut data = WINTRUST_DATA {
        cbStruct: size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 { pFile: &mut file },
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwProvFlags: WTD_REVOCATION_CHECK_NONE | WTD_CACHE_ONLY_URL_RETRIEVAL,
        dwUIContext: WTD_UICONTEXT_EXECUTE,
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY: `data` and the file information it points to (with its
    // NUL-terminated wide path) outlive both calls; no window is used.
    let status = unsafe {
        WinVerifyTrust(
            null_mut(),
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast::<c_void>(),
        )
    };
    let signer = if status == 0 { signer(&data) } else { None };
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    // SAFETY: closes the provider state the verify call opened (also after a
    // failure, as the WinVerifyTrust contract requires).
    unsafe {
        WinVerifyTrust(
            null_mut(),
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast::<c_void>(),
        );
    }
    AuthenticodeObservation { status, signer }
}

/// The first signer of the trusted state: certificate subject, EKUs and
/// whether a timestamp countersigner is present.
fn signer(data: &WINTRUST_DATA) -> Option<Signer> {
    // SAFETY: `hWVTStateData` is the open state of a successful verify; the
    // helpers return pointers into that state (or null), read before close.
    unsafe {
        let provider = WTHelperProvDataFromStateData(data.hWVTStateData);
        if provider.is_null() {
            return None;
        }
        let signer = WTHelperGetProvSignerFromChain(provider, 0, 0, 0);
        if signer.is_null() {
            return None;
        }
        let certificate = WTHelperGetProvCertFromChain(signer, 0);
        if certificate.is_null() || (*certificate).pCert.is_null() {
            return None;
        }
        let context = (*certificate).pCert;
        Some(Signer {
            subject: subject(context)?,
            ekus: ekus(context)?,
            timestamped: (*signer).csCounterSigners > 0,
        })
    }
}

/// The subject as `X509Certificate.Subject` renders it on Windows
/// (`CERT_X500_NAME_STR | CERT_NAME_STR_REVERSE_FLAG`: `CN=…, …, C=…`).
fn subject(context: *const CERT_CONTEXT) -> Option<String> {
    let flags = CERT_X500_NAME_STR | CERT_NAME_STR_REVERSE_FLAG;
    // SAFETY: `context` is a live certificate context with its `pCertInfo`;
    // the buffer is sized by the first call and bounded.
    unsafe {
        let info = (*context).pCertInfo;
        if info.is_null() {
            return None;
        }
        let name = &(*info).Subject;
        let length = CertNameToStrW(X509_ASN_ENCODING, name, flags, null_mut(), 0);
        if length <= 1 || length > MAX_FIELD_BYTES {
            return None;
        }
        let mut buffer = vec![0u16; length as usize];
        let written = CertNameToStrW(X509_ASN_ENCODING, name, flags, buffer.as_mut_ptr(), length);
        if written == 0 || written > length {
            return None;
        }
        String::from_utf16(&buffer[..written as usize - 1]).ok()
    }
}

/// The certificate's enhanced key usage extension; none is an empty list.
fn ekus(context: *const CERT_CONTEXT) -> Option<Vec<String>> {
    let flags = CERT_FIND_EXT_ONLY_ENHKEY_USAGE_FLAG;
    let mut size = 0u32;
    // SAFETY: a size query with a null output buffer.
    if unsafe { CertGetEnhancedKeyUsage(context, flags, null_mut(), &mut size) } == 0 {
        return Some(Vec::new());
    }
    if (size as usize) < size_of::<CTL_USAGE>() || size > MAX_FIELD_BYTES {
        return None;
    }
    // An 8-byte aligned buffer for the CTL_USAGE header and its pointers.
    let mut buffer = vec![0u64; (size as usize).div_ceil(size_of::<u64>())];
    let usage = buffer.as_mut_ptr().cast::<CTL_USAGE>();
    // SAFETY: the buffer holds `size` writable, aligned bytes; the returned
    // structure's pointers point into that buffer and are read before it drops.
    unsafe {
        if CertGetEnhancedKeyUsage(context, flags, usage, &mut size) == 0 {
            return None;
        }
        let count = (*usage).cUsageIdentifier as usize;
        let identifiers = (*usage).rgpszUsageIdentifier;
        if count > 0 && identifiers.is_null() {
            return None;
        }
        let mut found = Vec::with_capacity(count);
        for index in 0..count {
            let identifier = *identifiers.add(index);
            if identifier.is_null() {
                return None;
            }
            found.push(
                CStr::from_ptr(identifier.cast_const().cast())
                    .to_str()
                    .ok()?
                    .to_owned(),
            );
        }
        Some(found)
    }
}
