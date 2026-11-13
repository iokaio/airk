// SPDX-License-Identifier: Apache-2.0
//! The macOS login-startup helper (`service_manager` on macOS); nothing on other targets.
use std::env;

fn main() {
    println!("cargo:rerun-if-changed=src/service_manager_macos.m");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new()
            .file("src/service_manager_macos.m")
            .flag("-fobjc-arc")
            .flag("-Werror")
            .compile("airk_macos_login");
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=framework=ServiceManagement");
        println!("cargo:rustc-link-lib=objc");
    }
}
