// SPDX-License-Identifier: Apache-2.0
use super::*;
use serde_json::json;
use std::cell::Cell;

#[derive(Default)]
struct Memory {
    files: BTreeMap<String, Vec<u8>>,
    rechecked: Cell<bool>,
    fixed: BTreeMap<String, (Sha256Digest, u64)>,
}

impl Memory {
    fn put(&mut self, path: &Path, raw: &[u8]) -> Value {
        let path = path.to_str().unwrap();
        self.files.insert(path.into(), raw.into());
        json!({
            "path": path,
            "bytes": raw.len(),
            "sha256": Sha256Digest::from_bytes(Sha256::digest(raw).into()),
        })
    }

    fn fixed(&mut self, path: &Path, sha: &str) -> Value {
        let path = path.to_str().unwrap();
        let sha: Sha256Digest = sha.parse().unwrap();
        self.fixed.insert(path.into(), (sha, 11));
        json!({"path":path,"bytes":11,"sha256":sha})
    }

    fn json(&mut self, path: &Path, value: &Value) -> Value {
        self.put(path, &serde_json::to_vec(value).unwrap())
    }
}

impl Source for Memory {
    fn read(&self, pin: &ResourceFilePin, maximum: u64) -> Result<Vec<u8>> {
        let bytes = self.files.get(&pin.path).context("fixture file missing")?;
        ensure!(bytes.len() as u64 <= maximum, "fixture capacity");
        Ok(bytes.clone())
    }

    fn read_retained_configure_receipt(&self, root: &Path, maximum: u64) -> Result<Vec<u8>> {
        let log = root.join("logs").join("configure.receipt.json");
        let raw = self
            .files
            .get(log.to_str().unwrap())
            .context("configure receipt missing from held fixture")?;
        ensure!(
            raw.len() as u64 <= maximum,
            "fixture configure receipt capacity"
        );
        Ok(raw.clone())
    }

    fn authenticate(&self, pin: &ResourceFilePin, bytes: u64, maximum: u64) -> Result<()> {
        if let Some(expected) = self.fixed.get(&pin.path) {
            ensure!(
                *expected == (pin.sha256, bytes) && bytes <= maximum,
                "fixed fixture input differs"
            );
            return Ok(());
        }
        let raw = super::super::read(self, pin, maximum)?;
        ensure!(raw.len() as u64 == bytes, "fixture byte count differs");
        Ok(())
    }

    fn recheck(&self) -> Result<()> {
        self.rechecked.set(true);
        Ok(())
    }
}

fn time(minute: u32, seconds: u32) -> String {
    format!("2026-11-18T00:{minute:02}:{seconds:02}+00:00")
}

fn stage(
    fs: &mut Memory,
    root: &Path,
    name: &str,
    argv: &[String],
    args: &str,
    minute: u32,
    deadline: u64,
) -> Value {
    let log = root.join("logs").join(name);
    let tree = json!({
        "schema":"airk.private.process-tree-result/1", "command_pid":31, "wrapper_pid":29,
        "exit_code":0, "start_utc":time(minute, 0), "finished_utc":time(minute, 1),
        "wait":"Start-Process-Wait-process-and-descendants", "powershell":"7.5.2",
    });
    let request = json!({
        "schema":"airk.private.process-tree-command/1",
        "executable":argv[0], "arguments":args, "cwd":root,
        "stdout":log.with_extension("stdout.log"), "stderr":log.with_extension("stderr.log"),
        "result":log.with_extension("tree.json"), "started":log.with_extension("started.json"),
        "started_event":null, "done_event":null,
    });
    json!({
        "stage":name, "argv":argv, "cwd":root, "original_deadline_monotonic":deadline,
        "owned":{
            "exit_code":0,"timed_out":false,"log_oversized":false,
            "owned_descendants_settled":true,"owned_job_forced_termination":false,
            "parent_assigned_to_job":false,"child_assigned_before_resume":true,
            "owned_job_final_active_processes":0,"owned_job_final_member_pids":[],
        },
        "actual_tree_result":tree,
        "files":[
            fs.json(&log.with_extension("request.json"), &request),
            fs.put(&log.with_extension("stdout.log"), b""),
            fs.put(&log.with_extension("stderr.log"), b""),
            fs.json(&log.with_extension("tree.json"), &tree),
            fs.json(&log.with_extension("started.json"), &json!({
                "pid":31,"wrapper_pid":29,"started_utc":time(minute, 0),
            })),
            fs.put(&log.with_extension("outer.log"), b"outer receipt log"),
        ],
    })
}

#[test]
fn cuda_fresh_windows_request_quoting_matches_cpython_controls() {
    assert_eq!(
        command_line(&[
            "-S".into(),
            r"C:\source tree".into(),
            r#"a"b"#.into(),
            "".into()
        ]),
        r#"-S "C:\source tree" a\"b """#,
    );
    assert_eq!(
        command_line(&[r"C:\directory with spaces\".into()]),
        r#""C:\directory with spaces\\""#
    );
}

#[test]
fn cuda_fresh_windows_preserves_original_deadline_and_utc() -> Result<()> {
    let configured = json!({
        "started_utc":time(0,0), "finished_utc":time(1,2),
        "elapsed_seconds":62, "configure":{"original_deadline_monotonic":500},
    });
    let compiled = json!({
        "started_utc":time(1,3), "finished_utc":time(4,18), "elapsed_seconds":195,
        "original_deadline_monotonic":500, "deadline_monotonic":490,
    });
    let recipe = json!({"wall_seconds":300});
    let valid = windows(&configured, &compiled, &recipe)?;
    assert_eq!(valid.original, 500.0);
    assert_eq!(valid.compile_cutoff, 490.0);
    for (key, value) in [
        ("deadline_monotonic", json!(501)),
        ("original_deadline_monotonic", json!(600)),
        ("finished_utc", json!(time(5, 0))),
        ("started_utc", json!(time(1, 0))),
    ] {
        let mut invalid = compiled.clone();
        invalid[key] = value;
        assert!(windows(&configured, &invalid, &recipe).is_err(), "{key}");
    }
    Ok(())
}

#[test]
fn cuda_fresh_windows_refuses_attempt_rewriting_and_bad_leaf_custody() -> Result<()> {
    let root = std::env::current_dir()?.join("fixture");
    let mut fs = Memory::default();
    let executable = root.join("cmake.exe").to_string_lossy().into_owned();
    let argv = vec![executable, "--version".into()];
    let genuine = stage(&mut fs, &root, "build", &argv, "--version", 3, 430);
    assert!(verify_stage(&fs, &root, &genuine, "build", &argv, 430.0).is_ok());
    for (key, value) in [
        ("owned_job_forced_termination", json!(true)),
        ("owned_job_final_active_processes", json!(1)),
        ("owned_job_final_member_pids", json!([42])),
        ("child_assigned_before_resume", json!(false)),
    ] {
        let mut invalid = genuine.clone();
        invalid["owned"][key] = value;
        assert!(
            verify_stage(&fs, &root, &invalid, "build", &argv, 430.0).is_err(),
            "{key}"
        );
    }
    assert!(verify_stage(&fs, &root, &genuine, "build", &argv, 431.0).is_err());
    let mut altered = argv.clone();
    altered[1] = "--help".into();
    assert!(verify_stage(&fs, &root, &genuine, "build", &altered, 430.0).is_err());
    let mut invalid = genuine.clone();
    invalid["files"][0]["bytes"] = json!(0);
    assert!(verify_stage(&fs, &root, &invalid, "build", &argv, 430.0).is_err());
    fs.files
        .remove(genuine["files"][2]["path"].as_str().unwrap());
    assert!(verify_stage(&fs, &root, &genuine, "build", &argv, 430.0).is_err());
    Ok(())
}

#[test]
fn cuda_fresh_windows_requires_exact_live_same_build_provider_pair() -> Result<()> {
    let root = std::env::current_dir()?.join("fixture");
    let mut fs = Memory::default();
    let shared = fs.put(&root.join("onnxruntime_providers_shared.dll"), b"shared");
    let cuda = fs.put(&root.join("onnxruntime_providers_cuda.dll"), b"cuda");
    let pair = json!({"providers":[shared, cuda]});
    let observed = verify_providers(&fs, &root, &pair)?;
    assert_eq!(observed[0].name, "onnxruntime_providers_shared.dll");
    assert_eq!(observed[1].name, "onnxruntime_providers_cuda.dll");
    assert_eq!(observed[0].bytes, 6);
    assert_eq!(observed[1].bytes, 4);
    let mut swapped = pair.clone();
    swapped["providers"].as_array_mut().unwrap().swap(0, 1);
    assert!(verify_providers(&fs, &root, &swapped).is_err());
    fs.files.insert(
        pair["providers"][1]["path"].as_str().unwrap().into(),
        b"else".into(),
    );
    assert!(verify_providers(&fs, &root, &pair).is_err());
    Ok(())
}

#[test]
fn cuda_fresh_windows_uses_independent_authority_before_consuming_candidate_source() -> Result<()> {
    let anchor = std::env::current_dir()?.join("fixture");
    let root = anchor.join("build");
    let mut fs = Memory::default();
    let driver = fs.put(&anchor.join("driver.py"), b"reviewed driver");
    let recipe = fs.put(
        &anchor.join("recipe.json"),
        b"independently reviewed recipe",
    );
    let configure_pin = fs.put(&root.join("fresh-configure.json"), b"configured");
    let receipt = fs.json(
        &root.join("fresh-compile.json"),
        &json!({
            "schema":"airk.private.cuda-fresh-compiled/1", "status":"COMPILED_UNQUALIFIED",
            "qualification":false, "driver":driver, "configured":configure_pin, "recipe":recipe,
        }),
    );
    let raw = pin(&receipt, false)?.resource();
    let proof = json!({
        "schema":"airk.private.cuda-fresh-static-build-proof/1", "build_receipt":receipt,
    });
    let wrong = Sha256Digest::from_bytes([1; 32]);
    assert!(
        validate(&fs, &raw, &proof, &anchor, wrong, wrong)
            .err()
            .unwrap()
            .to_string()
            .contains("approval")
    );
    let mut raw = raw;
    raw.sha256 = wrong;
    assert!(
        validate(&fs, &raw, &proof, &anchor, wrong, wrong)
            .err()
            .unwrap()
            .to_string()
            .contains("staged producer SHA")
    );
    assert!(!fs.rechecked.get());
    Ok(())
}

fn ninja_fixture(root: &Path) -> String {
    let source = root.join("source/onnxruntime/core/session/onnxruntime_c_api.cc");
    let source = source
        .to_str()
        .unwrap()
        .replace('\\', "/")
        .replace(':', "$:");
    format!(
        concat!(
            "build CMakeFiles/session.dir/onnxruntime_c_api.cc.obj: CXX_COMPILER__onnxruntime_session_Release {source}\n",
            "  FLAGS = /std:c++20\n",
            "build CMakeFiles/cuda.dir/kernel.obj: CUDA_COMPILER__onnxruntime_providers_cuda_Release kernel.cu\n",
            "build onnxruntime_session.lib: CXX_STATIC_LIBRARY_LINKER__onnxruntime_session_Release CMakeFiles/session.dir/onnxruntime_c_api.cc.obj\n",
            "build all: phony onnxruntime_session.lib\n",
            "build re2: phony re2.lib\n",
            "build onnxruntime_providers_cuda.dll: CUDA_MODULE_LIBRARY_LINKER__onnxruntime_providers_cuda_Release kernel.obj\n",
            "build onnxruntime_providers_shared.dll onnxruntime_providers_shared.lib: CXX_SHARED_LIBRARY_LINKER__onnxruntime_providers_shared_Release shared.obj\n",
        ),
        source = source
    )
}

#[test]
fn cuda_fresh_windows_recomputes_ninja_and_rejects_resealed_graph_changes() -> Result<()> {
    let root = std::env::current_dir()?.join("fixture");
    let build = root.join("b").join("Release");
    let full = ninja_fixture(&root);
    let object = "CMakeFiles/session.dir/onnxruntime_c_api.cc.obj";
    let maximum = build.to_str().unwrap().chars().count() + 1 + object.len();
    let graph = json!({"objects":2,"max_object_path":maximum});
    verify_ninja_graph(&root, &build, &graph, "cache", &full, "rules")?;
    for (failed, expected) in [
        (full.replace("/std:c++20", "/std:c++17"), "C++20"),
        (
            full.replace("/std:c++20", "/std:c++20 --ALLOW-UNSUPPORTED-COMPILER"),
            "unsupported CUDA",
        ),
        (
            full.replace("build re2:", "build foreign:"),
            "target is missing: re2",
        ),
        (
            full.clone() + "\nbuild all: phony foreign\n",
            "target is ambiguous: all",
        ),
        (
            full.replace("CXX_STATIC_LIBRARY_LINKER", "CXX_SHARED_LIBRARY_LINKER"),
            "not a static archive",
        ),
        (
            full.replace(
                "session_Release CMakeFiles/session.dir/",
                "session_Release || CMakeFiles/session.dir/",
            ),
            "actual session archive",
        ),
        (
            full.replace(
                "/source/onnxruntime/core/session/",
                "/stale/onnxruntime/core/session/",
            ),
            "actual session archive",
        ),
        (
            full.replace("CMakeFiles/cuda.dir/kernel.obj:", "../kernel.obj:"),
            "object escapes build root",
        ),
        (
            full.replace(
                "CMakeFiles/cuda.dir/kernel.obj:",
                &format!("{}.obj:", "x".repeat(241)),
            ),
            "count or maximum path",
        ),
    ] {
        let error = verify_ninja_graph(&root, &build, &graph, "cache", &failed, "rules")
            .expect_err("valid file bytes must still satisfy their graph semantics");
        assert!(error.to_string().contains(expected), "{expected}: {error}");
    }
    for (cache, rules) in [
        ("-allow-unsupported-compiler", "rules"),
        ("cache", "-allow-unsupported-compiler"),
    ] {
        assert!(verify_ninja_graph(&root, &build, &graph, cache, &full, rules).is_err());
    }
    for false_graph in [
        json!({"objects":3,"max_object_path":maximum}),
        json!({"objects":2,"max_object_path":maximum - 1}),
    ] {
        assert!(verify_ninja_graph(&root, &build, &false_graph, "cache", &full, "rules").is_err());
    }
    Ok(())
}

#[test]
fn cuda_fresh_windows_reconstructs_proof_and_returns_only_observed_providers() -> Result<()> {
    verify_producer_fixture(None, ["2", "2"], None)
}

#[test]
fn cuda_fresh_windows_binds_worker_count_in_both_retained_commands() -> Result<()> {
    const REFUSAL: &str = "fresh stage command or natural owned settlement differs";
    verify_producer_fixture(Some(8), ["8", "8"], None)?;
    verify_producer_fixture(Some(8), ["2", "8"], Some(REFUSAL))?;
    verify_producer_fixture(Some(8), ["8", "2"], Some(REFUSAL))?;
    assert_eq!(parallel_workers(&json!({"parallel_workers":2}))?, 2);
    for workers in [
        json!(null),
        json!(true),
        json!("8"),
        json!(8.0),
        json!(-2),
        json!(4),
    ] {
        assert!(parallel_workers(&json!({"parallel_workers":workers})).is_err());
    }
    Ok(())
}

fn verify_producer_fixture(
    workers: Option<u64>,
    stage_workers: [&str; 2],
    expected_refusal: Option<&str>,
) -> Result<()> {
    let anchor = std::env::current_dir()?.join("fixture");
    let root = anchor.join("fresh");
    let build = root.join("b").join("Release");
    let repo = anchor.join("repo");
    let mut fs = Memory::default();
    let driver = fs.put(
        &repo.join("scripts/cuda-static-runtime/build/fresh_compile.py"),
        b"driver",
    );
    let cpu_owner = fs.put(
        &repo.join("scripts/provision-static-onnx-runtime.py"),
        b"cpu owner",
    );
    let checked_guard = fs.put(
        &repo.join("scripts/dev/assert-human-execution.ps1"),
        b"guard",
    );
    let powershell = fs.put(&repo.join("pwsh.exe"), b"PowerShell image");
    let cpu = fs.json(
        &repo.join("scripts/static-onnx-runtime-lock.json"),
        &json!({"ort_version":"1.30.0","upstream_commit":"rev"}),
    );
    let lock = fs.json(
        &repo.join("scripts/cuda-static-runtime/build/r8/cuda-runtime-producer-lock.json"),
        &json!({
            "target":"x86_64-pc-windows-msvc","ort_version":"1.30.0","upstream_commit":"rev",
            "cpu_lock_sha256":cpu["sha256"],"cpu_owner_sha256":cpu_owner["sha256"],
        }),
    );
    let source_archive = fs.put(&root.join("onnx.tar.gz"), b"onnx source");
    let cl = fs.put(&root.join("cl.exe"), b"cl image");
    let cmake = fs.put(&root.join("cmake.exe"), b"cmake image");
    let tools = json!({
        "compiler":cl, "cmake":cmake, "ninja":fs.put(&root.join("ninja.exe"), b"ninja"),
        "python":fs.put(&root.join("python.exe"), b"python"), "patch":fs.put(&root.join("patch.exe"), b"patch"),
        "vcvars":fs.put(&root.join("vcvars.bat"), b"developer tools"),
        "windows_sdk":"10.0.26100.0", "msvc_toolset":"14.44.35207",
    });
    let nvcc = fs.put(&root.join("nvcc.exe"), b"nvcc");
    let mut recipe = json!({
        "schema":"airk.private.cuda-fresh-configure-recipe/1", "repository_root":repo, "fresh_output":root,
        "execution_authorized":true,"qualification":false,"wall_seconds":600,
        "msvc_ciso646_compat":true,"output_audit_owner":driver,
        "tools":tools,"compiler_tools":[cl],"native_inputs":[nvcc],
        "environment":{"VCTOOLSVERSION":"14.44.35207","WINDOWSSDKVERSION":"10.0.26100.0\\","PATHEXT":".EXE;.CMD"},
        "owners":{
            "driver":fs.put(&repo.join("scripts/cuda-static-runtime/build/fresh_configure.py"),b"configure"),
            "guards":fs.put(&repo.join("scripts/cuda-component-execution.py"),b"guards"),
            "pin_reader":fs.put(&repo.join("scripts/cuda-static-incremental.py"),b"pin reader"),
            "source_stage":fs.put(&repo.join("scripts/cuda-static-runtime/build/fresh_source.py"),b"source stage"),
            "process_stage":fs.put(&repo.join("scripts/cuda-static-runtime/build/fresh_process.py"),b"process stage"),
            "cpu":cpu_owner,"lock":lock,"cpu_lock":cpu,
            "r8":fs.put(&repo.join("scripts/cuda-static-runtime/build/r8/provision_cuda_runtime.py"),b"r8 module"),
            "tree_waiter":fs.put(&repo.join("scripts/cuda-static-runtime/build/r12/wait_process_tree.ps1"),b"tree waiter"),
        },
        "checked_guard":checked_guard,"powershell":powershell,
        "source_archive":source_archive,"dependency_archives":{},
    });
    if let Some(workers) = workers {
        recipe["parallel_workers"] = json!(workers);
    }
    let recipe_pin = fs.json(&anchor.join("recipe.json"), &recipe);
    let collector = fs.fixed(
        &root.join("source/onnxruntime/core/session/onnxruntime_c_api.cc"),
        "db00479a06d0a1e8fadf4cc834ffe5a6da0b745ee2820b06a0821fb1dbeca97b",
    );
    let compatibility_source = "cmake/onnxruntime_providers_cuda.cmake";
    let compatibility_pin = fs.fixed(
        &root.join("source").join(compatibility_source),
        "82733200570c4dfe848cbcda34d05407417cbbb236ba0bb9dffe9fba35149271",
    );
    let source_inventory = json!([{
        "path":compatibility_source,
        "bytes":compatibility_pin["bytes"],
        "sha256":compatibility_pin["sha256"],
    }]);
    let mut patches = Vec::new();
    for label in [
        "loader",
        "offline",
        "final-placement",
        "initialized-public-api",
        "final-graph",
        "frontend-arm64",
        "frontend-authenticated-imports",
        "msvc-cuda-ciso646",
    ] {
        let original_path = repo.join(format!("{label}.patch"));
        let copied_path = root.join("patch-inputs").join(format!("{label}.patch"));
        let (original, snapshot) = if label == "final-graph" {
            (
                fs.fixed(
                    &original_path,
                    "a1a95c9b4401e302d7046159f882fdafa735bf7b32542314f75b333b902870c9",
                ),
                fs.fixed(
                    &copied_path,
                    "5222d2634878ac5e1c68264b6d521f0ca95df3a47397e3cf167b3ff7de844d58",
                ),
            )
        } else if label == "msvc-cuda-ciso646" {
            let original_path =
                repo.join("scripts/cuda-static-runtime/build/msvc-cuda-ciso646.patch");
            let sha = "bd511d6bdf8ce72d0983ec995f17ffb40194a1846c130ce9ee4c67c354566c60";
            (fs.fixed(&original_path, sha), fs.fixed(&copied_path, sha))
        } else {
            (
                fs.put(&original_path, label.as_bytes()),
                fs.put(&copied_path, label.as_bytes()),
            )
        };
        patches.push(json!({"stage":label,"original":original,"snapshot":snapshot}));
    }
    let mut configure_args = vec![
        cmake["path"].as_str().unwrap().to_owned(),
        "-S".into(),
        root.join("source/cmake").to_str().unwrap().into(),
        "-B".into(),
        build.to_str().unwrap().into(),
        "-G".into(),
        "Ninja".into(),
    ];
    let definitions = [
        ("CMAKE_BUILD_TYPE", "Release"),
        ("CMAKE_CXX_STANDARD", "20"),
        ("CMAKE_CUDA_ARCHITECTURES", "86"),
        ("CMAKE_OBJECT_PATH_MAX", "240"),
        ("onnxruntime_USE_CUDA", "ON"),
        ("onnxruntime_BUILD_CUDA_EP_AS_PLUGIN", "OFF"),
        ("onnxruntime_USE_FLASH_ATTENTION", "ON"),
        ("onnxruntime_USE_MEMORY_EFFICIENT_ATTENTION", "ON"),
        ("FETCHCONTENT_UPDATES_DISCONNECTED", "ON"),
        ("CMAKE_C_COMPILER", cl["path"].as_str().unwrap()),
        ("CMAKE_CXX_COMPILER", cl["path"].as_str().unwrap()),
        ("CMAKE_CUDA_HOST_COMPILER", cl["path"].as_str().unwrap()),
        (
            "CMAKE_MAKE_PROGRAM",
            tools["ninja"]["path"].as_str().unwrap(),
        ),
        (
            "Python3_EXECUTABLE",
            tools["python"]["path"].as_str().unwrap(),
        ),
        ("Patch_EXECUTABLE", tools["patch"]["path"].as_str().unwrap()),
        ("CMAKE_CUDA_COMPILER", nvcc["path"].as_str().unwrap()),
    ];
    let mut cache = String::new();
    for (key, value) in definitions {
        cache.push_str(&format!("{key}:STRING={value}\n"));
        configure_args.push(format!("-D{key}={value}"));
    }
    let cache = fs.put(&build.join("CMakeCache.txt"), cache.as_bytes());
    let ninja_path = build.join("build.ninja");
    let ninja_text = ninja_fixture(&root);
    let ninja = fs.put(&ninja_path, ninja_text.as_bytes());
    let rules = fs.put(&build.join("CMakeFiles/rules.ninja"), b"rules");
    let maximum = build.to_str().unwrap().chars().count()
        + 1
        + "CMakeFiles/session.dir/onnxruntime_c_api.cc.obj".len();
    let graph = json!({"cache_sha256":cache["sha256"],"ninja_sha256":ninja["sha256"],
        "rules_sha256":rules["sha256"],"checked_flags":16,"objects":2,"max_object_path":maximum,
        "final_capi_source_sha256":collector["sha256"]});
    let configure = stage(
        &mut fs,
        &root,
        "configure",
        &configure_args,
        &command_line(&configure_args[1..]),
        1,
        900,
    );
    let retained_configure_path = root.join("logs").join("configure.receipt.json");
    fs.json(&retained_configure_path, &configure);
    let configured = json!({
        "schema":"airk.private.cuda-fresh-configured/1","status":"CONFIGURED_UNQUALIFIED",
        "qualification":false,"recipe":recipe_pin,"toolchain":tools,"configure":configure,
        "configured_graph":graph,"collector_source":collector,"source_inventory":source_inventory,
        "frontend_inventory":[],"mirror_inventory":[],"patch_inputs":patches,
        "started_utc":time(0,0),"finished_utc":time(1,50),"elapsed_seconds":110,
    });
    let configured_pin = fs.json(&root.join("fresh-configure.json"), &configured);
    let mut stage_rows = Vec::new();
    let mut stage_pins = Vec::new();
    for ((label, targets, minute), workers) in [
        ("core-session-build", "onnxruntime_session", 3),
        ("build", "all re2", 4),
    ]
    .into_iter()
    .zip(stage_workers)
    {
        let mut arg_vec = vec![
            cmake["path"].as_str().unwrap().to_owned(),
            "--build".into(),
            build.to_str().unwrap().into(),
            "--config".into(),
            "Release".into(),
            "--target".into(),
        ];
        arg_vec.extend(targets.split(' ').map(str::to_owned));
        arg_vec.extend(["--parallel".into(), workers.into()]);
        let args = command_line(&arg_vec[1..]);
        let value = stage(&mut fs, &root, label, &arg_vec, &args, minute, 899);
        stage_pins.push(fs.json(
            &root.join("logs").join(format!("{label}.receipt.json")),
            &value,
        ));
        stage_rows.push(value);
    }
    let provider_rows = json!([
        fs.put(&build.join("onnxruntime_providers_shared.dll"), b"shared"),
        fs.put(&build.join("onnxruntime_providers_cuda.dll"), b"CUDA"),
    ]);
    let compiled = json!({
        "schema":"airk.private.cuda-fresh-compiled/1","status":"COMPILED_UNQUALIFIED",
        "qualification":false,"driver":driver,"configured":configured_pin,"recipe":recipe_pin,"build_root":build,
        "toolchain":tools,"configured_graph":graph,"collector_source":collector,"ninja":[cache,ninja,rules],
        "stages":stage_rows,"stage_receipts":stage_pins,"providers":provider_rows,
        "libraries":[fs.put(&build.join("onnxruntime_session.lib"), b"session"),
            fs.put(&build.join("_deps/re2/re2.lib"), b"re2")],
        "original_deadline_monotonic":900,"deadline_monotonic":899,
        "started_utc":time(2,0),"finished_utc":time(5,0),"elapsed_seconds":180,
    });
    let build_pin = fs.json(&root.join("fresh-compile.json"), &compiled);
    let independent = json!({
        "schema":"airk.private.cuda-fresh-static-build-proof/1","target":"x86_64-pc-windows-msvc","ort_version":"1.30.0",
        "build_receipt":build_pin,"producer_source":driver,"configure_receipt":configured_pin,"configure_recipe":recipe_pin,
        "toolchain":tools,"compiler_tools":[cl],"native_input_closure_sha256":canonical_sha(&json!([nvcc]))?,
        "provider_dlls":provider_rows,"collector_source":collector,"configured_graph":graph,"ninja":[cache,ninja,rules],
        "source_lock":lock,"cpu_lock":cpu,"source_archive":source_archive,"dependency_archives":{},
        "source_inventory_sha256":canonical_sha(&source_inventory)?,"frontend_inventory_sha256":canonical_sha(&json!([]))?,
        "mirror_inventory_sha256":canonical_sha(&json!([]))?,"patch_inputs":patches,
        "qualification":false,"activation_pins_available":false,
    });
    let copy = fs.json(&anchor.join("staged").join("fresh-compile.json"), &compiled);
    let raw = pin(&copy, false)?.resource();
    let approved_recipe = sha(&recipe_pin["sha256"])?;
    let approved_driver = sha(&driver["sha256"])?;
    let result = validate(
        &fs,
        &raw,
        &independent,
        &anchor,
        approved_recipe,
        approved_driver,
    );
    if let Some(expected) = expected_refusal {
        let error = result.err().expect("mixed worker command must fail");
        assert!(error.to_string().contains(expected), "{error}");
        return Ok(());
    }
    let result = result?;
    assert_eq!(result.static_core_build, raw.sha256);
    assert_eq!(
        result.provider_dlls[1].name,
        "onnxruntime_providers_cuda.dll"
    );
    assert!(fs.rechecked.get());

    for (field, changed, expected) in [
        (
            "parallel_workers",
            json!("8"),
            "parallel_workers must be integer 2 or 8",
        ),
        (
            "msvc_ciso646_compat",
            json!(false),
            "required MSVC CUDA compatibility scope",
        ),
        (
            "output_audit_owner",
            json!({"path":repo.join("alternate-audit.py"),"bytes":driver["bytes"],"sha256":driver["sha256"]}),
            "output audit owner differs",
        ),
    ] {
        let mut alternate_recipe = recipe.clone();
        alternate_recipe[field] = changed;
        let alternate_recipe = fs.json(&anchor.join("recipe.json"), &alternate_recipe);
        let mut alternate_configure = configured.clone();
        alternate_configure["recipe"] = alternate_recipe.clone();
        let alternate_configure = fs.json(&root.join("fresh-configure.json"), &alternate_configure);
        let mut alternate_compile = compiled.clone();
        alternate_compile["configured"] = alternate_configure.clone();
        alternate_compile["recipe"] = alternate_recipe.clone();
        let alternate_compile = fs.json(&root.join("fresh-compile.json"), &alternate_compile);
        let mut alternate_proof = independent.clone();
        alternate_proof["build_receipt"] = alternate_compile.clone();
        alternate_proof["configure_receipt"] = alternate_configure;
        alternate_proof["configure_recipe"] = alternate_recipe.clone();
        let error = validate(
            &fs,
            &pin(&alternate_compile, false)?.resource(),
            &alternate_proof,
            &anchor,
            sha(&alternate_recipe["sha256"])?,
            approved_driver,
        )
        .err()
        .expect("resealed missing compatibility scope or changed audit owner must fail");
        assert!(error.to_string().contains(expected), "{error}");
    }
    fs.json(&anchor.join("recipe.json"), &recipe);
    fs.json(&root.join("fresh-configure.json"), &configured);
    fs.json(&root.join("fresh-compile.json"), &compiled);

    let mut old_roster = configured.clone();
    old_roster["patch_inputs"]
        .as_array_mut()
        .unwrap()
        .truncate(7);
    assert!(
        verify_patches(&fs, &root, &repo, &old_roster)
            .unwrap_err()
            .to_string()
            .contains("patch roster")
    );
    let mut wrong_patch = configured.clone();
    wrong_patch["patch_inputs"][7]["original"]["path"] = json!(repo.join("alternate.patch"));
    assert!(
        verify_patches(&fs, &root, &repo, &wrong_patch)
            .unwrap_err()
            .to_string()
            .contains("evidence path")
    );
    let mut wrong_patch = configured.clone();
    for name in ["original", "snapshot"] {
        wrong_patch["patch_inputs"][7][name]["sha256"] = json!(Sha256Digest::from_bytes([7; 32]));
    }
    assert!(
        verify_patches(&fs, &root, &repo, &wrong_patch)
            .unwrap_err()
            .to_string()
            .contains("compatibility patch identity")
    );
    let mut wrong_source = configured.clone();
    wrong_source["source_inventory"][0]["sha256"] = json!(Sha256Digest::from_bytes([8; 32]));
    assert!(
        verify_patches(&fs, &root, &repo, &wrong_source)
            .unwrap_err()
            .to_string()
            .contains("patched source inventory identity")
    );

    let retained_key = retained_configure_path.to_str().unwrap();
    fs.files.remove(retained_key);
    let fail = |fs: &Memory, raw: &ResourceFilePin, proof: &Value| {
        validate(fs, raw, proof, &anchor, approved_recipe, approved_driver)
            .err()
            .expect("changed evidence must fail")
            .to_string()
    };
    assert!(fail(&fs, &raw, &independent).contains("configure receipt missing"));
    let mut changed_configure_stage = configure.clone();
    changed_configure_stage["owned"]["exit_code"] = json!(9);
    fs.json(&retained_configure_path, &changed_configure_stage);
    assert!(fail(&fs, &raw, &independent).contains("retained configure stage receipt differs"));
    fs.json(&retained_configure_path, &configure);

    // Recompute all enclosing pins; the Ninja semantics must still fail.
    let false_ninja = ninja_text.replace("build re2:", "build foreign:");
    let false_ninja = fs.put(&ninja_path, false_ninja.as_bytes());
    let mut false_configure = configured.clone();
    false_configure["configured_graph"]["ninja_sha256"] = false_ninja["sha256"].clone();
    let false_configure_pin = fs.json(&root.join("fresh-configure.json"), &false_configure);
    let mut false_compile = compiled.clone();
    false_compile["ninja"][1] = false_ninja;
    false_compile["configured"] = false_configure_pin.clone();
    false_compile["configured_graph"] = false_configure["configured_graph"].clone();
    let false_compile_pin = fs.json(&root.join("fresh-compile.json"), &false_compile);
    let mut false_proof = independent.clone();
    false_proof["build_receipt"] = false_compile_pin.clone();
    false_proof["configure_receipt"] = false_configure_pin;
    false_proof["configured_graph"] = false_compile["configured_graph"].clone();
    false_proof["ninja"] = false_compile["ninja"].clone();
    let false_raw = pin(&false_compile_pin, false)?.resource();
    assert!(fail(&fs, &false_raw, &false_proof).contains("target is missing: re2"));

    fs.json(&root.join("fresh-configure.json"), &configured);
    fs.json(&root.join("fresh-compile.json"), &compiled);
    fs.put(&ninja_path, ninja_text.as_bytes());

    let cmake = pin(&cmake, false)?;
    for (index, limit) in [(0, 16u64 << 20), (1, 64 << 20), (2, 16 << 20)] {
        let mut oversized = compiled.clone();
        oversized["ninja"][index]["bytes"] = json!(limit + 1);
        let error = configured_graph(
            &fs,
            &root,
            &build,
            &oversized,
            &configure_args,
            &cmake,
            &tools,
        )
        .expect_err("oversized configured input must be refused before read");
        assert!(error.to_string().contains("capacity"), "{error}");
    }
    let mut changed = independent.clone();
    changed["activation_pins_available"] = json!(true);
    assert!(
        validate(
            &fs,
            &raw,
            &changed,
            &anchor,
            approved_recipe,
            approved_driver
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn cuda_fresh_windows_dispatch_refuses_unapproved_fresh_authority() -> Result<()> {
    let root = std::env::current_dir()?.join("fixture");
    let mut fs = Memory::default();
    let actual = fs.put(&root.join("producer.json"), b"staged producer");
    let raw = pin(&actual, false)?.resource();
    let historical = fs.json(
        &root.join("checkpointed.json"),
        &json!({"schema":"airk.cuda-static-build-proof/2"}),
    );
    assert!(
        try_validate_fresh_producer(&fs, &raw, &pin(&historical, false)?.resource())?.is_none()
    );
    if REVIEWED_COMPILER_OWNER.is_none() {
        let current = fs.json(
            &root.join("fresh.json"),
            &json!({"schema":"airk.private.cuda-fresh-static-build-proof/1"}),
        );
        assert!(
            try_validate_fresh_producer(&fs, &raw, &pin(&current, false)?.resource())
                .err()
                .unwrap()
                .to_string()
                .contains("no reviewed authority")
        );
    }
    Ok(())
}
