// SPDX-License-Identifier: Apache-2.0
//! Bounded final graph observations from the owning static core; never a policy permit.
pub(super) mod bounded_execution;
pub(in crate::arbiter::runtime) mod cpu_values;
use super::*;
use serde_json::Value;
const MAX_JSON: usize = 8 << 20;
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Row {
    Graph {
        graph: u64,
        parent_graph: Option<u64>,
        parent_node: Option<u64>,
        attribute: String,
        outer_bindings: String,
    },
    Node {
        graph: u64,
        node: u64,
        name: String,
        domain: String,
        op: String,
        ep: String,
        version: i64,
    },
    Value {
        graph: u64,
        value: u64,
        name: String,
        element_type: Option<i64>,
        rank: Option<u64>,
        type_state: String,
        provenance: String,
        producer: Option<u64>,
    },
    Dimension {
        graph: u64,
        value: u64,
        index: u64,
        number: Option<i64>,
        symbol: Option<String>,
    },
    Edge {
        graph: u64,
        node: Option<u64>,
        value: Option<u64>,
        role: u32,
        slot: u64,
    },
    Attribute {
        graph: u64,
        node: u64,
        index: u64,
        name: String,
        attribute_type: i64,
        count: u64,
        readable: bool,
        reason: String,
    },
    AttributeElement {
        graph: u64,
        node: u64,
        attribute: u64,
        index: u64,
        element_kind: u32,
        value: Value,
    },
    Constant {
        graph: u64,
        value: u64,
        element_type: i64,
        count: u64,
        readable: bool,
        reason: String,
        device: String,
    },
    ConstantElement {
        graph: u64,
        value: u64,
        index: u64,
        element_bytes: u32,
        integer: i64,
    },
    End {
        graph: u64,
    },
}
impl Row {
    fn graph(&self) -> u64 {
        match self {
            Self::Graph { graph, .. }
            | Self::Node { graph, .. }
            | Self::Value { graph, .. }
            | Self::Dimension { graph, .. }
            | Self::Edge { graph, .. }
            | Self::Attribute { graph, .. }
            | Self::AttributeElement { graph, .. }
            | Self::Constant { graph, .. }
            | Self::ConstantElement { graph, .. }
            | Self::End { graph } => *graph,
        }
    }
    fn strings(&self) -> Vec<&str> {
        match self {
            Self::Graph { attribute, .. } => vec![attribute],
            Self::Node {
                name,
                domain,
                op,
                ep,
                ..
            } => vec![name, domain, op, ep],
            Self::Value {
                name, provenance, ..
            } => vec![name, provenance],
            Self::Dimension { symbol, .. } => symbol.iter().map(String::as_str).collect(),
            Self::Attribute { name, reason, .. } => vec![name, reason],
            Self::AttributeElement {
                value: Value::String(s),
                ..
            } => vec![s],
            Self::Constant { reason, device, .. } => vec![reason, device],
            _ => vec![],
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Observation {
    pub schema: &'static str,
    pub complete: bool,
    pub shim_status: i32,
    pub ort_error_code: i32,
    pub reported_counts: [u64; 6],
    pub encoded_bytes: usize,
    pub failure: Option<String>,
    pub rows: Vec<Row>,
}
impl Observation {
    pub fn validate_against(&self, placement: &PlacementObservation) -> Result<()> {
        ensure!(
            self.complete
                && self.shim_status == 0
                && self.ort_error_code == 0
                && self.failure.is_none(),
            "final graph metadata incomplete"
        );
        ensure!(
            placement.complete && placement.shim_status == 0 && placement.ort_error_code == 0,
            "comparison placement incomplete"
        );
        ensure!(
            serde_json::to_vec(self)?.len() == self.encoded_bytes && self.encoded_bytes <= MAX_JSON,
            "metadata encoded size differs"
        );
        ensure!(
            placement.scope == "onnxruntime-final-session-state-ep-placement"
                && placement.reported_nodes == placement.nodes.len() as u64,
            "comparison placement count or scope differs"
        );
        let mut replay = Sink::default();
        for row in &self.rows {
            replay.accept(row.clone())?;
        }
        let replay = replay.finish(self.shim_status, self.ort_error_code, self.reported_counts);
        ensure!(
            replay.complete,
            "metadata semantic closure or counters differ"
        );
        let mut observed = Vec::new();
        for row in &self.rows {
            if let Row::Node {
                graph,
                name,
                domain,
                op,
                ep,
                ..
            } = row
            {
                observed.push((
                    *graph,
                    name.as_str(),
                    domain.as_str(),
                    op.as_str(),
                    ep.as_str(),
                ));
            }
        }
        let mut expected = placement
            .nodes
            .iter()
            .map(|n| {
                (
                    n.subgraph_index,
                    n.name.as_str(),
                    n.domain.as_str(),
                    n.op.as_str(),
                    n.ep.as_str(),
                )
            })
            .collect::<Vec<_>>();
        observed.sort_unstable();
        expected.sort_unstable();
        ensure!(
            observed == expected && observed.len() as u64 == self.reported_counts[0],
            "metadata final node identities differ from placement-v1"
        );
        Ok(())
    }
    /// Every consumer comes from an observed ordered explicit/implicit edge.
    pub fn consumers(&self, graph: u64, value: u64) -> Vec<(u64, u32, u64)> {
        self.rows
            .iter()
            .filter_map(|r| match r {
                Row::Edge {
                    graph: g,
                    node: Some(n),
                    value: Some(v),
                    role,
                    slot,
                } if *g == graph && *v == value && *role < 2 => Some((*n, *role, *slot)),
                _ => None,
            })
            .collect()
    }
}
#[cfg(any(
    test,
    all(
        target_arch = "x86_64",
        any(windows, all(target_os = "linux", target_env = "gnu"))
    )
))]
#[repr(C)]
pub(super) struct Raw {
    kind: u32,
    flags: u32,
    graph: u64,
    node: u64,
    value: u64,
    slot: u64,
    integer: i64,
    real: f64,
    text: [*const u8; 4],
    text_bytes: [usize; 4],
}
#[cfg(all(
    target_arch = "x86_64",
    any(windows, all(target_os = "linux", target_env = "gnu"))
))]
pub(super) type Collector = unsafe extern "C" fn(
    *const std::ffi::c_void,
    *const std::ffi::c_void,
    unsafe extern "C" fn(*mut std::ffi::c_void, *const Raw) -> i32,
    *mut std::ffi::c_void,
    *mut u64,
    *mut i32,
) -> i32;
#[derive(Default)]
struct ValueState {
    rank: Option<u64>,
    dims: u64,
    element: Option<i64>,
    provenance: String,
    producer: Option<u64>,
    constant: Option<(u64, u64, bool, i64)>,
}
type AttributeKey = (u64, u64, u64);
// Declared count, observed count, readable flag and ONNX attribute type.
type AttributeState = (u64, u64, bool, i64);

#[derive(Default)]
struct Sink {
    rows: Vec<Row>,
    counts: [u64; 6],
    json_bytes: usize,
    stack: Vec<u64>,
    graphs: u64,
    nodes: BTreeSet<(u64, u64)>,
    values: BTreeMap<(u64, u64), ValueState>,
    slots: BTreeMap<(u64, Option<u64>, u32), u64>,
    outputs: BTreeMap<(u64, u64), u64>,
    graph_inputs: BTreeSet<(u64, u64)>,
    attributes: BTreeMap<AttributeKey, AttributeState>,
    attribute_next: BTreeMap<(u64, u64), u64>,
    failure: Option<String>,
    deadline: Option<Instant>,
}
impl Sink {
    fn accept(&mut self, row: Row) -> Result<()> {
        if let Some(deadline) = self.deadline {
            in_time(deadline)?;
        }
        let bytes = serde_json::to_vec(&row)?.len() + 1;
        ensure!(
            self.json_bytes
                .checked_add(bytes)
                .is_some_and(|n| n <= MAX_JSON - 4096),
            "metadata encoded JSON budget"
        );
        let strings = row.strings();
        ensure!(
            strings.iter().all(|s| s.len() <= 4096),
            "metadata string bound"
        );
        let text_bytes = strings.iter().map(|s| s.len() as u64).sum::<u64>();
        ensure!(
            self.counts[3] + text_bytes <= MAX_JSON as u64 && self.counts[5] < 4194304,
            "metadata callback budget"
        );
        if !matches!(row, Row::Graph { .. }) {
            ensure!(
                self.stack.last() == Some(&row.graph()),
                "metadata graph scope/order"
            );
        }
        match &row {
            Row::Graph {
                graph,
                parent_graph,
                parent_node,
                attribute,
                outer_bindings,
            } => {
                ensure!(
                    *graph == self.graphs && self.graphs < 4096 && self.stack.len() <= 32,
                    "metadata graph identity/depth"
                );
                match (parent_graph, parent_node) {
                    (None, None) => ensure!(
                        *graph == 0
                            && self.stack.is_empty()
                            && attribute.is_empty()
                            && outer_bindings == "not_applicable",
                        "metadata root binding"
                    ),
                    (Some(g), Some(n)) => ensure!(
                        self.stack.last() == Some(g)
                            && self.nodes.contains(&(*g, *n))
                            && !attribute.is_empty()
                            && outer_bindings == "unresolved",
                        "metadata child binding"
                    ),
                    _ => anyhow::bail!("partial parent binding"),
                }
                self.graphs += 1;
                self.stack.push(*graph);
            }
            Row::Node {
                graph,
                node,
                op,
                ep,
                version,
                ..
            } => {
                ensure!(
                    self.counts[0] < 65536
                        && !op.is_empty()
                        && !ep.is_empty()
                        && *version > 0
                        && self.nodes.insert((*graph, *node)),
                    "metadata node duplicate/bound/identity"
                );
                self.counts[0] += 1;
            }
            Row::Value {
                graph,
                value,
                name,
                element_type,
                rank,
                type_state,
                provenance,
                producer,
            } => {
                ensure!(
                    self.counts[1] < 262144 && !name.is_empty() && rank.is_none_or(|n| n <= 16),
                    "metadata value bound"
                );
                ensure!(
                    matches!(
                        type_state.as_str(),
                        "tensor" | "unsupported_non_tensor" | "unknown"
                    ) && (type_state == "tensor" || (rank.is_none() && element_type.is_none())),
                    "metadata value type state"
                );
                ensure!(
                    [
                        "immutable_session_initializer",
                        "overridable_initializer_unread",
                        "node_output",
                        "graph_input",
                        "unresolved_outer_or_internal"
                    ]
                    .contains(&provenance.as_str()),
                    "unknown metadata provenance"
                );
                ensure!(
                    self.values
                        .insert(
                            (*graph, *value),
                            ValueState {
                                rank: *rank,
                                element: *element_type,
                                provenance: provenance.clone(),
                                producer: *producer,
                                ..Default::default()
                            }
                        )
                        .is_none(),
                    "duplicate metadata value"
                );
                self.counts[1] += 1;
            }
            Row::Dimension {
                graph,
                value,
                index,
                number,
                symbol,
            } => {
                let v = self
                    .values
                    .get_mut(&(*graph, *value))
                    .context("dimension before value")?;
                ensure!(
                    v.rank.is_some_and(|rank| *index < rank)
                        && *index == v.dims
                        && !(number.is_some() && symbol.is_some())
                        && number.is_none_or(|n| n >= 0)
                        && symbol.as_ref().is_none_or(|s| !s.is_empty()),
                    "invalid dimension/order"
                );
                v.dims += 1;
            }
            Row::Edge {
                graph,
                node,
                value,
                role,
                slot,
            } => {
                ensure!(
                    self.counts[2] < 524288 && *role <= 5 && ((*role < 3) == node.is_some()),
                    "metadata edge role/bound"
                );
                if let Some(n) = node {
                    ensure!(
                        self.nodes.contains(&(*graph, *n)),
                        "edge references unknown node"
                    );
                }
                if let Some(v) = value {
                    ensure!(
                        self.values.contains_key(&(*graph, *v)),
                        "edge references unknown value"
                    );
                } else {
                    ensure!(*role < 3, "graph boundary cannot be absent");
                }
                let next = self.slots.entry((*graph, *node, *role)).or_default();
                ensure!(*slot == *next, "edge slot missing/repeated");
                *next += 1;
                if *role == 2
                    && let Some(v) = value
                {
                    ensure!(
                        self.outputs.insert((*graph, *v), node.unwrap()).is_none(),
                        "multiple final producers"
                    );
                }
                if *role == 3 {
                    self.graph_inputs.insert((*graph, value.unwrap()));
                }
                self.counts[2] += 1;
            }
            Row::Attribute {
                graph,
                node,
                index,
                name,
                attribute_type,
                count,
                readable,
                reason,
            } => {
                ensure!(
                    self.nodes.contains(&(*graph, *node)) && !name.is_empty(),
                    "attribute owner absent"
                );
                let next = self.attribute_next.entry((*graph, *node)).or_default();
                ensure!(
                    *index == *next && *index < 64,
                    "attribute ordering or per-node bound"
                );
                *next += 1;
                ensure!(
                    !*readable
                        || (*count <= 64
                            && matches!(*attribute_type, 1 | 2 | 3 | 6 | 7 | 8)
                            && (!matches!(*attribute_type, 1..=3) || *count == 1)
                            && reason == "read"),
                    "unsupported readable attribute"
                );
                ensure!(
                    *readable
                        || ["large_array_unread", "unsupported_attribute_unread"]
                            .contains(&reason.as_str()),
                    "attribute missing unread reason"
                );
                self.attributes.insert(
                    (*graph, *node, *index),
                    (*count, 0, *readable, *attribute_type),
                );
            }
            Row::AttributeElement {
                graph,
                node,
                attribute,
                index,
                element_kind,
                value,
            } => {
                let a = self
                    .attributes
                    .get_mut(&(*graph, *node, *attribute))
                    .context("attribute element before header")?;
                let expected = match a.3 {
                    2 | 7 => 1,
                    1 | 6 => 2,
                    3 | 8 => 3,
                    _ => 0,
                };
                ensure!(
                    a.2 && *index == a.1 && *index < a.0 && *element_kind == expected,
                    "attribute payload order/type"
                );
                ensure!(
                    match element_kind {
                        1 => value.as_i64().is_some(),
                        2 => value.as_f64().is_some_and(f64::is_finite),
                        3 => value.as_str().is_some(),
                        _ => false,
                    },
                    "attribute payload representation"
                );
                a.1 += 1;
            }
            Row::Constant {
                graph,
                value,
                element_type,
                count,
                readable,
                reason,
                device,
            } => {
                let v = self
                    .values
                    .get_mut(&(*graph, *value))
                    .context("constant before value")?;
                ensure!(
                    v.provenance == "immutable_session_initializer" && v.constant.is_none(),
                    "constant provenance/duplicate"
                );
                ensure!(
                    !*readable
                        || (*count <= 64
                            && [6, 7, 9].contains(element_type)
                            && reason == "read"
                            && device == "CPU"
                            && v.element == Some(*element_type)),
                    "unsafe readable constant"
                );
                ensure!(
                    *readable
                        || [
                            "not_tensor",
                            "non_CPU_unread",
                            "unsupported_type_unread",
                            "large_constant_unread",
                            "constant_payload_budget_unread"
                        ]
                        .contains(&reason.as_str()),
                    "constant unread reason absent"
                );
                v.constant = Some((*count, 0, *readable, *element_type));
            }
            Row::ConstantElement {
                graph,
                value,
                index,
                element_bytes,
                integer,
            } => {
                let c = self
                    .values
                    .get_mut(&(*graph, *value))
                    .and_then(|v| v.constant.as_mut())
                    .context("constant payload before header")?;
                let width = match c.3 {
                    6 => 4,
                    7 => 8,
                    9 => 1,
                    _ => 0,
                };
                ensure!(
                    c.2 && *index == c.1
                        && *index < c.0
                        && *element_bytes == width
                        && self.counts[4] + u64::from(width) <= 1 << 20,
                    "constant payload bound/type/order"
                );
                ensure!(
                    (c.3 != 6 || i32::try_from(*integer).is_ok())
                        && (c.3 != 9 || [0, 1].contains(integer)),
                    "constant value outside element type"
                );
                c.1 += 1;
                self.counts[4] += u64::from(width);
            }
            Row::End { graph } => {
                ensure!(self.stack.pop() == Some(*graph), "graph end order");
            }
        }
        self.counts[3] += text_bytes;
        self.counts[5] += 1;
        self.json_bytes += bytes;
        self.rows.push(row);
        Ok(())
    }
    fn closure(&self) -> Result<()> {
        ensure!(
            self.stack.is_empty() && self.graphs > 0 && self.counts[0] > 0,
            "metadata graph did not complete"
        );
        for (key, v) in &self.values {
            ensure!(v.rank.is_none_or(|r| r == v.dims), "missing dimensions");
            if let Some(n) = v.producer {
                ensure!(
                    self.nodes.contains(&(key.0, n)) && self.outputs.get(key) == Some(&n),
                    "value producer differs from output edges"
                );
            }
            match v.provenance.as_str() {
                "node_output" => ensure!(v.producer.is_some(), "node output has no producer"),
                "immutable_session_initializer" => ensure!(
                    v.constant.is_some(),
                    "immutable initializer metadata missing"
                ),
                "graph_input" => ensure!(
                    self.graph_inputs.contains(key),
                    "input provenance differs from graph boundary"
                ),
                _ => (),
            }
            if let Some((count, seen, readable, _)) = v.constant {
                ensure!(!readable || count == seen, "constant payload missing");
            }
        }
        for (key, node) in &self.outputs {
            ensure!(
                self.values
                    .get(key)
                    .is_some_and(|v| v.producer == Some(*node)),
                "output edge lacks matching producer metadata"
            );
        }
        ensure!(
            self.attributes
                .values()
                .all(|(count, seen, readable, _)| !*readable || count == seen),
            "attribute payload missing"
        );
        Ok(())
    }
    fn finish(mut self, status: i32, error: i32, reported: [u64; 6]) -> Observation {
        if self.failure.is_none() {
            if let Err(error) = self.closure() {
                self.failure = Some(format!("{error:#}"));
            } else if self.counts != reported {
                self.failure = Some("collector counters differ".into());
            }
        }
        let mut value = Observation {
            schema: "airk.private.ort-final-graph-metadata/1",
            complete: status == 0 && error == 0 && self.failure.is_none(),
            shim_status: status,
            ort_error_code: error,
            reported_counts: reported,
            encoded_bytes: 0,
            failure: self.failure,
            rows: self.rows,
        };
        for _ in 0..8 {
            let n = serde_json::to_vec(&value).map_or(MAX_JSON + 1, |v| v.len());
            if n == value.encoded_bytes {
                break;
            }
            value.encoded_bytes = n;
        }
        if value.encoded_bytes > MAX_JSON {
            value.complete = false;
            value.failure = Some("metadata encoded budget exceeded".into());
        }
        value
    }
}
#[cfg(any(
    test,
    all(
        target_arch = "x86_64",
        any(windows, all(target_os = "linux", target_env = "gnu"))
    )
))]
unsafe fn decode(raw: &Raw) -> Result<Row> {
    let mut strings = Vec::with_capacity(4);
    for i in 0..4 {
        ensure!(
            raw.text_bytes[i] <= 4096 && (raw.text_bytes[i] == 0 || !raw.text[i].is_null()),
            "invalid metadata string span"
        );
        let bytes = if raw.text_bytes[i] == 0 {
            &[][..]
        } else {
            unsafe { std::slice::from_raw_parts(raw.text[i], raw.text_bytes[i]) }
        };
        strings.push(std::str::from_utf8(bytes)?.to_owned());
    }
    let optional = |v| if v == u64::MAX { None } else { Some(v) };
    let [a, b, c, d]: [String; 4] = strings.try_into().unwrap();
    Ok(match raw.kind {
        1 => {
            ensure!(raw.flags <= 1, "unknown outer binding flag");
            Row::Graph {
                graph: raw.graph,
                parent_graph: optional(raw.value),
                parent_node: optional(raw.node),
                attribute: a,
                outer_bindings: if raw.flags == 0 {
                    "not_applicable"
                } else {
                    "unresolved"
                }
                .into(),
            }
        }
        2 => Row::Node {
            graph: raw.graph,
            node: raw.node,
            name: a,
            domain: b,
            op: c,
            ep: d,
            version: raw.integer,
        },
        3 => {
            ensure!(raw.flags <= 2, "unknown value type flag");
            Row::Value {
                graph: raw.graph,
                value: raw.value,
                name: a,
                element_type: if raw.flags == 1 {
                    Some(raw.integer)
                } else {
                    None
                },
                rank: optional(raw.slot),
                type_state: match raw.flags {
                    1 => "tensor",
                    2 => "unsupported_non_tensor",
                    _ => "unknown",
                }
                .into(),
                provenance: b,
                producer: optional(raw.node),
            }
        }
        4 => {
            ensure!(raw.flags <= 2, "unknown dimension flag");
            Row::Dimension {
                graph: raw.graph,
                value: raw.value,
                index: raw.slot,
                number: if raw.flags == 1 {
                    Some(raw.integer)
                } else {
                    None
                },
                symbol: if raw.flags == 2 { Some(a) } else { None },
            }
        }
        5 => Row::Edge {
            graph: raw.graph,
            node: optional(raw.node),
            value: optional(raw.value),
            role: raw.flags,
            slot: raw.slot,
        },
        6 => {
            ensure!(raw.flags <= 1, "attribute readability flag");
            Row::Attribute {
                graph: raw.graph,
                node: raw.node,
                index: raw.slot,
                name: a,
                attribute_type: raw.integer,
                count: raw.value,
                readable: raw.flags == 1,
                reason: b,
            }
        }
        7 => Row::AttributeElement {
            graph: raw.graph,
            node: raw.node,
            attribute: raw.slot,
            index: raw.value,
            element_kind: raw.flags,
            value: match raw.flags {
                1 => Value::from(raw.integer),
                2 => Value::Number(
                    serde_json::Number::from_f64(raw.real).context("nonfinite attribute")?,
                ),
                3 => Value::String(a),
                _ => anyhow::bail!("unknown attribute element kind"),
            },
        },
        8 => {
            ensure!(raw.flags <= 1, "constant readability flag");
            Row::Constant {
                graph: raw.graph,
                value: raw.value,
                element_type: raw.integer,
                count: raw.slot,
                readable: raw.flags == 1,
                reason: a,
                device: b,
            }
        }
        9 => Row::ConstantElement {
            graph: raw.graph,
            value: raw.value,
            index: raw.slot,
            element_bytes: raw.flags,
            integer: raw.integer,
        },
        10 => Row::End { graph: raw.graph },
        _ => anyhow::bail!("unknown metadata record kind"),
    })
}
#[cfg(all(
    target_arch = "x86_64",
    any(windows, all(target_os = "linux", target_env = "gnu"))
))]
unsafe extern "C" fn callback(user: *mut std::ffi::c_void, raw: *const Raw) -> i32 {
    if user.is_null() || raw.is_null() {
        return 1;
    }
    let sink = unsafe { &mut *user.cast::<Sink>() };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
        let raw = unsafe { &*raw };
        if raw.kind == 0 {
            ensure!(
                raw.flags == 0
                    && raw.graph == 0
                    && raw.node == 0
                    && raw.value == 0
                    && raw.slot == 0
                    && raw.integer == 0
                    && raw.real == 0.0
                    && raw.text.iter().all(|p| p.is_null())
                    && raw.text_bytes == [0; 4],
                "invalid cancellation checkpoint"
            );
            if let Some(deadline) = sink.deadline {
                in_time(deadline)?;
            }
            return Ok(());
        }
        sink.accept(unsafe { decode(raw) }?)
    }));
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => {
            sink.failure = Some(format!("{error:#}"));
            1
        }
        Err(_) => {
            sink.failure = Some("metadata callback panicked".into());
            1
        }
    }
}
#[cfg(all(
    target_arch = "x86_64",
    any(windows, all(target_os = "linux", target_env = "gnu"))
))]
pub(super) unsafe fn observe(
    collect: Collector,
    session: &ort::session::Session,
    deadline: Instant,
    placement: &PlacementObservation,
) -> Observation {
    use ort::AsPointer;
    let mut sink = Sink {
        deadline: Some(deadline),
        ..Default::default()
    };
    let mut counts = [0; 6];
    let mut error = 0;
    let status = if Instant::now() >= deadline {
        sink.failure = Some("original metadata deadline expired".into());
        1
    } else {
        unsafe {
            collect(
                ort::sys::OrtGetApiBase().cast(),
                session.ptr().cast(),
                callback,
                (&mut sink as *mut Sink).cast(),
                counts.as_mut_ptr(),
                &mut error,
            )
        }
    };
    if Instant::now() >= deadline {
        sink.failure = Some("original metadata deadline expired".into());
    }
    let mut observation = sink.finish(status, error, counts);
    if observation.complete
        && let Err(error) = observation.validate_against(placement)
    {
        observation.complete = false;
        observation.failure = Some(format!("{error:#}"));
    }
    for _ in 0..8 {
        let n = serde_json::to_vec(&observation).map_or(MAX_JSON + 1, |v| v.len());
        if n == observation.encoded_bytes {
            break;
        }
        observation.encoded_bytes = n;
    }
    observation
}
#[cfg(test)]
mod tests;
