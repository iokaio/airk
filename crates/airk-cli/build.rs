// SPDX-License-Identifier: Apache-2.0
//! Windows VERSIONINFO for `airk.exe` (Explorer Properties > Details, support
//! diagnostics). Values come from this package's Cargo metadata, so every
//! version bump updates them. Other targets get nothing: macOS reads the
//! bundle's Info.plist and Linux ELF has no standard version resource.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("TARGET").as_deref() == Ok("x86_64-unknown-linux-gnu")
        && std::env::var_os("CARGO_FEATURE_GPU_CUDA").is_some()
    {
        println!("cargo:rerun-if-env-changed=AIRK_APPROVED_LINUX_CUDA_MANIFEST_SHA256");
        println!("cargo:rerun-if-env-changed=AIRK_STATIC_LINUX_CUDA_BUILD_SHA256");
        if std::env::var_os("AIRK_APPROVED_LINUX_CUDA_MANIFEST_SHA256").is_some()
            && std::env::var_os("AIRK_STATIC_LINUX_CUDA_BUILD_SHA256").is_some()
        {
            for name in [
                "OrtGetApiBase",
                "airk_ort_bootstrap_linux_cuda_v1",
                "airk_ort_collect_final_placement_v1",
                "airk_ort_collect_final_graph_metadata_v1",
            ] {
                println!("cargo:rustc-link-arg-bin=airk=-Wl,--undefined={name},--export-dynamic-symbol={name}");
            }
        }
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let output = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let script = output.join("airk-version.rc");
    std::fs::write(&script, version_resource("airk.exe", "AIRK Service"))
        .expect("write the Windows version resource script");
    embed_resource::compile(&script, embed_resource::NONE)
        .manifest_optional()
        .expect("compile the Windows version resource");
}

/// A VERSIONINFO script for one executable of the workspace version.
fn version_resource(file_name: &str, description: &str) -> String {
    let number = |name: &str| -> u16 {
        std::env::var(name)
            .expect("Cargo package version component")
            .parse()
            .expect("numeric Cargo package version component")
    };
    let (major, minor, patch) = (
        number("CARGO_PKG_VERSION_MAJOR"),
        number("CARGO_PKG_VERSION_MINOR"),
        number("CARGO_PKG_VERSION_PATCH"),
    );
    let version = std::env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION");
    format!(
        r#"#pragma code_page(65001)
1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "CompanyName", "Ioka LLC"
            VALUE "FileDescription", "{description}"
            VALUE "FileVersion", "{version}"
            VALUE "InternalName", "{file_name}"
            VALUE "LegalCopyright", "Copyright Ioka LLC. Licensed under Apache-2.0."
            VALUE "OriginalFilename", "{file_name}"
            VALUE "ProductName", "AIRK"
            VALUE "ProductVersion", "{version}"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x409, 1200
    END
END
"#
    )
}
