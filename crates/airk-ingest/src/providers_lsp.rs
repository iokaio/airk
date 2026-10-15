// SPDX-License-Identifier: Apache-2.0
//! A bounded LSP 3.17 client. It requests evidence, never edits or executes commands.
use super::*;
use crate::provider_process::{Process, Writer, remaining};
use serde_json::json;
use std::{
    io::{BufRead, BufReader, Read},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const MAX_FRAME: usize = 4_194_304;

fn read_frame(reader: &mut impl BufRead) -> Result<(Value, usize)> {
    let mut length = None;
    let mut header_bytes = 0;
    loop {
        let mut line = Vec::new();
        reader.take(8193).read_until(b'\n', &mut line)?;
        header_bytes += line.len();
        ensure!(
            !line.is_empty() && header_bytes <= 8192 && line.ends_with(b"\r\n"),
            "invalid/oversized LSP frame header"
        );
        if line == b"\r\n" {
            break;
        }
        let text = std::str::from_utf8(&line)?;
        let (name, value) = text
            .trim_end()
            .split_once(':')
            .context("malformed LSP header")?;
        if name.eq_ignore_ascii_case("Content-Length") {
            ensure!(length.is_none(), "duplicate LSP content length");
            let parsed = value.trim().parse::<usize>()?;
            ensure!((1..=MAX_FRAME).contains(&parsed), "LSP frame exceeds 4 MiB");
            length = Some(parsed);
        }
    }
    let mut bytes = vec![0; length.context("LSP content length missing")?];
    reader.read_exact(&mut bytes)?;
    Ok((serde_json::from_slice(&bytes)?, header_bytes + bytes.len()))
}

struct Session {
    process: Option<Process>,
    writer: Writer,
    receiver: Option<mpsc::Receiver<Result<Value>>>,
    threads: Vec<thread::JoinHandle<()>>,
    deadline: Instant,
    id: u64,
    messages: usize,
    root_uri: String,
    server_status: Option<(String, bool)>,
}
impl Session {
    fn start(workspace: &Workspace, request: &LiveRequest) -> Result<Self> {
        let mut process = Process::spawn(&request.command, workspace.directory.path(), &[])?;
        let writer = Writer::new(process.child.stdin.take().context("LSP stdin missing")?);
        let output = process.child.stdout.take().context("LSP stdout missing")?;
        let errors = process.child.stderr.take().context("LSP stderr missing")?;
        let (sender, receiver) = mpsc::sync_channel(4);
        let error_sender = sender.clone();
        let output_thread = thread::spawn(move || {
            let mut reader = BufReader::new(output);
            let mut total = 0;
            loop {
                let result = read_frame(&mut reader).and_then(|(message, frame_bytes)| {
                    total += frame_bytes;
                    ensure!(total <= 33_554_432, "LSP cumulative output exceeds 32 MiB");
                    Ok(message)
                });
                let failed = result.is_err();
                if sender.send(result).is_err() || failed {
                    break;
                }
            }
        });
        let error_thread = thread::spawn(move || {
            if let Err(error) = crate::provider_process::read_capped(errors, 65_536) {
                let _ = error_sender.send(Err(error));
            }
        });
        let root_uri = url::Url::from_directory_path(workspace.directory.path())
            .map_err(|_| anyhow::anyhow!("LSP root URI"))?
            .to_string();
        Ok(Self {
            process: Some(process),
            writer,
            receiver: Some(receiver),
            threads: vec![output_thread, error_thread],
            deadline: Instant::now() + Duration::from_millis(request.command.timeout_ms),
            id: 0,
            messages: 0,
            root_uri,
            server_status: None,
        })
    }
    fn send(&self, message: Value) -> Result<()> {
        let bytes = serde_json::to_vec(&message)?;
        ensure!(bytes.len() <= MAX_FRAME, "outgoing LSP frame exceeds 4 MiB");
        let mut framed = format!("Content-Length: {}\r\n\r\n", bytes.len()).into_bytes();
        framed.extend(bytes);
        self.writer.send(framed, self.deadline)
    }
    fn notify(&self, method: &str, params: Value) -> Result<()> {
        self.send(json!({"jsonrpc":"2.0","method":method,"params":params}))
    }
    fn receive(&mut self) -> Result<Value> {
        let message = self
            .receiver
            .as_ref()
            .context("LSP disconnected")?
            .recv_timeout(remaining(self.deadline)?)
            .context("LSP response timeout/disconnect")??;
        self.messages += 1;
        ensure!(
            self.messages <= 8192 && message.get("jsonrpc").and_then(Value::as_str) == Some("2.0"),
            "invalid/excess LSP messages"
        );
        Ok(message)
    }
    fn service(&mut self, message: &Value) -> Result<bool> {
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            return Ok(false);
        };
        if method == "experimental/serverStatus" {
            let health = message
                .pointer("/params/health")
                .and_then(Value::as_str)
                .context("server status health missing")?;
            ensure!(
                matches!(health, "ok" | "warning" | "error"),
                "invalid server status health"
            );
            self.server_status = Some((
                health.into(),
                message
                    .pointer("/params/quiescent")
                    .and_then(Value::as_bool)
                    .context("server status quiescence missing")?,
            ));
        }
        if let Some(id) = message.get("id") {
            let result = match method {
                "workspace/configuration" => Some(Value::Array(
                    message
                        .pointer("/params/items")
                        .and_then(Value::as_array)
                        .context("invalid LSP configuration request")?
                        .iter()
                        .map(|item| {
                            if item.get("section").and_then(Value::as_str) == Some("rust-analyzer")
                            {
                                initialization_options()
                            } else {
                                Value::Null
                            }
                        })
                        .collect(),
                )),
                "workspace/workspaceFolders" => {
                    Some(json!([{"uri":self.root_uri,"name":"airk-private-source"}]))
                }
                "window/workDoneProgress/create" => Some(Value::Null),
                "workspace/applyEdit" => Some(
                    json!({"applied":false,"failureReason":"AIRK is a read-only evidence client"}),
                ),
                _ => None,
            };
            if let Some(result) = result {
                self.send(json!({"jsonrpc":"2.0","id":id,"result":result}))?;
            } else {
                self.send(json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"AIRK does not permit this server request"}}))?;
            }
        }
        Ok(true)
    }
    fn wait_rust_analyzer(&mut self) -> Result<String> {
        loop {
            if let Some((health, true)) = &self.server_status {
                ensure!(
                    health != "error",
                    "Rust Analyzer reports an unusable project configuration"
                );
                return Ok(format!(
                    "rust-analyzer initialization quiescent; health={health}; Cargo offline and sysroot discovery disabled"
                ));
            }
            let message = self.receive()?;
            ensure!(
                self.service(&message)?,
                "unexpected response during LSP initialization"
            );
        }
    }
    fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        self.id += 1;
        ensure!(self.id <= 2048, "LSP request limit exceeded");
        let id = self.id;
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        loop {
            let message = self.receive()?;
            if self.service(&message)? {
                continue;
            }
            ensure!(
                message.get("id").and_then(Value::as_u64) == Some(id),
                "unexpected LSP response identifier"
            );
            ensure!(
                message.get("error").is_none(),
                "LSP {method} returned an error; no partial provider evidence published"
            );
            return message.get("result").cloned().context("LSP result missing");
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.process.take(); // terminate whole child group/job before joining I/O
        self.receiver.take();
        self.writer.close();
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

#[derive(Clone)]
struct Location {
    path: String,
    start: usize,
    end: usize,
    first: usize,
    last: usize,
    range: Value,
}
impl Location {
    fn id(&self, producer: &str) -> String {
        format!("lsp:{producer}:{}:{}:{}", self.path, self.start, self.end)
    }
    fn citation(&self, workspace: &Workspace) -> Result<Citation> {
        workspace.citation(&self.path, self.first, self.last)
    }
}

fn offset(text: &str, line: usize, column: usize, encoding: &str) -> Result<usize> {
    let mut base = 0;
    let mut lines = text.split('\n');
    for _ in 0..line {
        base += lines.next().context("LSP line outside source")?.len() + 1;
    }
    let value = lines.next().context("LSP line outside source")?;
    let value = value.strip_suffix('\r').unwrap_or(value);
    if encoding == "utf-8" {
        ensure!(
            column <= value.len() && value.is_char_boundary(column),
            "LSP coordinate splits UTF-8/source boundary"
        );
        return Ok(base + column);
    }
    let mut units = 0;
    for (index, character) in value.char_indices() {
        if units == column {
            return Ok(base + index);
        }
        units += if encoding == "utf-16" {
            character.len_utf16()
        } else {
            1
        };
        ensure!(units <= column, "LSP coordinate splits surrogate pair");
    }
    ensure!(units == column, "LSP character outside source");
    Ok(base + value.len())
}

fn location(workspace: &Workspace, path: &str, range: &Value, encoding: &str) -> Result<Location> {
    let coordinate = |pointer| -> Result<usize> {
        Ok(usize::try_from(
            range
                .pointer(pointer)
                .and_then(Value::as_u64)
                .context("LSP coordinate missing/noninteger")?,
        )?)
    };
    let (sl, sc, el, ec) = (
        coordinate("/start/line")?,
        coordinate("/start/character")?,
        coordinate("/end/line")?,
        coordinate("/end/character")?,
    );
    let text = workspace.text(path)?;
    let (start, end) = (
        offset(&text, sl, sc, encoding)?,
        offset(&text, el, ec, encoding)?,
    );
    ensure!(start < end, "LSP empty/reversed source range");
    Ok(Location {
        path: path.into(),
        start,
        end,
        first: sl + 1,
        last: el + usize::from(ec > 0),
        range: range.clone(),
    })
}

fn result_locations(
    workspace: &Workspace,
    result: &Value,
    encoding: &str,
) -> Result<Vec<Location>> {
    if result.is_null() {
        return Ok(Vec::new());
    }
    let values = if let Some(values) = result.as_array() {
        values.clone()
    } else {
        vec![result.clone()]
    };
    ensure!(values.len() <= MAX_RECORDS, "LSP location count exceeded");
    values
        .into_iter()
        .map(|value| {
            let uri = value
                .get("uri")
                .or_else(|| value.get("targetUri"))
                .and_then(Value::as_str)
                .context("LSP location URI missing")?;
            let path = workspace.path(uri)?;
            location(
                workspace,
                &path,
                value
                    .get("range")
                    .or_else(|| value.get("targetSelectionRange"))
                    .context("LSP location range missing")?,
                encoding,
            )
        })
        .collect()
}

#[derive(Clone)]
struct Symbol {
    name: String,
    location: Location,
    body: Location,
}

struct SymbolCollector<'a> {
    workspace: &'a Workspace,
    encoding: &'a str,
    output: &'a mut ProviderAnalysis,
    collected: &'a mut Vec<Symbol>,
}
impl SymbolCollector<'_> {
    fn visit(
        &mut self,
        path: &str,
        nodes: &Value,
        parent: Option<&str>,
        depth: usize,
    ) -> Result<()> {
        let workspace = self.workspace;
        let encoding = self.encoding;
        if nodes.is_null() {
            return Ok(());
        }
        ensure!(depth <= 32, "LSP symbol nesting limit exceeded");
        for node in nodes
            .as_array()
            .context("LSP documentSymbol result is not an array")?
        {
            ensure!(
                self.collected.len() < 256,
                "LSP selected documents exceed 256 symbols; narrow the selected paths"
            );
            let name = node
                .get("name")
                .and_then(Value::as_str)
                .context("LSP symbol name missing")?;
            ensure!(
                !name.is_empty() && name.len() <= 1024,
                "LSP symbol name outside bounds"
            );
            let (selection, body) = if node.get("location").is_some() {
                let selected = result_locations(workspace, &node["location"], encoding)?.remove(0);
                ensure!(
                    selected.path == path,
                    "documentSymbol returned a foreign document"
                );
                (selected.clone(), selected)
            } else {
                (
                    location(
                        workspace,
                        path,
                        node.get("selectionRange")
                            .context("LSP selectionRange missing")?,
                        encoding,
                    )?,
                    location(
                        workspace,
                        path,
                        node.get("range").context("LSP symbol range missing")?,
                        encoding,
                    )?,
                )
            };
            ensure!(
                body.start <= selection.start && selection.end <= body.end,
                "LSP symbol selection outside declaration"
            );
            let id = selection.id(&self.output.manifest.producer);
            let mut code = evidence(
                &self.output.manifest,
                id.clone(),
                name.into(),
                "LSP document symbol; hash-bound provider assertion, not AIRK type analysis",
            );
            code.span = Some(CodeSpan {
                start_byte: selection.start,
                end_byte: selection.end,
            });
            code.owner_id = parent.map(str::to_owned);
            if let Some(detail) = node.get("detail").and_then(Value::as_str) {
                code.signature = Some(crate::tables::bounded(detail, 1024));
                code.details_truncated = detail.len() > 1024;
            }
            self.output.facts.push(FactHit {
                continuity_key: None,
                name: name.into(),
                kind: "definition".into(),
                syntax_kind: format!(
                    "lsp-symbol/{}",
                    node.get("kind")
                        .and_then(Value::as_u64)
                        .context("LSP symbol kind missing")?
                ),
                language: workspace.artifacts[path].language.clone(),
                citation: selection.citation(workspace)?,
                document: None,
                code: Some(code),
            });
            self.collected.push(Symbol {
                name: name.into(),
                location: selection,
                body,
            });
            if let Some(children) = node.get("children") {
                self.visit(path, children, Some(&id), depth + 1)?;
            }
        }
        Ok(())
    }
}

fn supported(capabilities: &Value, key: &str) -> bool {
    capabilities
        .get(key)
        .is_some_and(|value| value == true || value.is_object())
}

fn edge(
    output: &mut ProviderAnalysis,
    workspace: &Workspace,
    source: String,
    target: String,
    kind: &str,
    at: &Location,
) -> Result<()> {
    ensure!(
        output.facts.len() + output.relationships.len() < MAX_RECORDS,
        "LSP record limit exceeded"
    );
    output.relationships.push(RelationshipHit {
        source,
        target,
        kind: kind.into(),
        resolution: "provider_asserted".into(),
        basis: EvidenceBasis::Imported,
        producer: output.manifest.producer.clone(),
        producer_version: output.manifest.producer_version.clone(),
        citation: at.citation(workspace)?,
    });
    Ok(())
}

fn hierarchy_item(workspace: &Workspace, item: &Value, encoding: &str) -> Result<Location> {
    let path = workspace.path(
        item.get("uri")
            .and_then(Value::as_str)
            .context("call hierarchy URI missing")?,
    )?;
    location(
        workspace,
        &path,
        item.get("selectionRange")
            .context("call hierarchy selection missing")?,
        encoding,
    )
}

fn initialization_options() -> Value {
    json!({"checkOnSave":false,"check":{"enable":false},"cargo":{"buildScripts":{"enable":false},"sysroot":null,"extraArgs":["--offline"]},"procMacro":{"enable":false}})
}

/// Query only advertised LSP capabilities. Results are scoped to selected indexed
/// documents and validated locations; references are never promoted to call edges.
pub fn run_lsp(
    root: &Path,
    snapshot: &Snapshot,
    source: &dyn ArtifactSource,
    request: &LiveRequest,
) -> Result<ProviderAnalysis> {
    let workspace = Workspace::prepare(root, snapshot, source, request)?;
    let mut session = Session::start(&workspace, request)?;
    let initialized = session.request("initialize", json!({"processId":std::process::id(),"clientInfo":{"name":"AIRK","version":"0.1.0"},"rootUri":session.root_uri,"workspaceFolders":[{"uri":session.root_uri,"name":"airk-private-source"}],"capabilities":{"experimental":{"serverStatusNotification":true},"general":{"positionEncodings":["utf-8","utf-16","utf-32"]},"workspace":{"applyEdit":false,"configuration":true},"textDocument":{"documentSymbol":{"hierarchicalDocumentSymbolSupport":true},"definition":{"linkSupport":true},"typeDefinition":{"linkSupport":true},"implementation":{"linkSupport":true},"references":{},"callHierarchy":{}}},"initializationOptions":initialization_options()}))?;
    let capabilities = initialized
        .get("capabilities")
        .context("LSP initialize capabilities missing")?;
    let encoding = capabilities
        .get("positionEncoding")
        .and_then(Value::as_str)
        .unwrap_or("utf-16");
    ensure!(
        matches!(encoding, "utf-8" | "utf-16" | "utf-32"),
        "unsupported LSP position encoding"
    );
    let mut output = workspace.empty("lsp-3.17/1");
    if let Some(info) = initialized.get("serverInfo") {
        let info = serde_json::to_string(info)?;
        ensure!(info.len() <= 1024, "LSP server identity exceeds 1024 bytes");
        output.coverage.push(format!("server_info={info}"));
    }
    ensure!(
        supported(capabilities, "documentSymbolProvider"),
        "LSP server does not advertise document symbols"
    );
    session.notify("initialized", json!({}))?;
    let mut collected = Vec::new();
    for path in &workspace.paths {
        let uri = url::Url::from_file_path(workspace.directory.path().join(path))
            .map_err(|_| anyhow::anyhow!("LSP document URI"))?
            .to_string();
        let language = match workspace.artifacts[path].language.as_str() {
            "tsx" => "typescriptreact",
            "csharp" => "csharp",
            other => other,
        };
        session.notify("textDocument/didOpen", json!({"textDocument":{"uri":uri,"languageId":language,"version":1,"text":workspace.text(path)?}}))?;
    }
    if initialized
        .pointer("/serverInfo/name")
        .and_then(Value::as_str)
        == Some("rust-analyzer")
    {
        output.coverage.push(session.wait_rust_analyzer()?);
    } else {
        output.coverage.push("server initialization readiness is not reported by a qualified adapter; results may be partial".into());
    }
    for path in &workspace.paths {
        let uri = url::Url::from_file_path(workspace.directory.path().join(path))
            .map_err(|_| anyhow::anyhow!("LSP document URI"))?
            .to_string();
        let result = session.request(
            "textDocument/documentSymbol",
            json!({"textDocument":{"uri":uri}}),
        )?;
        SymbolCollector {
            workspace: &workspace,
            encoding,
            output: &mut output,
            collected: &mut collected,
        }
        .visit(path, &result, None, 0)?;
    }
    for (capability, label) in [
        ("referencesProvider", "references"),
        ("definitionProvider", "definition"),
        ("typeDefinitionProvider", "type definition"),
        ("implementationProvider", "implementation"),
        ("callHierarchyProvider", "incoming/outgoing call hierarchy"),
    ] {
        output.coverage.push(format!(
            "{label}: {}",
            if supported(capabilities, capability) {
                "queried at document-symbol selection positions"
            } else {
                "not advertised; unknown"
            }
        ));
    }
    for symbol in &collected {
        let uri = url::Url::from_file_path(workspace.directory.path().join(&symbol.location.path))
            .map_err(|_| anyhow::anyhow!("LSP query URI"))?
            .to_string();
        let params = json!({"textDocument":{"uri":uri},"position":symbol.location.range["start"]});
        let id = symbol.location.id(&output.manifest.producer);
        if supported(capabilities, "referencesProvider") {
            let mut parameters = params.clone();
            parameters["context"] = json!({"includeDeclaration":false});
            for reference in result_locations(
                &workspace,
                &session.request("textDocument/references", parameters)?,
                encoding,
            )? {
                let owner = collected
                    .iter()
                    .filter(|owner| {
                        owner.body.path == reference.path
                            && owner.body.start <= reference.start
                            && reference.end <= owner.body.end
                    })
                    .min_by_key(|owner| owner.body.end - owner.body.start)
                    .map(|owner| owner.location.id(&output.manifest.producer))
                    .unwrap_or_else(|| format!("file:{}", reference.path));
                let mut code = evidence(
                    &output.manifest,
                    id.clone(),
                    symbol.name.clone(),
                    "LSP reference; provider-asserted target; not a call edge",
                );
                code.span = Some(CodeSpan {
                    start_byte: reference.start,
                    end_byte: reference.end,
                });
                code.owner_id = Some(owner.clone());
                output.facts.push(FactHit {
                    continuity_key: None,
                    name: symbol.name.clone(),
                    kind: "reference".into(),
                    syntax_kind: "lsp-reference".into(),
                    language: workspace.artifacts[&reference.path].language.clone(),
                    citation: reference.citation(&workspace)?,
                    document: None,
                    code: Some(code),
                });
                edge(
                    &mut output,
                    &workspace,
                    owner,
                    id.clone(),
                    "reference",
                    &reference,
                )?;
            }
        }
        for (capability, method, kind) in [
            (
                "definitionProvider",
                "textDocument/definition",
                "definition",
            ),
            (
                "typeDefinitionProvider",
                "textDocument/typeDefinition",
                "type_definition",
            ),
            (
                "implementationProvider",
                "textDocument/implementation",
                "implements",
            ),
        ] {
            if supported(capabilities, capability) {
                for target in result_locations(
                    &workspace,
                    &session.request(method, params.clone())?,
                    encoding,
                )? {
                    let target_id = target.id(&output.manifest.producer);
                    edge(
                        &mut output,
                        &workspace,
                        id.clone(),
                        target_id,
                        kind,
                        &symbol.location,
                    )?;
                }
            }
        }
        if supported(capabilities, "callHierarchyProvider") {
            let prepared = session.request("textDocument/prepareCallHierarchy", params)?;
            if prepared.is_null() {
                continue;
            }
            let items = prepared
                .as_array()
                .context("invalid LSP call hierarchy items")?;
            ensure!(items.len() <= 16, "excess LSP hierarchy items per symbol");
            for item in items {
                let selected = hierarchy_item(&workspace, item, encoding)?;
                for incoming in [true, false] {
                    let method = if incoming {
                        "callHierarchy/incomingCalls"
                    } else {
                        "callHierarchy/outgoingCalls"
                    };
                    let calls = session.request(method, json!({"item":item}))?;
                    if calls.is_null() {
                        continue;
                    }
                    for call in calls.as_array().context("invalid LSP calls response")? {
                        let other = hierarchy_item(
                            &workspace,
                            &call[if incoming { "from" } else { "to" }],
                            encoding,
                        )?;
                        let (caller, target) = if incoming {
                            (&other, &selected)
                        } else {
                            (&selected, &other)
                        };
                        for range in call
                            .get("fromRanges")
                            .and_then(Value::as_array)
                            .context("LSP call ranges missing")?
                        {
                            let at = location(&workspace, &caller.path, range, encoding)?;
                            let source = caller.id(&output.manifest.producer);
                            let target = target.id(&output.manifest.producer);
                            edge(&mut output, &workspace, source, target, "call", &at)?;
                        }
                    }
                }
            }
        }
    }
    session.request("shutdown", Value::Null)?;
    session.notify("exit", Value::Null)?;
    drop(session);
    workspace.finish(output, root, snapshot, request)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lsp_framing_and_unicode_positions_fail_closed() -> Result<()> {
        let json = br#"{"jsonrpc":"2.0","id":1,"result":null}"#;
        let mut bytes = format!("Content-Length: {}\r\n\r\n", json.len()).into_bytes();
        bytes.extend(json);
        assert!(read_frame(&mut std::io::Cursor::new(bytes))?.0["result"].is_null());
        for invalid in [
            b"Content-Length: 9\r\nContent-Length: 9\r\n\r\n{}".as_slice(),
            b"Content-Length: 99999999\r\n\r\n",
            b"Content-Length: 2\n\n{}",
        ] {
            assert!(read_frame(&mut std::io::Cursor::new(invalid)).is_err());
        }
        assert_eq!(offset("🚀 name\r\nnext", 0, 3, "utf-16")?, 5);
        assert_eq!(offset("🚀 name\r\nnext", 0, 2, "utf-32")?, 5);
        assert!(offset("🚀 name", 0, 1, "utf-16").is_err());
        assert!(offset("🚀 name", 0, 2, "utf-8").is_err());
        Ok(())
    }
}
