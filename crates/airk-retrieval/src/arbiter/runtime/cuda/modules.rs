// SPDX-License-Identifier: Apache-2.0
//! Full new-mapping classification; basename prefixes never authorize a new image.
use airk_process::directory::ObjectIdentity;
#[derive(Clone, Debug)]
pub(super) struct ModuleIdentity {
    pub address: usize,
    pub name: String,
    pub path: String,
    pub id: ObjectIdentity,
}
pub(super) fn allowed(
    current: &ModuleIdentity,
    baseline: &[ModuleIdentity],
    approved: &[ModuleIdentity],
) -> bool {
    approved
        .iter()
        .any(|v| v.name == current.name && v.path == current.path && v.id == current.id)
        || baseline.iter().any(|v| {
            v.address == current.address
                && v.name == current.name
                && v.path == current.path
                && v.id == current.id
        })
}
const MAX_REFUSAL_BYTES: usize = 1 << 20;
fn identity_value(row: &ModuleIdentity) -> serde_json::Value {
    let id = match row.id {
        ObjectIdentity::Windows {
            volume_serial,
            file_id,
        } => {
            serde_json::json!({"kind":"windows", "volume_serial":volume_serial, "file_id":file_id})
        }
        ObjectIdentity::Unix { device, inode } => {
            serde_json::json!({"kind":"unix", "device":device, "inode":inode})
        }
    };
    serde_json::json!({"name":row.name,"path":row.path,"address":row.address,"id":id})
}
fn ordered(rows: &[ModuleIdentity]) -> Vec<&ModuleIdentity> {
    let mut rows: Vec<_> = rows.iter().collect();
    rows.sort_by_cached_key(|row| {
        (
            row.name.clone(),
            row.path.clone(),
            row.address,
            format!("{:?}", row.id),
        )
    });
    rows
}
fn related(left: &ModuleIdentity, right: &ModuleIdentity) -> bool {
    left.name == right.name
        || left.path == right.path
        || left.id == right.id
        || (left.address != 0 && left.address == right.address)
}
/// Diagnostic observations only. The unchanged allowed predicate decides refusal.
pub(super) fn refusal_report(
    stage: &str,
    current: &[ModuleIdentity],
    baseline: &[ModuleIdentity],
    approved: &[ModuleIdentity],
) -> Option<String> {
    let denied: Vec<_> = ordered(current)
        .into_iter()
        .filter(|row| !allowed(row, baseline, approved))
        .collect();
    if denied.is_empty() {
        return None;
    }
    let stage_valid = stage.len() <= 512;
    let mut report = serde_json::json!({
        "schema":"airk.private.cuda-module-refusal/1",
        "stage":if stage_valid {stage} else {"diagnostic-stage-overflow"},
        "unapproved_count":denied.len(), "reported_count":0,
        "omitted_count":denied.len(), "complete":false, "mismatches":[]
    });
    let baseline = ordered(baseline);
    let approved = ordered(approved);
    let mut budget = report.to_string().len() + 128;
    let mut details = Vec::new();
    if stage_valid {
        for row in &denied {
            let expected_baseline: Vec<_> = baseline
                .iter()
                .copied()
                .filter(|v| related(row, v))
                .collect();
            let expected_approved: Vec<_> = approved
                .iter()
                .copied()
                .filter(|v| related(row, v))
                .collect();
            let same_object = expected_baseline.iter().any(|v| {
                v.name == row.name
                    && v.path == row.path
                    && v.id == row.id
                    && v.address != row.address
            });
            let classification = if same_object {
                "address_reload"
            } else if expected_baseline.is_empty() && expected_approved.is_empty() {
                "new_image"
            } else {
                "path_or_identity_drift"
            };
            let detail = serde_json::json!({"classification":classification,"actual":identity_value(row),
                "baseline_candidates":expected_baseline.into_iter().map(identity_value).collect::<Vec<_>>(),
                "approved_candidates":expected_approved.into_iter().map(identity_value).collect::<Vec<_>>()});
            let length = detail.to_string().len() + 1;
            if budget + length > MAX_REFUSAL_BYTES {
                break;
            }
            budget += length;
            details.push(detail);
        }
    }
    report["reported_count"] = details.len().into();
    report["omitted_count"] = (denied.len() - details.len()).into();
    report["complete"] = (details.len() == denied.len()).into();
    report["mismatches"] = details.into();
    Some(report.to_string())
}
fn gpu_name(name: &str) -> bool {
    name == "onnxruntime.dll"
        || driver_name(name)
        || [
            "onnxruntime_providers_",
            "cudnn",
            "cublas",
            "cudart",
            "cufft",
            "nvrtc",
            "nvjitlink",
        ]
        .iter()
        .any(|p| name.starts_with(p))
}
pub(super) fn baseline_allowed(current: &ModuleIdentity, approved: &[ModuleIdentity]) -> bool {
    !gpu_name(&current.name) || allowed(current, &[], approved)
}
/// An exact scientific host row, never a product driver-discovery policy.
pub(super) const DRIVER_PACKAGE: &str = "nv_dispwi.inf_amd64_b9ead98da9a85944";
pub(super) const DRIVER_COMPANION: &str =
    "driverstore/filerepository/nv_dispwi.inf_amd64_b9ead98da9a85944/nvcuda64.dll";
#[derive(Clone, Copy, Debug)]
pub(super) struct DriverImage {
    pub path: &'static str,
    pub sha: &'static str,
    pub bytes: u64,
}
pub(super) const DRIVER_IMAGES: [DriverImage; 7] = [
    DriverImage {
        path: DRIVER_COMPANION,
        sha: "4cf09882e346099361d9bc1e125d34f076d36bdcf5838da88132e74d1af3d5c6",
        bytes: 29_713_128,
    },
    DriverImage {
        path: "cryptnet.dll",
        sha: "440af621e97fd5fcb93f06393b8f3fd5af936de550e763380cc1f4c71cd6c795",
        bytes: 265_920,
    },
    DriverImage {
        path: "drvstore.dll",
        sha: "f2bd8b7cfa74c7f001262ebc1fd909e48e9c377b682fa38952eb104b3130e725",
        bytes: 1_591_312,
    },
    DriverImage {
        path: "nvapi64.dll",
        sha: "08ead220e3c0d1deda35473e7fc24614cf7fcdffec4941213514440312da1750",
        bytes: 5_533_456,
    },
    DriverImage {
        path: "driverstore/filerepository/nv_dispwi.inf_amd64_b9ead98da9a85944/nvdxgdmal64.dll",
        sha: "cd27e222beaa5602190c614f1378f0cefacdc2dd571df3696371ee7c2418a675",
        bytes: 626_408,
    },
    DriverImage {
        path: "ole32.dll",
        sha: "83f1f855f2985785a45976a6a5e2aacdc8900f61e373978668f5fa3456f09a66",
        bytes: 1_716_408,
    },
    DriverImage {
        path: "wldp.dll",
        sha: "2fd7baf055f1cd968ad1c783f7414d8aaa24ef93dc25fb1eeabc60afa3e5d4ea",
        bytes: 482_720,
    },
];
pub(super) fn scientific_baseline_allowed(
    current: &ModuleIdentity,
    approved: &[ModuleIdentity],
    scientific: bool,
) -> bool {
    if scientific
        && DRIVER_IMAGES
            .iter()
            .any(|row| row.path.rsplit('/').next() == Some(current.name.as_str()))
    {
        allowed(current, &[], approved)
    } else {
        baseline_allowed(current, approved)
    }
}
pub(super) fn validate_driver_image(path: &str, sha: &str, bytes: u64) -> anyhow::Result<()> {
    anyhow::ensure!(
        DRIVER_IMAGES
            .iter()
            .any(|row| row.path == path && row.sha == sha && row.bytes == bytes),
        "scientific CUDA driver image path/hash/size differs from reviewed closure"
    );
    Ok(())
}
pub(super) fn validate_driver_closure(actual: &[(&str, &str, u64)]) -> anyhow::Result<()> {
    anyhow::ensure!(
        actual.len() == 7
            && DRIVER_IMAGES.iter().all(|row| actual
                .iter()
                .filter(|item| **item == (row.path, row.sha, row.bytes))
                .count()
                == 1),
        "scientific CUDA driver closure is incomplete, duplicated or changed"
    );
    Ok(())
}
pub(super) const DRIVER_IMPORTS: [&str; 6] = [
    "advapi32.dll",
    "gdi32.dll",
    "kernel32.dll",
    "shlwapi.dll",
    "user32.dll",
    "version.dll",
];
pub(super) fn prepare_driver_companion<T>(
    product: bool,
    dispatcher_sha: &str,
    dispatcher_bytes: u64,
    authenticate: impl FnOnce() -> anyhow::Result<T>,
) -> anyhow::Result<Option<T>> {
    if product {
        return Ok(None);
    }
    anyhow::ensure!(
        dispatcher_sha == "f654977a2d34156c861a1434bbbbd1e9173b8d28b7e1c3f415bebca404df42be"
            && dispatcher_bytes == 4_466_920,
        "scientific CUDA dispatcher is outside the reviewed companion row"
    );
    Ok(Some(authenticate()?))
}
pub(super) fn driver_name(name: &str) -> bool {
    matches!(
        name,
        "nvcuda.dll" | "nvcuda64.dll" | "nvapi64.dll" | "nvdxgdmal64.dll"
    )
}
/// The pinned core must install its genuine ProviderHost before CUDA DLL static
/// initialization. This seam keeps options ownership testable without any DLLs.
pub(super) trait ProviderBridge {
    type Options;
    fn create(&self) -> anyhow::Result<Self::Options>;
    fn update_empty(&self, options: &Self::Options) -> anyhow::Result<()>;
    fn release(&self, options: Self::Options);
}

pub(super) fn prepare_provider<B: ProviderBridge>(
    name: &str,
    bridge: &B,
    reconcile: impl FnOnce() -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    if name != "onnxruntime_providers_cuda.dll" {
        return Ok(());
    }
    struct Held<'a, B: ProviderBridge> {
        bridge: &'a B,
        options: Option<B::Options>,
    }
    impl<B: ProviderBridge> Drop for Held<'_, B> {
        fn drop(&mut self) {
            if let Some(options) = self.options.take() {
                self.bridge.release(options);
            }
        }
    }
    let held = Held {
        bridge,
        options: Some(bridge.create()?),
    };
    bridge.update_empty(held.options.as_ref().unwrap())?;
    drop(held);
    reconcile()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(address: usize, name: &str, path: &str, inode: u64) -> ModuleIdentity {
        ModuleIdentity {
            address,
            name: name.into(),
            path: path.into(),
            id: ObjectIdentity::Unix { device: 1, inode },
        }
    }
    #[test]
    fn unlisted_new_names_paths_and_known_name_wrong_identity_refuse() {
        let old = row(1, "existing.dll", "system/existing.dll", 1);
        let good = row(0, "cuda.dll", "app/cuda.dll", 2);
        assert!(allowed(
            &old,
            std::slice::from_ref(&old),
            std::slice::from_ref(&good)
        ));
        assert!(allowed(
            &row(2, "cuda.dll", "app/cuda.dll", 2),
            std::slice::from_ref(&old),
            std::slice::from_ref(&good)
        ));
        for bad in [
            row(3, "helper.dll", "app/helper.dll", 3),
            row(3, "renamed-core.dll", "outside/renamed-core.dll", 4),
            row(3, "cuda.dll", "outside/cuda.dll", 2),
            row(3, "cuda.dll", "app/cuda.dll", 9),
            row(8, "existing.dll", "system/existing.dll", 1),
        ] {
            assert!(!allowed(
                &bad,
                std::slice::from_ref(&old),
                std::slice::from_ref(&good)
            ));
        }
    }

    struct Bridge<'a> {
        events: &'a std::cell::RefCell<Vec<&'static str>>,
        failure: &'static str,
    }
    impl ProviderBridge for Bridge<'_> {
        type Options = u32;
        fn create(&self) -> anyhow::Result<u32> {
            self.events.borrow_mut().push("create");
            anyhow::ensure!(self.failure != "create", "create refused");
            Ok(17)
        }
        fn update_empty(&self, options: &u32) -> anyhow::Result<()> {
            assert_eq!(*options, 17);
            self.events.borrow_mut().push("empty-update");
            assert_ne!(self.failure, "panic", "injected update panic");
            anyhow::ensure!(self.failure != "update", "update refused");
            Ok(())
        }
        fn release(&self, options: u32) {
            assert_eq!(options, 17);
            self.events.borrow_mut().push("release");
        }
    }
    #[test]
    fn cuda_bridge_initializes_then_releases_before_reconciling() -> anyhow::Result<()> {
        let events = std::cell::RefCell::new(Vec::new());
        let bridge = Bridge {
            events: &events,
            failure: "",
        };
        prepare_provider("onnxruntime_providers_cuda.dll", &bridge, || {
            events.borrow_mut().push("reconcile");
            Ok(())
        })?;
        assert_eq!(
            *events.borrow(),
            ["create", "empty-update", "release", "reconcile"]
        );
        Ok(())
    }
    #[test]
    fn non_cuda_modules_do_not_initialize_provider_or_reconcile_early() -> anyhow::Result<()> {
        let events = std::cell::RefCell::new(Vec::new());
        let bridge = Bridge {
            events: &events,
            failure: "create",
        };
        prepare_provider("onnxruntime_providers_shared.dll", &bridge, || {
            panic!("ordinary module must use the existing post-load reconciliation")
        })?;
        assert!(events.borrow().is_empty());
        Ok(())
    }
    #[test]
    fn bridge_create_refusal_cannot_update_release_or_reconcile() {
        let events = std::cell::RefCell::new(Vec::new());
        let bridge = Bridge {
            events: &events,
            failure: "create",
        };
        assert!(
            prepare_provider("onnxruntime_providers_cuda.dll", &bridge, || {
                panic!("failed creation cannot reconcile")
            })
            .is_err()
        );
        assert_eq!(*events.borrow(), ["create"]);
    }
    #[test]
    fn bridge_update_refusal_releases_original_options_without_reconcile() {
        let events = std::cell::RefCell::new(Vec::new());
        let bridge = Bridge {
            events: &events,
            failure: "update",
        };
        assert!(
            prepare_provider("onnxruntime_providers_cuda.dll", &bridge, || {
                panic!("failed update cannot reconcile")
            })
            .is_err()
        );
        assert_eq!(*events.borrow(), ["create", "empty-update", "release"]);
    }
    #[test]
    fn bridge_update_panic_releases_original_options() {
        let events = std::cell::RefCell::new(Vec::new());
        let bridge = Bridge {
            events: &events,
            failure: "panic",
        };
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                prepare_provider("onnxruntime_providers_cuda.dll", &bridge, || {
                    panic!("panicked update cannot reconcile")
                })
            }))
            .is_err()
        );
        assert_eq!(*events.borrow(), ["create", "empty-update", "release"]);
    }
    #[test]
    fn bridge_reconciliation_refusal_is_not_success_and_options_are_released() {
        let events = std::cell::RefCell::new(Vec::new());
        let bridge = Bridge {
            events: &events,
            failure: "",
        };
        assert!(
            prepare_provider("onnxruntime_providers_cuda.dll", &bridge, || {
                events.borrow_mut().push("reconcile");
                anyhow::bail!("unapproved module")
            })
            .is_err()
        );
        assert_eq!(
            *events.borrow(),
            ["create", "empty-update", "release", "reconcile"]
        );
    }

    const DISPATCHER: &str = "f654977a2d34156c861a1434bbbbd1e9173b8d28b7e1c3f415bebca404df42be";
    const COMPANION: &str = "4cf09882e346099361d9bc1e125d34f076d36bdcf5838da88132e74d1af3d5c6";
    const COMPANION_PATH: &str =
        "driverstore/filerepository/nv_dispwi.inf_amd64_b9ead98da9a85944/nvcuda64.dll";

    #[test]
    fn scientific_driver_row_authenticates_before_returning_custody() -> anyhow::Result<()> {
        let events = std::cell::RefCell::new(Vec::new());
        let held = prepare_driver_companion(false, DISPATCHER, 4_466_920, || {
            events.borrow_mut().push("authenticate");
            validate_driver_image(COMPANION_PATH, COMPANION, 29_713_128)?;
            Ok(17)
        })?;
        assert_eq!(held, Some(17));
        events.borrow_mut().push("load");
        assert_eq!(*events.borrow(), ["authenticate", "load"]);
        Ok(())
    }
    #[test]
    fn product_cannot_consume_scientific_companion_row() -> anyhow::Result<()> {
        let held: Option<()> = prepare_driver_companion(true, DISPATCHER, 4_466_920, || {
            panic!("product must not authenticate a scientific companion")
        })?;
        assert!(held.is_none());
        Ok(())
    }
    #[test]
    fn different_dispatcher_hash_or_size_refuses_before_companion_access() {
        for (sha, bytes) in [("different", 4_466_920), (DISPATCHER, 4_466_921)] {
            let result: anyhow::Result<Option<()>> =
                prepare_driver_companion(false, sha, bytes, || {
                    panic!("wrong dispatcher cannot authorize companion access")
                });
            assert!(result.is_err());
        }
    }
    #[test]
    fn companion_hash_and_size_are_exact_independent_literals() {
        assert!(validate_driver_image(COMPANION_PATH, "different", 29_713_128).is_err());
        assert!(validate_driver_image(COMPANION_PATH, COMPANION, 29_713_129).is_err());
    }
    #[test]
    fn wrong_companion_package_path_and_basename_refuse() {
        for path in [
            "driverstore/filerepository/other/nvcuda64.dll",
            "driverstore/filerepository/nv_dispwi.inf_amd64_b9ead98da9a85944/other.dll",
            "outside/nvcuda64.dll",
            "driverstore/filerepository/nv_dispwi.inf_amd64_b9ead98da9a85944/../nvcuda64.dll",
        ] {
            assert!(validate_driver_image(path, COMPANION, 29_713_128).is_err());
        }
    }
    #[test]
    fn companion_signature_parent_policy_and_deadline_refusals_cannot_load() {
        for refusal in [
            "signature",
            "parent policy",
            "reparse parent",
            "deadline",
            "PE imports",
        ] {
            let mut loaded = false;
            let result: anyhow::Result<Option<()>> =
                prepare_driver_companion(false, DISPATCHER, 4_466_920, || {
                    anyhow::bail!("{refusal}")
                });
            if result.is_ok() {
                loaded = true;
            }
            assert!(result.is_err());
            assert!(!loaded);
        }
    }
    #[test]
    fn companion_mapping_requires_exact_file_identity_and_path() {
        assert!(driver_name("nvcuda64.dll"));
        let approved = row(0, "nvcuda64.dll", COMPANION_PATH, 7);
        assert!(allowed(
            &row(3, "nvcuda64.dll", COMPANION_PATH, 7),
            &[],
            std::slice::from_ref(&approved)
        ));
        for bad in [
            row(3, "nvcuda64.dll", COMPANION_PATH, 8),
            row(3, "nvcuda64.dll", "outside/nvcuda64.dll", 7),
            row(3, "renamed.dll", COMPANION_PATH, 7),
        ] {
            assert!(!allowed(&bad, &[], std::slice::from_ref(&approved)));
        }
    }

    #[test]
    fn preexisting_companion_never_bypasses_explicit_approval() {
        let expected = row(0, "nvcuda64.dll", COMPANION_PATH, 7);
        assert!(baseline_allowed(
            &row(3, "nvcuda64.dll", COMPANION_PATH, 7),
            std::slice::from_ref(&expected)
        ));
        for preloaded in [
            row(3, "nvcuda64.dll", COMPANION_PATH, 8),
            row(3, "nvcuda64.dll", "outside/nvcuda64.dll", 7),
        ] {
            assert!(!baseline_allowed(
                &preloaded,
                std::slice::from_ref(&expected)
            ));
            assert!(!baseline_allowed(&preloaded, &[]));
        }
        assert!(baseline_allowed(
            &row(1, "existing.dll", "system/existing.dll", 1),
            &[]
        ));
    }

    #[test]
    fn scientific_driver_package_and_system_import_roster_are_literal() {
        assert_eq!(DRIVER_PACKAGE, "nv_dispwi.inf_amd64_b9ead98da9a85944");
        assert_eq!(
            DRIVER_IMPORTS,
            [
                "advapi32.dll",
                "gdi32.dll",
                "kernel32.dll",
                "shlwapi.dll",
                "user32.dll",
                "version.dll",
            ]
        );
        assert_eq!(
            DRIVER_COMPANION,
            "driverstore/filerepository/nv_dispwi.inf_amd64_b9ead98da9a85944/nvcuda64.dll"
        );
    }

    fn report(
        stage: &str,
        current: &[ModuleIdentity],
        baseline: &[ModuleIdentity],
        approved: &[ModuleIdentity],
    ) -> serde_json::Value {
        serde_json::from_str(&refusal_report(stage, current, baseline, approved).unwrap()).unwrap()
    }
    #[test]
    fn refusal_reports_all_new_images_in_deterministic_order_at_exact_stage() {
        let actual = [
            row(4, "z.dll", "system/z.dll", 4),
            row(3, "a.dll", "system/a.dll", 3),
        ];
        let value = report("after-cuInit", &actual, &[], &[]);
        assert_eq!(value["schema"], "airk.private.cuda-module-refusal/1");
        assert_eq!(value["stage"], "after-cuInit");
        assert_eq!(value["unapproved_count"], 2);
        assert_eq!(value["reported_count"], 2);
        assert_eq!(value["omitted_count"], 0);
        assert_eq!(value["complete"], true);
        assert_eq!(
            value["mismatches"][0],
            serde_json::json!({
            "classification":"new_image", "actual":{"name":"a.dll","path":"system/a.dll","address":3,
                "id":{"kind":"unix","device":1,"inode":3}}, "baseline_candidates":[],"approved_candidates":[]})
        );
        assert_eq!(value["mismatches"][1]["actual"]["name"], "z.dll");
        assert_eq!(
            value,
            report(
                "after-cuInit",
                &[actual[1].clone(), actual[0].clone()],
                &[],
                &[]
            )
        );
    }
    #[test]
    fn refusal_distinguishes_same_object_address_reload_from_identity_drift() {
        let baseline = [row(1, "base.dll", "system/base.dll", 7)];
        let reloaded = report(
            "runtime-recheck",
            &[row(9, "base.dll", "system/base.dll", 7)],
            &baseline,
            &[],
        );
        assert_eq!(
            reloaded["mismatches"][0]["classification"],
            "address_reload"
        );
        assert_eq!(
            reloaded["mismatches"][0]["baseline_candidates"][0]["address"],
            1
        );
        assert_eq!(reloaded["mismatches"][0]["actual"]["address"], 9);
        for changed in [
            row(1, "base.dll", "system/base.dll", 8),
            row(1, "base.dll", "outside/base.dll", 7),
        ] {
            assert_eq!(
                report("runtime-recheck", &[changed], &baseline, &[])["mismatches"][0]["classification"],
                "path_or_identity_drift"
            );
        }
    }
    #[test]
    fn refusal_preserves_full_windows_actual_and_approved_object_ids() {
        let expected = ModuleIdentity {
            address: 0,
            name: "cuda.dll".into(),
            path: "app/cuda.dll".into(),
            id: ObjectIdentity::Windows {
                volume_serial: 42,
                file_id: [7; 16],
            },
        };
        let actual = ModuleIdentity {
            address: 123,
            id: ObjectIdentity::Windows {
                volume_serial: 43,
                file_id: [8; 16],
            },
            ..expected.clone()
        };
        let value = report(
            "after-provider-bridge",
            std::slice::from_ref(&actual),
            &[],
            std::slice::from_ref(&expected),
        );
        assert_eq!(
            value["mismatches"][0]["classification"],
            "path_or_identity_drift"
        );
        assert_eq!(
            value["mismatches"][0]["actual"]["id"],
            serde_json::json!({"kind":"windows","volume_serial":43,"file_id":[8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8]})
        );
        assert_eq!(
            value["mismatches"][0]["approved_candidates"][0]["id"],
            serde_json::json!({"kind":"windows","volume_serial":42,"file_id":[7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7]})
        );
        assert!(!allowed(&actual, &[], &[expected]));
    }
    #[test]
    fn diagnostic_does_not_refuse_unchanged_baseline_or_approved_relocation() {
        let baseline = [row(1, "base.dll", "system/base.dll", 1)];
        let approved = [row(0, "cuda.dll", "app/cuda.dll", 2)];
        assert!(
            refusal_report(
                "before-placement",
                &[baseline[0].clone(), row(99, "cuda.dll", "app/cuda.dll", 2)],
                &baseline,
                &approved
            )
            .is_none()
        );
    }
    #[test]
    fn bounded_refusal_discloses_omission_without_truncating_included_identity() {
        let current = [
            row(1, "a.dll", "system/a.dll", 1),
            row(2, "z.dll", &"x".repeat(MAX_REFUSAL_BYTES), 2),
        ];
        let raw = refusal_report("after-cuInit", &current, &[], &[]).unwrap();
        assert!(raw.len() <= 1 << 20);
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["complete"], false);
        assert_eq!(value["unapproved_count"], 2);
        assert_eq!(value["reported_count"], 1);
        assert_eq!(value["omitted_count"], 1);
        assert_eq!(value["mismatches"][0]["actual"]["path"], "system/a.dll");
        assert!(!allowed(&current[1], &[], &[]));
    }
    #[test]
    fn oversized_diagnostic_stage_is_explicit_incomplete_refusal() {
        let value = report(
            &"x".repeat(513),
            &[row(1, "a.dll", "system/a.dll", 1)],
            &[],
            &[],
        );
        assert_eq!(value["stage"], "diagnostic-stage-overflow");
        assert_eq!(value["complete"], false);
        assert_eq!(value["omitted_count"], 1);
    }

    fn literal_driver_closure() -> Vec<(&'static str, &'static str, u64)> {
        vec![
            (
                "driverstore/filerepository/nv_dispwi.inf_amd64_b9ead98da9a85944/nvcuda64.dll",
                "4cf09882e346099361d9bc1e125d34f076d36bdcf5838da88132e74d1af3d5c6",
                29_713_128,
            ),
            (
                "cryptnet.dll",
                "440af621e97fd5fcb93f06393b8f3fd5af936de550e763380cc1f4c71cd6c795",
                265_920,
            ),
            (
                "drvstore.dll",
                "f2bd8b7cfa74c7f001262ebc1fd909e48e9c377b682fa38952eb104b3130e725",
                1_591_312,
            ),
            (
                "nvapi64.dll",
                "08ead220e3c0d1deda35473e7fc24614cf7fcdffec4941213514440312da1750",
                5_533_456,
            ),
            (
                "driverstore/filerepository/nv_dispwi.inf_amd64_b9ead98da9a85944/nvdxgdmal64.dll",
                "cd27e222beaa5602190c614f1378f0cefacdc2dd571df3696371ee7c2418a675",
                626_408,
            ),
            (
                "ole32.dll",
                "83f1f855f2985785a45976a6a5e2aacdc8900f61e373978668f5fa3456f09a66",
                1_716_408,
            ),
            (
                "wldp.dll",
                "2fd7baf055f1cd968ad1c783f7414d8aaa24ef93dc25fb1eeabc60afa3e5d4ea",
                482_720,
            ),
        ]
    }
    #[test]
    fn exact_seven_image_scientific_closure_matches_independent_literals() -> anyhow::Result<()> {
        let expected = literal_driver_closure();
        assert_eq!(
            DRIVER_IMAGES
                .iter()
                .map(|row| (row.path, row.sha, row.bytes))
                .collect::<Vec<_>>(),
            expected
        );
        validate_driver_closure(&expected)?;
        for (path, sha, bytes) in expected {
            validate_driver_image(path, sha, bytes)?;
        }
        Ok(())
    }
    #[test]
    fn each_missing_or_duplicated_driver_member_refuses_the_whole_closure() {
        for index in 0..7 {
            let mut partial = literal_driver_closure();
            partial.remove(index);
            assert!(validate_driver_closure(&partial).is_err());
            let mut duplicate = literal_driver_closure();
            duplicate[index] = duplicate[(index + 1) % 7];
            assert!(validate_driver_closure(&duplicate).is_err());
        }
    }
    #[test]
    fn each_scientific_driver_path_hash_and_size_is_exact() {
        for index in 0..7 {
            for field in 0..3 {
                let mut changed = literal_driver_closure();
                match field {
                    0 => changed[index].0 = "outside/shadow.dll",
                    1 => changed[index].1 = "wrong",
                    _ => changed[index].2 += 1,
                }
                let (path, sha, bytes) = changed[index];
                assert!(validate_driver_image(path, sha, bytes).is_err());
                assert!(validate_driver_closure(&changed).is_err());
            }
        }
    }
    #[test]
    fn product_never_attempts_the_seven_image_scientific_closure() -> anyhow::Result<()> {
        let value: Option<Vec<()>> = prepare_driver_companion(true, DISPATCHER, 4_466_920, || {
            panic!("product must not access scientific rows")
        })?;
        assert!(value.is_none());
        Ok(())
    }
    #[test]
    fn scientific_baseline_refuses_every_shadowed_closure_member() {
        for (index, (path, _, _)) in literal_driver_closure().iter().enumerate() {
            let name = path.rsplit('/').next().unwrap();
            let expected = row(0, name, path, index as u64 + 1);
            let exact = row(17, name, path, index as u64 + 1);
            assert!(scientific_baseline_allowed(
                &exact,
                std::slice::from_ref(&expected),
                true
            ));
            assert!(!scientific_baseline_allowed(&exact, &[], true));
            for bad in [
                row(17, name, "outside/shadow.dll", index as u64 + 1),
                row(17, name, path, 999),
            ] {
                assert!(!scientific_baseline_allowed(
                    &bad,
                    std::slice::from_ref(&expected),
                    true
                ));
            }
        }
    }
    #[test]
    fn product_os_baseline_unchanged_but_nvidia_helpers_remain_sensitive() {
        for name in ["cryptnet.dll", "drvstore.dll", "ole32.dll", "wldp.dll"] {
            let existing = row(1, name, "system/existing.dll", 7);
            assert!(scientific_baseline_allowed(&existing, &[], false));
            assert!(!scientific_baseline_allowed(&existing, &[], true));
        }
        for name in [
            "nvcuda.dll",
            "nvcuda64.dll",
            "nvapi64.dll",
            "nvdxgdmal64.dll",
        ] {
            assert!(!scientific_baseline_allowed(
                &row(1, name, "outside/shadow.dll", 7),
                &[],
                false
            ));
        }
    }
}
