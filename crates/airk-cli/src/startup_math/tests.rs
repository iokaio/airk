// SPDX-License-Identifier: Apache-2.0
//! Pure bootstrap decisions. These tests never change the process environment.
use super::*;
use std::cell::{Cell, RefCell};

#[test]
fn absent_cuda_setting_receives_exact_compute32_default_once() {
    let reads = Cell::new(0);
    let writes = RefCell::new(Vec::new());
    supply_default(
        true,
        || {
            reads.set(reads.get() + 1);
            None
        },
        |v| writes.borrow_mut().push(v),
    );
    assert_eq!(reads.get(), 1);
    assert_eq!(*writes.borrow(), ["2"]);
}

#[test]
fn explicit_valid_conflicting_and_empty_values_are_preserved() {
    for value in ["2", "3", "0", "", "unrecognized"] {
        supply_default(
            true,
            || Some(OsString::from(value)),
            |_| panic!("overwrote explicit value {value:?}"),
        );
    }
}

#[test]
fn disabled_build_policy_neither_reads_nor_writes_environment() {
    supply_default(
        false,
        || panic!("disabled target read environment"),
        |_| panic!("disabled target wrote environment"),
    );
}

#[test]
fn repeated_startup_does_not_overwrite_the_supplied_default() {
    let value = RefCell::new(None);
    let writes = Cell::new(0);
    for _ in 0..2 {
        supply_default(
            true,
            || value.borrow().clone(),
            |v| {
                writes.set(writes.get() + 1);
                *value.borrow_mut() = Some(OsString::from(v));
            },
        );
    }
    assert_eq!(writes.get(), 1);
    assert_eq!(*value.borrow(), Some(OsString::from("2")));
}

#[test]
#[cfg(windows)]
fn explicit_non_unicode_windows_value_is_preserved() {
    use std::os::windows::ffi::OsStringExt;
    let value = OsString::from_wide(&[0xD800]);
    assert!(value.to_str().is_none());
    supply_default(
        true,
        || Some(value),
        |_| panic!("overwrote explicit non-Unicode policy"),
    );
}
