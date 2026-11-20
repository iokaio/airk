// SPDX-License-Identifier: Apache-2.0
//! Validate the owned ORT profiler's ordered per-forward node and tensor evidence.
use super::*;
use airk_retrieval::arbiter::runtime::cuda::GraphMetadataRow;

const CPU: &str = "CPUExecutionProvider";

#[derive(Default, Serialize)]
pub(super) struct ProfiledRun {
    tokens: usize,
    model_run_us: u64,
    node_count: usize,
    cpu_nodes: usize,
    cpu_input_bytes: u64,
    cpu_output_bytes: u64,
    cpu_max_tensor_bytes: u64,
}

fn duration(event: &Value) -> Result<u64> {
    event["dur"].as_u64().context("ORT event duration missing")
}

fn interval(event: &Value) -> Result<(u64, u64)> {
    let start = event["ts"].as_u64().context("ORT event time missing")?;
    let end = start
        .checked_add(duration(event)?)
        .context("ORT event time overflow")?;
    Ok((start, end))
}

#[derive(Clone, Copy)]
enum CpuControlScope {
    Fp32,
    Fp16,
}

fn control_tensors(values: &Value, scope: CpuControlScope) -> Result<(u64, u64)> {
    let maximum_vector_length = match scope {
        CpuControlScope::Fp32 => 5,
        CpuControlScope::Fp16 => 4,
    };
    let mut total = 0;
    let mut maximum = 0;
    for value in values.as_array().context("ORT tensor IO roster missing")? {
        let fields = value
            .as_object()
            .context("ORT typed tensor entry is not an object")?;
        ensure!(fields.len() == 1, "ORT typed tensor entry is ambiguous");
        let (kind, dims) = fields.iter().next().unwrap();
        let width = match kind.as_str() {
            "int64" => 8,
            "bool" => 1,
            "int32" if matches!(scope, CpuControlScope::Fp16) => 4,
            _ => anyhow::bail!("non-control ORT CPU tensor type {kind}"),
        };
        let dims = dims.as_array().context("ORT tensor shape missing")?;
        let elements = match dims.as_slice() {
            [] => 1,
            [axis] => axis
                .as_u64()
                .filter(|n| *n <= maximum_vector_length)
                .context("CPU vector exceeded its reviewed length bound")?,
            _ => anyhow::bail!("CPU tensor exceeds scalar or fixed vector scope"),
        };
        let bytes = width * elements;
        total += bytes;
        maximum = maximum.max(bytes);
    }
    Ok((total, maximum))
}

fn arg_u64(args: &Value, key: &str) -> Result<u64> {
    args[key]
        .as_str()
        .with_context(|| format!("missing ORT {key}"))?
        .parse()
        .with_context(|| format!("invalid ORT {key}"))
}

fn record_cpu_event(run: &mut ProfiledRun, args: &Value, scope: CpuControlScope) -> Result<()> {
    let (input, input_max) = control_tensors(&args["input_type_shape"], scope)?;
    let (output, output_max) = control_tensors(&args["output_type_shape"], scope)?;
    ensure!(
        arg_u64(args, "activation_size")?.checked_add(arg_u64(args, "parameter_size")?)
            == Some(input)
            && arg_u64(args, "output_size")? == output,
        "ORT CPU tensor shape bytes differ from observed input/output bytes"
    );
    run.cpu_nodes += 1;
    run.cpu_input_bytes += input;
    run.cpu_output_bytes += output;
    run.cpu_max_tensor_bytes = run.cpu_max_tensor_bytes.max(input_max).max(output_max);
    Ok(())
}

pub(super) fn profiled_runs(
    bytes: &[u8],
    graph: &[GraphMetadataRow],
    widths: &[usize],
) -> Result<Vec<ProfiledRun>> {
    profiled_runs_for_scope(bytes, graph, widths, CpuControlScope::Fp32)
}

pub(super) fn large_fp16_t32_profiled_run(
    bytes: &[u8],
    graph: &[GraphMetadataRow],
    placement: &str,
) -> Result<ProfiledRun> {
    let expected_nodes = match placement {
        "fff6dbe2f0c2fac4b9ba215da7d5029421d2706ee48c01c2485b9f157ee15ca6" => 2234,
        "0d00e2fabdb5fe54474041fb1f2787502349b78ce40436b955cd4433633a4462" => 2486,
        "96641f4cf15ddd6cf1209402eae4ed366b9f77f00bfc28f1c894e67143ca4a16" => 2882,
        _ => anyhow::bail!("unreviewed Large T32 execution graph"),
    };
    let run = profiled_runs_for_scope(bytes, graph, &[32], CpuControlScope::Fp16)?
        .pop()
        .context("Large T32 serial profile missing")?;
    ensure!(
        run.node_count == expected_nodes && run.cpu_nodes == 478 && run.cpu_max_tensor_bytes <= 32,
        "Large FP16 T32 profile exceeds its reviewed final graph or CPU control bounds"
    );
    Ok(run)
}

#[test]
fn large_body_trace_refuses_unreviewed_placement() {
    assert!(matches!(large_fp16_t32_profiled_run(&[], &[], "unreviewed"),
        Err(error) if error.to_string().contains("unreviewed Large T32")));
}

pub(super) fn compact_fp16_t32_profiled_run(
    bytes: &[u8],
    graph: &[GraphMetadataRow],
    expected_nodes: usize,
) -> Result<ProfiledRun> {
    compact_fp16_profiled_run(bytes, graph, 32, expected_nodes)
}

pub(super) fn compact_qkv_layer0_t1024_profiled_run(
    bytes: &[u8],
    graph: &[GraphMetadataRow],
) -> Result<ProfiledRun> {
    compact_fp16_profiled_run(bytes, graph, 1024, 1942)
}

fn compact_fp16_profiled_run(
    bytes: &[u8],
    graph: &[GraphMetadataRow],
    tokens: usize,
    expected_nodes: usize,
) -> Result<ProfiledRun> {
    let run = profiled_runs_for_scope(bytes, graph, &[tokens], CpuControlScope::Fp16)?
        .into_iter()
        .next()
        .with_context(|| format!("Compact FP16 T{tokens} profile run missing"))?;
    ensure!(
        run.node_count == expected_nodes && run.cpu_nodes == 374 && run.cpu_max_tensor_bytes <= 32,
        "Compact FP16 profile differs from the reviewed node and CPU tensor bounds"
    );
    Ok(run)
}

pub(super) fn compact_fp16_development_profiled_runs(
    bytes: &[u8],
    graph: &[GraphMetadataRow],
    widths: &[usize],
) -> Result<Vec<ProfiledRun>> {
    ensure!(
        [128, 1024, 8192].starts_with(widths),
        "Compact Q/K/V development profile width order differs"
    );
    let runs = profiled_runs_for_scope(bytes, graph, widths, CpuControlScope::Fp16)?;
    ensure!(
        runs.iter().all(|run| run.node_count == 1942
            && run.cpu_nodes == 374
            && run.cpu_max_tensor_bytes <= 32),
        "Compact Q/K/V development profile differs from reviewed node and CPU tensor bounds"
    );
    Ok(runs)
}

fn profiled_runs_for_scope(
    bytes: &[u8],
    graph: &[GraphMetadataRow],
    widths: &[usize],
    scope: CpuControlScope,
) -> Result<Vec<ProfiledRun>> {
    let events: Vec<Value> = serde_json::from_slice(bytes)?;
    let mut nodes = BTreeMap::new();
    for row in graph {
        if let GraphMetadataRow::Node {
            graph,
            node,
            op,
            ep,
            ..
        } = row
        {
            ensure!(
                *graph == 0 && nodes.insert(*node, (op, ep)).is_none(),
                "profile requires a complete unique root node roster"
            );
        }
    }
    ensure!(!nodes.is_empty(), "profile graph contains no nodes");
    let mut spans = events
        .iter()
        .filter(|event| event["cat"] == "Session" && event["name"] == "model_run")
        .map(interval)
        .collect::<Result<Vec<_>>>()?;
    spans.sort_unstable();
    ensure!(
        spans.len() == widths.len()
            && spans.iter().all(|(start, end)| start < end)
            && spans.windows(2).all(|spans| spans[0].1 <= spans[1].0),
        "ORT profile lacks the complete serial model_run windows"
    );
    let mut seen = vec![BTreeSet::new(); spans.len()];
    let mut runs = spans
        .iter()
        .zip(widths)
        .map(|(&(start, end), &tokens)| ProfiledRun {
            tokens,
            model_run_us: end - start,
            ..ProfiledRun::default()
        })
        .collect::<Vec<_>>();
    for event in events.iter().filter(|event| event["cat"] == "Node") {
        let args = &event["args"];
        let node = arg_u64(args, "node_index")?;
        let (op, ep) = nodes
            .get(&node)
            .context("profiled node missing from final graph")?;
        ensure!(
            args["op_name"] == op.as_str() && args["provider"] == ep.as_str(),
            "profiled node operator or provider drifted"
        );
        let (start, end) = interval(event)?;
        let index = spans
            .iter()
            .position(|&(first, last)| first <= start && start < last && end <= last)
            .context("node kernel is outside a single forward window")?;
        ensure!(
            seen[index].insert(node),
            "duplicate ORT node kernel in one forward"
        );
        let run = &mut runs[index];
        run.node_count += 1;
        if ep.as_str() == CPU {
            record_cpu_event(run, args, scope)?;
        }
    }
    let cpu_count = nodes.values().filter(|(_, ep)| ep.as_str() == CPU).count();
    ensure!(
        runs.iter()
            .all(|run| run.node_count == nodes.len() && run.cpu_nodes == cpu_count),
        "ORT forward lacks the complete final-node census"
    );
    Ok(runs)
}

#[test]
fn compact_fp16_profile_allows_only_its_fixed_integer_scope() -> Result<()> {
    let controls = json!([{"int32":[1]}, {"int64":[4]}, {"bool":[]}]);
    assert_eq!(control_tensors(&controls, CpuControlScope::Fp16)?, (37, 32));
    assert!(control_tensors(&controls, CpuControlScope::Fp32).is_err());
    let fp32_only = json!([{"int64":[5]}]);
    assert_eq!(
        control_tensors(&fp32_only, CpuControlScope::Fp32)?,
        (40, 40)
    );
    assert!(control_tensors(&fp32_only, CpuControlScope::Fp16).is_err());
    for value in [
        json!([{"float16":[]}]),
        json!([{"int32":[1, 1]}]),
        json!([{"bool":[5]}]),
    ] {
        assert!(control_tensors(&value, CpuControlScope::Fp16).is_err());
    }
    Ok(())
}

#[test]
fn compact_profile_refuses_missing_duplicate_or_non_control_nodes() -> Result<()> {
    let graph = [GraphMetadataRow::Node {
        graph: 0,
        node: 9,
        name: "test".into(),
        domain: String::new(),
        op: "Gather".into(),
        ep: CPU.into(),
        version: 13,
    }];
    let run = json!({"cat":"Session","name":"model_run","ts":1,"dur":20});
    let entry = json!({"cat":"Node","name":"test_kernel_time","ts":9,"dur":0,
        "args":{"node_index":"9","op_name":"Gather","provider":CPU,
            "input_type_shape":[{"int64":[5]},{"bool":[]}],"output_type_shape":[{"int64":[]}],
            "activation_size":"1","parameter_size":"40","output_size":"8"}});
    let valid = profiled_runs(
        &serde_json::to_vec(&[run.clone(), entry.clone()])?,
        &graph,
        &[32],
    )?;
    assert_eq!(valid[0].node_count, 1);
    assert_eq!(valid[0].cpu_input_bytes, 41);
    assert_eq!(valid[0].cpu_output_bytes, 8);
    assert_eq!(valid[0].cpu_max_tensor_bytes, 40);
    for (field, drift) in [
        ("/ts", json!(30)),
        ("/args/node_index", json!("8")),
        ("/args/input_type_shape/0", json!({"float":[5]})),
        ("/args/input_type_shape/0", json!({"int64":[6]})),
        ("/args/activation_size", json!("0")),
        ("/args/provider", json!("CUDAExecutionProvider")),
    ] {
        let mut changed = entry.clone();
        *changed.pointer_mut(field).unwrap() = drift;
        assert!(
            profiled_runs(&serde_json::to_vec(&[run.clone(), changed])?, &graph, &[32]).is_err()
        );
    }
    assert!(
        profiled_runs(
            &serde_json::to_vec(std::slice::from_ref(&run))?,
            &graph,
            &[32]
        )
        .is_err()
    );
    assert!(
        profiled_runs(
            &serde_json::to_vec(&[run, entry.clone(), entry])?,
            &graph,
            &[32]
        )
        .is_err()
    );
    Ok(())
}
