// SPDX-License-Identifier: Apache-2.0
//! Verify approved fresh Windows producer evidence without dispatching tools.
use super::*;
use std::path::{Path, PathBuf};

const FRESH_BUILD_SCHEMA: &str = "airk.private.cuda-fresh-static-build-proof/1";
const CONFIGURE_RECIPE: &str = "airk.private.cuda-fresh-configure-recipe/1";
const CONFIGURED: &str = "airk.private.cuda-fresh-configured/1";
const COMPILED: &str = "airk.private.cuda-fresh-compiled/1";
const REVIEWED_RECIPE: &str = "286e6226f4f7f85ab5f79f717138e49604a555ca6f9fd8a27bbf50fd6532695e";
// Set only when the producer owner has reviewed and frozen the actual compile driver.
const REVIEWED_COMPILER_OWNER: Option<&str> = None;
const CAMPAIGN_END: &str = "2026-11-19T05:29:42+00:00";
const ANCHOR: &str = "C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z";
const COLLECTOR: &str = "onnxruntime/core/session/onnxruntime_c_api.cc";
const COLLECTOR_SHA: &str = "db00479a06d0a1e8fadf4cc834ffe5a6da0b745ee2820b06a0821fb1dbeca97b";
const TARGET: &str = "x86_64-pc-windows-msvc";
const MAX_METADATA: u64 = 32 << 20;
const MAX_FILE: u64 = 2 << 30;
const MAX_LOG: u64 = 64 << 20;
const STAGE_FILES: [(&str, &str); 6] = [
    ("request", ".request.json"),
    ("stdout", ".stdout.log"),
    ("stderr", ".stderr.log"),
    ("result", ".tree.json"),
    ("started", ".started.json"),
    ("outer", ".outer.log"),
];
const GRAPH_FILES: [(&str, &str, u64); 3] = [
    ("CMakeCache.txt", "cache_sha256", 16 << 20),
    ("build.ninja", "ninja_sha256", 64 << 20),
    ("CMakeFiles/rules.ninja", "rules_sha256", 16 << 20),
];
const PROVIDERS: [&str; 2] = [
    "onnxruntime_providers_shared.dll",
    "onnxruntime_providers_cuda.dll",
];
const MSVC_CISO646_STAGE: &str = "msvc-cuda-ciso646";
const MSVC_CISO646_PATCH: &str = "scripts/cuda-static-runtime/build/msvc-cuda-ciso646.patch";
const MSVC_CISO646_PATCH_SHA: &str =
    "bd511d6bdf8ce72d0983ec995f17ffb40194a1846c130ce9ee4c67c354566c60";
const MSVC_CISO646_SOURCE: &str = "cmake/onnxruntime_providers_cuda.cmake";
const MSVC_CISO646_SOURCE_SHA: &str =
    "82733200570c4dfe848cbcda34d05407417cbbb236ba0bb9dffe9fba35149271";
const PATCH_STAGES: [&str; 8] = [
    "loader",
    "offline",
    "final-placement",
    "initialized-public-api",
    "final-graph",
    "frontend-arm64",
    "frontend-authenticated-imports",
    MSVC_CISO646_STAGE,
];

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pin {
    path: String,
    bytes: u64,
    sha256: Sha256Digest,
}

impl Pin {
    fn resource(&self) -> ResourceFilePin {
        ResourceFilePin {
            path: self.path.clone(),
            sha256: self.sha256,
        }
    }

    fn authenticate(&self, source: &dyn Source, maximum: u64) -> Result<()> {
        ensure!(
            self.bytes <= maximum,
            "fresh evidence pin exceeds its capacity"
        );
        source.authenticate(&self.resource(), self.bytes, maximum)
    }

    fn read_bytes(&self, source: &dyn Source, maximum: u64) -> Result<Vec<u8>> {
        ensure!(
            self.bytes <= maximum,
            "fresh evidence bytes exceed their capacity"
        );
        let raw = read(source, &self.resource(), maximum)?;
        ensure!(
            raw.len() as u64 == self.bytes,
            "fresh evidence length differs"
        );
        Ok(raw)
    }

    fn json(&self, source: &dyn Source) -> Result<Value> {
        strict::parse(&self.read_bytes(source, MAX_METADATA)?)
    }
}

fn pin(value: &Value, allow_empty_log: bool) -> Result<Pin> {
    let pin: Pin = serde_json::from_value(value.clone())?;
    pin.resource().validate()?;
    ensure!(
        pin.bytes <= MAX_FILE && (allow_empty_log || pin.bytes > 0) && nonzero(pin.sha256),
        "invalid fresh evidence FilePin"
    );
    Ok(pin)
}

fn string(value: &Value) -> Result<&str> {
    value.as_str().context("fresh evidence string missing")
}

fn exact_fields(value: &Value, fields: &[&str]) -> Result<()> {
    let object = value.as_object().context("fresh evidence object missing")?;
    ensure!(
        object.len() == fields.len() && fields.iter().all(|field| object.contains_key(*field)),
        "fresh evidence field roster differs"
    );
    Ok(())
}

fn path_eq(actual: &str, expected: &Path) -> Result<bool> {
    let expected = expected
        .to_str()
        .context("fresh evidence path must be UTF-8")?;
    Ok(actual
        .replace('/', "\\")
        .eq_ignore_ascii_case(&expected.replace('/', "\\")))
}

fn at(pin: &Pin, path: &Path) -> Result<()> {
    ensure!(path_eq(&pin.path, path)?, "fresh evidence path differs");
    Ok(())
}

fn utc(value: &Value) -> Result<i128> {
    checkpoint::utc(string(value)?)
}

fn seconds(value: &Value) -> Result<f64> {
    let n = value.as_f64().context("fresh original clock missing")?;
    ensure!(n.is_finite() && n > 0.0, "invalid fresh original clock");
    Ok(n)
}

struct Windows {
    configure_start: i128,
    configure_finish: i128,
    compile_start: i128,
    compile_finish: i128,
    original: f64,
    compile_cutoff: f64,
}

fn windows(configured: &Value, compiled: &Value, recipe: &Value) -> Result<Windows> {
    let wall = recipe["wall_seconds"]
        .as_u64()
        .filter(|s| (1..=10800).contains(s))
        .context("fresh original wall bound differs")?;
    let configure_start = utc(&configured["started_utc"])?;
    let configure_finish = utc(&configured["finished_utc"])?;
    let compile_start = utc(&compiled["started_utc"])?;
    let compile_finish = utc(&compiled["finished_utc"])?;
    let cutoff =
        (configure_start + (wall as i128) * 1_000_000_000).min(checkpoint::utc(CAMPAIGN_END)?);
    let original = seconds(&configured["configure"]["original_deadline_monotonic"])?;
    let compile_cutoff = seconds(&compiled["deadline_monotonic"])?;
    let configure_elapsed = seconds(&configured["elapsed_seconds"])?;
    let compile_elapsed = seconds(&compiled["elapsed_seconds"])?;
    ensure!(
        configure_start <= configure_finish
            && configure_finish <= compile_start
            && compile_start <= compile_finish
            && compile_finish < cutoff
            && configure_elapsed < (((cutoff - configure_start) as f64) / 1_000_000_000.0)
            && compile_elapsed < (((cutoff - compile_start) as f64) / 1_000_000_000.0)
            && configure_elapsed < original
            && compile_elapsed < compile_cutoff
            && seconds(&compiled["original_deadline_monotonic"])? == original
            && compile_cutoff <= original,
        "fresh producer exceeded the original configure window"
    );
    Ok(Windows {
        configure_start,
        configure_finish,
        compile_start,
        compile_finish,
        original,
        compile_cutoff,
    })
}

fn command_args(value: &Value) -> Result<Vec<String>> {
    let argv: Vec<String> = serde_json::from_value(value.clone())?;
    ensure!(
        !argv.is_empty()
            && argv.len() <= 128
            && argv
                .iter()
                .all(|arg| arg.len() <= 4096 && !arg.chars().any(char::is_control)),
        "invalid fresh build argument vector"
    );
    Ok(argv)
}

// The producer uses CPython's Windows CRT command-line encoding in the raw request.
fn command_line(args: &[String]) -> String {
    let mut line = String::new();
    for (index, arg) in args.iter().enumerate() {
        if index > 0 {
            line.push(' ');
        }
        let quote = arg.is_empty() || arg.contains([' ', '\t']);
        if quote {
            line.push('"');
        }
        let mut backslashes = 0;
        for ch in arg.chars() {
            if ch == '\\' {
                backslashes += 1;
                continue;
            }
            let count = if ch == '"' {
                2 * backslashes + 1
            } else {
                backslashes
            };
            line.extend(std::iter::repeat_n('\\', count));
            backslashes = 0;
            line.push(ch);
        }
        line.extend(std::iter::repeat_n(
            '\\',
            if quote { 2 * backslashes } else { backslashes },
        ));
        if quote {
            line.push('"');
        }
    }
    line
}

fn expected_build_command(cmake: &Pin, build: &Path, targets: &[&str]) -> Result<Vec<String>> {
    let mut argv = vec![
        cmake.path.clone(),
        "--build".into(),
        build
            .to_str()
            .context("fresh build path must be UTF-8")?
            .into(),
        "--config".into(),
        "Release".into(),
        "--target".into(),
    ];
    argv.extend(targets.iter().map(|target| (*target).to_owned()));
    argv.extend(["--parallel".into(), "2".into()]);
    Ok(argv)
}

fn verify_stage(
    source: &dyn Source,
    root: &Path,
    stage: &Value,
    name: &str,
    argv: &[String],
    cutoff: f64,
) -> Result<(i128, i128)> {
    exact_fields(
        stage,
        &[
            "stage",
            "argv",
            "cwd",
            "original_deadline_monotonic",
            "owned",
            "actual_tree_result",
            "files",
        ],
    )?;
    let owned = &stage["owned"];
    ensure!(
        stage["stage"] == name
            && command_args(&stage["argv"])? == argv
            && path_eq(string(&stage["cwd"])?, root)?
            && seconds(&stage["original_deadline_monotonic"])? == cutoff
            && successful(owned)
            && owned["exit_code"].as_u64() == Some(0)
            && owned["parent_assigned_to_job"] == false
            && owned["child_assigned_before_resume"] == true
            && owned["owned_job_final_active_processes"].as_u64() == Some(0)
            && owned["owned_job_final_member_pids"]
                .as_array()
                .is_some_and(Vec::is_empty),
        "fresh stage command or natural owned settlement differs"
    );
    let prefix = root.join("logs").join(name);
    let files = rows(&stage["files"])?;
    ensure!(
        files.len() == STAGE_FILES.len(),
        "fresh stage file roster differs"
    );
    let mut metadata = BTreeMap::new();
    let mut path_strings = BTreeMap::new();
    for (kind, suffix) in STAGE_FILES {
        let expected = prefix.with_extension(&suffix[1..]);
        let candidates = files
            .iter()
            .filter(|row| {
                row["path"]
                    .as_str()
                    .and_then(|path| path_eq(path, &expected).ok())
                    == Some(true)
            })
            .collect::<Vec<_>>();
        ensure!(
            candidates.len() == 1,
            "fresh stage evidence leaf differs: {kind}"
        );
        let is_log = matches!(kind, "stdout" | "stderr" | "outer");
        let file = pin(candidates[0], is_log)?;
        path_strings.insert(kind, file.path.clone());
        if is_log {
            file.authenticate(source, if kind == "outer" { MAX_LOG } else { MAX_FILE })?;
        } else {
            metadata.insert(kind, file.json(source)?);
        }
    }
    let request = &metadata["request"];
    ensure!(
        *request
            == serde_json::json!({
                "schema": "airk.private.process-tree-command/1", "executable": argv[0],
                "arguments": command_line(&argv[1..]), "cwd": stage["cwd"],
                "stdout": path_strings["stdout"], "stderr": path_strings["stderr"],
                "result": path_strings["result"], "started": path_strings["started"],
                "started_event": null, "done_event": null,
            }),
        "fresh stage retained command differs"
    );
    let tree = &metadata["result"];
    let started = &metadata["started"];
    let pid = |value: &Value| {
        value
            .as_u64()
            .is_some_and(|pid| (1..=u32::MAX as u64).contains(&pid))
    };
    let began = utc(&tree["start_utc"])?;
    let finished = utc(&tree["finished_utc"])?;
    ensure!(
        *tree == stage["actual_tree_result"]
            && tree["schema"] == "airk.private.process-tree-result/1"
            && tree["exit_code"].as_u64() == Some(0)
            && tree["wait"] == "Start-Process-Wait-process-and-descendants"
            && pid(&tree["command_pid"])
            && pid(&tree["wrapper_pid"])
            && tree["command_pid"] == started["pid"]
            && tree["wrapper_pid"] == started["wrapper_pid"]
            && began == utc(&started["started_utc"])?
            && began <= finished,
        "fresh stage actual process tree differs"
    );
    Ok((began, finished))
}

fn build_proof(
    build_pin: &Value,
    compiled: &Value,
    configured: &Value,
    recipe: &Value,
) -> Result<Value> {
    Ok(serde_json::json!({
        "schema": FRESH_BUILD_SCHEMA,
        "target": TARGET,
        "ort_version": "1.30.0",
        "build_receipt": build_pin,
        "producer_source": compiled["driver"],
        "configure_receipt": compiled["configured"],
        "configure_recipe": compiled["recipe"],
        "toolchain": compiled["toolchain"],
        "compiler_tools": recipe["compiler_tools"],
        "native_input_closure_sha256": canonical_sha(&recipe["native_inputs"])?,
        "provider_dlls": compiled["providers"],
        "collector_source": compiled["collector_source"],
        "configured_graph": compiled["configured_graph"],
        "ninja": compiled["ninja"],
        "source_lock": recipe["owners"]["lock"],
        "cpu_lock": recipe["owners"]["cpu_lock"],
        "source_archive": recipe["source_archive"],
        "dependency_archives": recipe["dependency_archives"],
        "source_inventory_sha256": canonical_sha(&configured["source_inventory"])?,
        "frontend_inventory_sha256": canonical_sha(&configured["frontend_inventory"])?,
        "mirror_inventory_sha256": canonical_sha(&configured["mirror_inventory"])?,
        "patch_inputs": configured["patch_inputs"],
        "qualification": false,
        "activation_pins_available": false,
    }))
}

fn configured_graph(
    source: &dyn Source,
    root: &Path,
    build: &Path,
    compiled: &Value,
    argv: &[String],
    cmake: &Pin,
    tools: &Value,
) -> Result<()> {
    ensure!(
        argv.len() > 8
            && argv[0] == cmake.path
            && argv[1] == "-S"
            && path_eq(&argv[2], &root.join("source/cmake"))?
            && argv[3] == "-B"
            && path_eq(&argv[4], build)?
            && argv[5] == "-G"
            && argv[6] == "Ninja",
        "fresh configure command is outside its approved source/build"
    );
    let graph = &compiled["configured_graph"];
    let graph_files = rows(&compiled["ninja"])?;
    ensure!(
        graph_files.len() == GRAPH_FILES.len(),
        "fresh configured graph pin roster differs"
    );
    let data = graph_files
        .iter()
        .zip(GRAPH_FILES)
        .map(|(row, (relative, key, maximum))| {
            let file = pin(row, false)?;
            at(&file, &build.join(relative))?;
            ensure!(
                file.sha256 == sha(&graph[key])?,
                "fresh generated graph hash differs"
            );
            file.read_bytes(source, maximum)
        })
        .collect::<Result<Vec<_>>>()?;
    let [cache, ninja, rules] = data.as_slice() else {
        anyhow::bail!("fresh configured graph pin roster differs");
    };
    let cache = std::str::from_utf8(cache)?;
    verify_ninja_graph(
        root,
        build,
        graph,
        cache,
        std::str::from_utf8(ninja)?,
        std::str::from_utf8(rules)?,
    )?;
    let mut values = BTreeMap::new();
    for line in cache.lines().filter(|line| !line.starts_with(['/', '#'])) {
        if let Some((typed, value)) = line.split_once('=')
            && let Some((key, _)) = typed.split_once(':')
        {
            ensure!(
                values.insert(key, value).is_none(),
                "duplicate fresh configure cache key"
            );
        }
    }
    let mut flags = BTreeMap::new();
    for arg in &argv[7..] {
        let (key, expected) = arg
            .strip_prefix("-D")
            .and_then(|flag| flag.split_once('='))
            .context("fresh configure definition is malformed")?;
        let actual = values
            .get(key)
            .context("fresh configure definition absent from held cache")?;
        let path_flag = ["_EXECUTABLE", "_PROGRAM", "_COMPILER", "_SYSROOT"]
            .iter()
            .any(|suffix| key.ends_with(*suffix))
            || matches!(
                key,
                "CUDAToolkit_ROOT"
                    | "CUDNN_PATH"
                    | "onnxruntime_CUDNN_HOME"
                    | "onnxruntime_CUDA_HOME"
                    | "FETCHCONTENT_SOURCE_DIR_CUDNN_FRONTEND"
            );
        ensure!(
            if path_flag {
                path_eq(actual, Path::new(expected))?
            } else {
                *actual == expected
            },
            "fresh configured cache differs for {key}"
        );
        ensure!(
            flags.insert(key, expected).is_none(),
            "duplicate fresh configure definition"
        );
    }
    for (key, value) in [
        ("CMAKE_BUILD_TYPE", "Release"),
        ("CMAKE_CXX_STANDARD", "20"),
        ("CMAKE_CUDA_ARCHITECTURES", "86"),
        ("CMAKE_OBJECT_PATH_MAX", "240"),
        ("onnxruntime_USE_CUDA", "ON"),
        ("onnxruntime_BUILD_CUDA_EP_AS_PLUGIN", "OFF"),
        ("onnxruntime_USE_FLASH_ATTENTION", "ON"),
        ("onnxruntime_USE_MEMORY_EFFICIENT_ATTENTION", "ON"),
        ("FETCHCONTENT_UPDATES_DISCONNECTED", "ON"),
    ] {
        ensure!(
            flags.get(key) == Some(&value),
            "fresh required configure option differs: {key}"
        );
    }
    for (key, tool) in [
        ("CMAKE_C_COMPILER", "compiler"),
        ("CMAKE_CXX_COMPILER", "compiler"),
        ("CMAKE_CUDA_HOST_COMPILER", "compiler"),
        ("CMAKE_MAKE_PROGRAM", "ninja"),
        ("Python3_EXECUTABLE", "python"),
        ("Patch_EXECUTABLE", "patch"),
    ] {
        let expected = string(&tools[tool]["path"])?;
        ensure!(
            path_eq(
                flags
                    .get(key)
                    .context("fresh configure tool flag missing")?,
                Path::new(expected)
            )?,
            "fresh configure actual tool binding differs"
        );
    }
    ensure!(
        graph["checked_flags"].as_u64() == Some(flags.len() as u64)
            && graph["final_capi_source_sha256"] == COLLECTOR_SHA,
        "fresh selected compiler or generated graph differs"
    );
    Ok(())
}

fn verify_ninja_graph(
    root: &Path,
    build: &Path,
    graph: &Value,
    cache: &str,
    ninja: &str,
    rules: &str,
) -> Result<()> {
    ensure!(
        [cache, ninja, rules].iter().all(|text| !text
            .to_ascii_lowercase()
            .contains("allow-unsupported-compiler")),
        "fresh unsupported CUDA compiler override"
    );
    ensure!(
        ninja.contains("-std:c++20") || ninja.contains("/std:c++20"),
        "fresh generated Ninja graph lacks the selected C++20 build mode"
    );
    let names = [
        "onnxruntime_session.lib",
        "all",
        "re2",
        "onnxruntime_providers_cuda.dll",
        "onnxruntime_providers_shared.dll",
    ];
    let mut definitions = [None; 5];
    let mut capi = None;
    let mut count = 0;
    let mut maximum = 0;
    for edge in ninja.lines().filter_map(|line| line.strip_prefix("build ")) {
        for (index, name) in names.iter().enumerate() {
            if edge
                .strip_prefix(*name)
                .is_some_and(|tail| tail.starts_with([':', ' ']))
            {
                ensure!(
                    definitions[index].replace(edge).is_none(),
                    "fresh configured Ninja target is ambiguous: {name}"
                );
            }
        }
        let Some((output, rule)) = edge.split_once(": ") else {
            continue;
        };
        if output.ends_with(".obj")
            && (rule.starts_with("CXX_COMPILER") || rule.starts_with("CUDA_COMPILER"))
        {
            let normalized = output.replace('\\', "/");
            let mut path = build.to_path_buf();
            for part in Path::new(&normalized).components() {
                match part {
                    std::path::Component::Normal(name) if !name.to_string_lossy().contains(':') => {
                        path.push(name);
                    }
                    std::path::Component::CurDir => {}
                    std::path::Component::ParentDir if path.as_path() != build => {
                        path.pop();
                    }
                    _ => anyhow::bail!("fresh configured Ninja object escapes build root"),
                }
            }
            ensure!(
                path.as_path() != build,
                "fresh configured Ninja object path is empty"
            );
            count += 1;
            maximum = maximum.max(
                path.to_str()
                    .context("fresh Ninja object path must be UTF-8")?
                    .chars()
                    .count(),
            );
        }
        if output.ends_with("onnxruntime_c_api.cc.obj")
            && rule.starts_with("CXX_COMPILER__onnxruntime_session_")
        {
            let source = rule
                .split_whitespace()
                .nth(1)
                .context("fresh configured CAPI source missing")?;
            ensure!(
                capi.replace((output, source)).is_none(),
                "fresh configured CAPI object is ambiguous"
            );
        }
    }
    for (name, definition) in names.into_iter().zip(definitions) {
        ensure!(
            definition.is_some(),
            "fresh configured Ninja target is missing: {name}"
        );
    }
    let session = definitions[0].context("fresh configured session target missing")?;
    let (_, inputs) = session
        .split_once(": ")
        .context("fresh configured session edge missing")?;
    ensure!(
        inputs.starts_with("CXX_STATIC_LIBRARY_LINKER"),
        "fresh configured session target is not a static archive"
    );
    let (object, source) = capi.context("fresh configured CAPI object missing")?;
    let expected_source = root.join("source").join(COLLECTOR);
    ensure!(
        source.replace("$:", ":").replace('\\', "/")
            == expected_source
                .to_str()
                .context("fresh CAPI source path must be UTF-8")?
                .replace('\\', "/")
            && inputs
                .split_whitespace()
                .skip(1)
                .take_while(|input| !matches!(*input, "|" | "||" | "|@"))
                .any(|input| input == object),
        "fresh configured CAPI source/object is not in the actual session archive"
    );
    ensure!(
        count > 0
            && maximum <= 240
            && graph["objects"].as_u64() == Some(count as u64)
            && graph["max_object_path"].as_u64() == Some(maximum as u64),
        "fresh configured Ninja object count or maximum path differs"
    );
    Ok(())
}

fn verify_configure_owners(source: &dyn Source, repo: &Path, recipe: &Value) -> Result<()> {
    let paths = [
        (
            "driver",
            "scripts/cuda-static-runtime/build/fresh_configure.py",
        ),
        ("guards", "scripts/cuda-component-execution.py"),
        ("pin_reader", "scripts/cuda-static-incremental.py"),
        (
            "source_stage",
            "scripts/cuda-static-runtime/build/fresh_source.py",
        ),
        (
            "process_stage",
            "scripts/cuda-static-runtime/build/fresh_process.py",
        ),
        ("cpu", "scripts/provision-static-onnx-runtime.py"),
        ("cpu_lock", "scripts/static-onnx-runtime-lock.json"),
        (
            "r8",
            "scripts/cuda-static-runtime/build/r8/provision_cuda_runtime.py",
        ),
        (
            "lock",
            "scripts/cuda-static-runtime/build/r8/cuda-runtime-producer-lock.json",
        ),
        (
            "tree_waiter",
            "scripts/cuda-static-runtime/build/r12/wait_process_tree.ps1",
        ),
    ];
    let owners = recipe["owners"]
        .as_object()
        .context("fresh configure owners missing")?;
    ensure!(
        owners.len() == paths.len(),
        "fresh configure owner roster differs"
    );
    for (name, relative) in paths {
        let file = pin(&recipe["owners"][name], false)?;
        at(&file, &repo.join(relative))?;
        // The two source locks are held and parsed with their metadata below.
        if !matches!(name, "lock" | "cpu_lock") {
            file.authenticate(source, 1 << 20)?;
        }
    }
    let guard = pin(&recipe["checked_guard"], false)?;
    at(&guard, &repo.join("scripts/dev/assert-human-execution.ps1"))?;
    guard.authenticate(source, 1 << 20)?;
    pin(&recipe["powershell"], false)?.authenticate(source, 1 << 30)
}

fn verify_tools(source: &dyn Source, recipe: &Value, argv: &[String]) -> Result<Pin> {
    let tools = &recipe["tools"];
    exact_fields(
        tools,
        &[
            "compiler",
            "cmake",
            "ninja",
            "patch",
            "python",
            "windows_sdk",
            "msvc_toolset",
            "vcvars",
        ],
    )?;
    for name in ["compiler", "cmake", "ninja", "patch", "python", "vcvars"] {
        pin(&tools[name], false)?.authenticate(source, 1 << 30)?;
    }
    let compiler = pin(&tools["compiler"], false)?;
    let selected = rows(&recipe["compiler_tools"])?
        .iter()
        .filter(|row| row["path"] == compiler.path)
        .collect::<Vec<_>>();
    let env = &recipe["environment"];
    ensure!(
        selected == [&tools["compiler"]]
            && tools["msvc_toolset"] == env["VCTOOLSVERSION"]
            && string(&env["WINDOWSSDKVERSION"])?.trim_end_matches(['\\', '/'])
                == string(&tools["windows_sdk"])?
            && ["CL", "_CL_"]
                .iter()
                .all(|name| env.get(*name).is_none_or(|value| value == ""))
            && string(&env["PATHEXT"])?
                .split(';')
                .any(|extension| extension.eq_ignore_ascii_case(".EXE")),
        "fresh configured compiler environment or census differs"
    );
    let nvcc = argv
        .iter()
        .find_map(|arg| arg.strip_prefix("-DCMAKE_CUDA_COMPILER="))
        .context("fresh CUDA compiler flag missing")?;
    let expected = Path::new(nvcc);
    ensure!(
        expected
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("nvcc.exe")),
        "fresh CUDA compiler name differs"
    );
    let native = rows(&recipe["native_inputs"])?;
    ensure!(
        !native.is_empty() && native.len() <= 8192,
        "fresh native input census exceeds its capacity"
    );
    let matches = native
        .iter()
        .filter(|row| {
            row["path"]
                .as_str()
                .and_then(|path| path_eq(path, expected).ok())
                == Some(true)
        })
        .collect::<Vec<_>>();
    ensure!(
        matches.len() == 1,
        "fresh CUDA compiler not uniquely selected from approved native inputs"
    );
    pin(matches[0], false)?.authenticate(source, 1 << 30)?;
    pin(&tools["cmake"], false)
}

fn verify_providers(
    source: &dyn Source,
    build: &Path,
    compiled: &Value,
) -> Result<[PublicFile; 2]> {
    let providers = rows(&compiled["providers"])?;
    ensure!(
        providers.len() == PROVIDERS.len(),
        "fresh same-build provider pair differs"
    );
    let public = providers
        .iter()
        .zip(PROVIDERS)
        .map(|(value, name)| {
            let file = pin(value, false)?;
            at(&file, &build.join(name))?;
            file.authenticate(source, MAX_FILE)?;
            Ok(PublicFile {
                name: name.into(),
                bytes: file.bytes,
                sha256: file.sha256,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    public
        .try_into()
        .map_err(|_| anyhow::anyhow!("fresh provider count changed"))
}

fn verify_patches(source: &dyn Source, root: &Path, repo: &Path, configured: &Value) -> Result<()> {
    let patches = rows(&configured["patch_inputs"])?;
    ensure!(
        patches.len() == PATCH_STAGES.len(),
        "fresh patch roster differs"
    );
    for (row, stage) in patches.iter().zip(PATCH_STAGES) {
        exact_fields(row, &["stage", "original", "snapshot"])?;
        ensure!(row["stage"] == stage, "fresh patch order differs");
        let original = pin(&row["original"], false)?;
        let snapshot = pin(&row["snapshot"], false)?;
        at(
            &snapshot,
            &root.join("patch-inputs").join(format!("{stage}.patch")),
        )?;
        if stage == "final-graph" {
            ensure!(
                original.sha256
                    == "a1a95c9b4401e302d7046159f882fdafa735bf7b32542314f75b333b902870c9"
                        .parse()?
                    && snapshot.sha256
                        == "5222d2634878ac5e1c68264b6d521f0ca95df3a47397e3cf167b3ff7de844d58"
                            .parse()?,
                "fresh final-graph patch binding differs"
            );
        } else {
            ensure!(
                original.sha256 == snapshot.sha256 && original.bytes == snapshot.bytes,
                "fresh unchanged patch transport differs"
            );
        }
        if stage == MSVC_CISO646_STAGE {
            at(&original, &repo.join(MSVC_CISO646_PATCH))?;
            ensure!(
                original.sha256 == MSVC_CISO646_PATCH_SHA.parse()?,
                "fresh MSVC CUDA compatibility patch identity differs"
            );
        }
        original.authenticate(source, 1 << 20)?;
        snapshot.authenticate(source, 1 << 20)?;
    }
    let mut corrected = rows(&configured["source_inventory"])?
        .iter()
        .filter(|row| row["path"] == MSVC_CISO646_SOURCE);
    let target = corrected
        .next()
        .context("fresh MSVC CUDA patched source inventory row missing")?;
    exact_fields(target, &["path", "bytes", "sha256"])?;
    ensure!(
        corrected.next().is_none() && sha(&target["sha256"])? == MSVC_CISO646_SOURCE_SHA.parse()?,
        "fresh MSVC CUDA patched source inventory identity differs"
    );
    let mut target = target.clone();
    target["path"] = serde_json::json!(root.join("source").join(MSVC_CISO646_SOURCE));
    pin(&target, false)?.authenticate(source, 1 << 20)
}

fn verify_archive_roster(source: &dyn Source, build: &Path, compiled: &Value) -> Result<()> {
    let libraries = rows(&compiled["libraries"])?;
    ensure!(
        (2..=512).contains(&libraries.len()),
        "fresh build library census differs"
    );
    let mut names = BTreeSet::new();
    let mut session = None;
    let build_key = build
        .to_str()
        .context("fresh build path must be UTF-8")?
        .replace('/', "\\")
        .to_ascii_lowercase()
        + "\\";
    for row in libraries {
        let file = pin(row, false)?;
        let normalized = file.path.replace('/', "\\").to_ascii_lowercase();
        let rel = normalized
            .strip_prefix(&build_key)
            .context("fresh library outside same build")?;
        ensure!(
            rel.ends_with(".lib")
                && rel.split('\\').all(|part| !part.is_empty()
                    && part != "."
                    && part != ".."
                    && !part.contains(':')
                    && !part.ends_with(['.', ' ']))
                && !rel.ends_with("\\onnxruntime.lib")
                && rel != "onnxruntime.lib"
                && names.insert(normalized.clone()),
            "unsafe, duplicate or second-core fresh library"
        );
        if rel == "onnxruntime_session.lib" {
            session = Some(file);
        }
    }
    session
        .context("fresh session archive missing")?
        .authenticate(source, MAX_FILE)
}

pub(in crate::arbiter::runtime::cuda) struct FreshProducer {
    pub static_core_build: Sha256Digest,
    pub provider_dlls: [PublicFile; 2],
}

pub(in crate::arbiter::runtime::cuda) fn try_validate_fresh_producer(
    source: &dyn Source,
    raw_pin: &ResourceFilePin,
    proof_pin: &ResourceFilePin,
) -> Result<Option<FreshProducer>> {
    let proof = json(source, proof_pin)?;
    if proof["schema"] != FRESH_BUILD_SCHEMA {
        return Ok(None);
    }
    validate_fresh_producer(source, raw_pin, &proof).map(Some)
}

pub(super) fn validate_fresh_producer(
    source: &dyn Source,
    raw_pin: &ResourceFilePin,
    proof: &Value,
) -> Result<FreshProducer> {
    let approved_driver = REVIEWED_COMPILER_OWNER
        .context("fresh producer compile driver has no reviewed authority")?
        .parse()?;
    validate(
        source,
        raw_pin,
        proof,
        Path::new(ANCHOR),
        REVIEWED_RECIPE.parse()?,
        approved_driver,
    )
}

fn validate(
    source: &dyn Source,
    raw_pin: &ResourceFilePin,
    proof: &Value,
    anchor: &Path,
    approved_recipe: Sha256Digest,
    approved_driver: Sha256Digest,
) -> Result<FreshProducer> {
    ensure!(
        proof["schema"] == FRESH_BUILD_SCHEMA
            && nonzero(approved_recipe)
            && nonzero(approved_driver),
        "unapproved fresh producer evidence"
    );
    let _ = canonical_sha(proof)?;
    let compile_pin = pin(&proof["build_receipt"], false)?;
    ensure!(
        raw_pin.sha256 == compile_pin.sha256,
        "fresh original and staged producer SHA differ"
    );
    let compiled = compile_pin.json(source)?;
    if raw_pin != &compile_pin.resource() {
        ensure!(
            read(source, raw_pin, MAX_METADATA)?.len() as u64 == compile_pin.bytes,
            "fresh staged producer length differs"
        );
    }
    ensure!(
        compiled["schema"] == COMPILED
            && compiled["status"] == "COMPILED_UNQUALIFIED"
            && compiled["qualification"] == false,
        "settled unqualified fresh compile evidence required"
    );
    let driver = pin(&compiled["driver"], false)?;
    let configure_pin = pin(&compiled["configured"], false)?;
    let recipe_pin = pin(&compiled["recipe"], false)?;
    ensure!(
        driver.sha256 == approved_driver && recipe_pin.sha256 == approved_recipe,
        "fresh producer approval or configure recipe identity differs"
    );
    let recipe = recipe_pin.json(source)?;
    let configured = configure_pin.json(source)?;
    let root = PathBuf::from(string(&recipe["fresh_output"])?);
    let build = root.join("b").join("Release");
    let repo = PathBuf::from(string(&recipe["repository_root"])?);
    ensure!(
        root.parent().is_some_and(|parent| parent == anchor)
            && root.file_name().is_some()
            && recipe["schema"] == CONFIGURE_RECIPE
            && recipe["execution_authorized"] == true
            && recipe["qualification"] == false
            && repo.is_absolute()
            && configured["schema"] == CONFIGURED
            && configured["status"] == "CONFIGURED_UNQUALIFIED"
            && configured["qualification"] == false
            && configured["recipe"] == compiled["recipe"]
            && configured["toolchain"] == compiled["toolchain"]
            && configured["toolchain"] == recipe["tools"]
            && configured["collector_source"] == compiled["collector_source"]
            && configured["configured_graph"] == compiled["configured_graph"]
            && path_eq(string(&compiled["build_root"])?, &build)?,
        "fresh configure and compile lineage differs"
    );
    ensure!(
        recipe["msvc_ciso646_compat"] == true,
        "fresh configure recipe lacks required MSVC CUDA compatibility scope"
    );
    ensure!(
        recipe["output_audit_owner"] == compiled["driver"],
        "fresh configure output audit owner differs from the reviewed compile driver"
    );
    at(&compile_pin, &root.join("fresh-compile.json"))?;
    at(&configure_pin, &root.join("fresh-configure.json"))?;
    at(
        &driver,
        &repo.join("scripts/cuda-static-runtime/build/fresh_compile.py"),
    )?;
    driver.authenticate(source, 1 << 20)?;
    ensure!(
        build_proof(&proof["build_receipt"], &compiled, &configured, &recipe)? == *proof,
        "fresh build proof does not reproduce the authenticated producer facts"
    );
    verify_configure_owners(source, &repo, &recipe)?;
    let source_lock = pin(&recipe["owners"]["lock"], false)?;
    let cpu_lock = pin(&recipe["owners"]["cpu_lock"], false)?;
    let lock = source_lock.json(source)?;
    let cpu = cpu_lock.json(source)?;
    ensure!(
        lock["target"] == TARGET
            && lock["ort_version"] == "1.30.0"
            && cpu["ort_version"] == "1.30.0"
            && sha(&lock["cpu_lock_sha256"])? == cpu_lock.sha256
            && lock["cpu_owner_sha256"] == recipe["owners"]["cpu"]["sha256"]
            && lock["upstream_commit"] == cpu["upstream_commit"],
        "fresh ORT and CPU source locks differ"
    );
    let collector = pin(&compiled["collector_source"], false)?;
    at(&collector, &root.join("source").join(COLLECTOR))?;
    ensure!(
        collector.sha256 == COLLECTOR_SHA.parse::<Sha256Digest>()?,
        "fresh final collector source differs"
    );
    collector.authenticate(source, 1 << 20)?;
    verify_patches(source, &root, &repo, &configured)?;

    let configure_stage = &configured["configure"];
    let retained_configure = source.read_retained_configure_receipt(&root, MAX_METADATA)?;
    ensure!(
        strict::parse(&retained_configure)? == *configure_stage,
        "fresh retained configure stage receipt differs"
    );
    let argv = command_args(&configure_stage["argv"])?;
    let cmake = verify_tools(source, &recipe, &argv)?;
    configured_graph(
        source,
        &root,
        &build,
        &compiled,
        &argv,
        &cmake,
        &recipe["tools"],
    )?;
    let times = windows(&configured, &compiled, &recipe)?;
    let (first, last) = verify_stage(
        source,
        &root,
        configure_stage,
        "configure",
        &argv,
        times.original,
    )?;
    ensure!(
        times.configure_start <= first && last <= times.configure_finish,
        "fresh configure tree is outside its original window"
    );
    let stages = rows(&compiled["stages"])?;
    let retained = rows(&compiled["stage_receipts"])?;
    let expected = [
        ("core-session-build", &["onnxruntime_session"][..]),
        ("build", &["all", "re2"][..]),
    ];
    ensure!(
        stages.len() == expected.len() && retained.len() == stages.len(),
        "fresh compile stage roster differs"
    );
    let mut previous = times.compile_start;
    for ((stage, receipt), (name, targets)) in stages.iter().zip(retained).zip(expected) {
        let receipt = pin(receipt, false)?;
        at(
            &receipt,
            &root.join("logs").join(format!("{name}.receipt.json")),
        )?;
        ensure!(
            receipt.json(source)? == *stage,
            "fresh retained compile stage receipt differs"
        );
        let command = expected_build_command(&cmake, &build, targets)?;
        let (began, finish) =
            verify_stage(source, &root, stage, name, &command, times.compile_cutoff)?;
        ensure!(
            previous <= began && finish <= times.compile_finish,
            "fresh build stages are out of order"
        );
        previous = finish;
    }
    verify_archive_roster(source, &build, &compiled)?;
    let provider_dlls = verify_providers(source, &build, &compiled)?;
    source.recheck()?;
    Ok(FreshProducer {
        static_core_build: compile_pin.sha256,
        provider_dlls,
    })
}

#[cfg(test)]
mod tests;
