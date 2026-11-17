# SPDX-License-Identifier: Apache-2.0
"""Inert P11a transport delta; apply after corrected Core/Kernel report proposal."""
import difflib,hashlib,json,subprocess
from pathlib import Path
base=Path(__file__).resolve().parent; root=base/'p10-worktree'; changes={}
def read(name):
    value=(root/name).read_text(encoding='utf-8');changes[name]=[value,value];return value
def replace(value,old,new):
    assert value.count(old)==1,old
    return value.replace(old,new)
name='crates/airk-core/src/native_reads.rs';text=read(name)
text+='''
/// Shared input contract for the existing canonical FileInfo tool route. NativeRead
/// remains the settings/admin channel; there is no parallel FileInfo endpoint.
pub const FILE_INFO_PROFILE_FAMILIES_HELP: &str = "Include eighteen metadata-only effective profile family observations for this file. Collection policy is unavailable under partial Project scopes. Reporting adds no source reads or profile migration; existing verification remains independent.";
pub fn file_info_profile_families_schema()->serde_json::Value {
    serde_json::json!({"type":"boolean","default":false,"description":FILE_INFO_PROFILE_FAMILIES_HELP})
}
''';changes[name][1]=text
name='crates/airk-mcp/src/lib.rs';text=read(name)
needle='''pub struct PathArgs {
'''
text=replace(text,needle,needle+'''    #[serde(default)]
    #[schemars(schema_with="file_info_profile_families_schema")]
    pub include_profile_families: bool,
''')
needle='''#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PathArgs'''
text=replace(text,needle,'''fn file_info_profile_families_schema(_: &mut schemars::SchemaGenerator)->schemars::Schema {
    serde_json::from_value(airk_core::native_reads::file_info_profile_families_schema()).expect("static file-info family input")
}

'''+needle)
text=replace(text,'move |kernel| kernel.file_info_conditional(&args.path, &args.evidence.options()?),','move |kernel| kernel.file_info_conditional_with_profile_families(&args.path, &args.evidence.options()?,args.include_profile_families),')
needle='''            "conditional_reads": {"tools":["airk_file_info","airk_read_file","airk_outline","airk_context"],"validator":"representation-specific if_evidence","follow_moves_default":false,"move_basis":"unique compatible exact content; not filesystem intent"},'''
text=replace(text,needle,needle+'''
            "file_info_profile_families":{"input":"include_profile_families","default":false,"schema":"airk.profile-families/1","families":airk_core::profile_families::Family::ALL,"basis":"metadata_only","retention":"policy eligibility, not observed presence or completeness","collection_policy":"typed unavailable under partial Project scope","configured_file_profile":"unknown when fresh source/header selection would be required","live_source_checked":false},''')
changes[name][1]=text
name='crates/airk-cli/src/main.rs';text=read(name)
needle='''    Info {
'''
text=replace(text,needle,needle+'''        #[arg(long,help=airk_core::native_reads::FILE_INFO_PROFILE_FAMILIES_HELP)]
        include_profile_families: bool,
''')
needle='''        Command::Info {
            path,
            evidence: options,
            ..
        } => ("airk_file_info", evidence(json!({"path":path}), options)?),'''
text=replace(text,needle,'''        Command::Info {
            path,
            include_profile_families,
            evidence: options,
            ..
        } => {
            let mut arguments=json!({"path":path});
            if include_profile_families {arguments["include_profile_families"]=json!(true);}
            ("airk_file_info",evidence(arguments,options)?)
        },''')
test='''
    #[test]
    fn info_family_flag_forwards_only_explicit_opt_in()->Result<()> {
        for enabled in [false,true] {
            let mut arguments=vec!["airk","info","api/policy.md"];
            if enabled {arguments.push("--include-profile-families");}
            let cli=Cli::try_parse_from(arguments)?;
            let (tool,args)=query(cli.command)?;
            assert_eq!(tool,"airk_file_info");
            if enabled {assert_eq!(args["include_profile_families"],true);}else{assert!(args.get("include_profile_families").is_none());}
        }
        Ok(())
    }
'''
assert text.rstrip().endswith('}');text=text.rstrip()[:-1]+test+'}\n';changes[name][1]=text
name='crates/airk-kernel/src/workbench.rs';text=read(name)
needle='''        "file_info" => &[
''';text=replace(text,needle,needle+'            "include_profile_families",\n')
needle='''fn argument_schema(key: &str) -> Value {
    match key {''';text=replace(text,needle,needle+'''
        "include_profile_families"=>airk_core::native_reads::file_info_profile_families_schema(),''')
needle='''            "include_count"
                | "include_files"'''
text=replace(text,needle,'''            "include_count"
                | "include_profile_families"
                | "include_files"''')
needle='''                serde_json::to_value(self.file_info_conditional(string(args, "path")?, &evidence)?)?'''
text=replace(text,needle,'''                serde_json::to_value(self.file_info_conditional_with_profile_families(string(args,"path")?,&evidence,args.get("include_profile_families").and_then(Value::as_bool).unwrap_or(false))?)?''');changes[name][1]=text
name='crates/airk-mcp/src/response.rs';text=read(name)
test='''
    #[test]
    fn conditional_file_family_rows_and_unknown_fields_survive_display_modes() {
        let report=serde_json::json!({"schema":"airk.profile-families/1","families":airk_core::profile_families::Family::ALL.map(|family|serde_json::json!({"family":family,"support":"unknown","retention":"selected","reply_inclusion":"omitted"})),"future_native_field":{"complete":true}});
        for mode in [ResponseMode::Terse,ResponseMode::Full] {
            for detail in [None,Some(airk_core::reply::ReplyDetail::Lean),Some(airk_core::reply::ReplyDetail::Rich)] {
                let mut value=serde_json::json!({"read":{"status":"content"},"content":{"path":"api/policy.md","profile_families":report}});
                airk_core::reply::ReplyDetail::annotate(&mut value,detail);
                Session::default().display(&mut value,mode,false);
                assert_eq!(value["content"]["profile_families"],report);
            }
        }
    }
'''
assert text.rstrip().endswith('}');text=text.rstrip()[:-1]+test+'}\n';changes[name][1]=text
name='crates/airk-cli/tests/combined_reads.rs';text=read(name)
text+='''
#[path="combined_reads/profile_families.rs"]
mod profile_families;
''';changes[name][1]=text
native_test='''// SPDX-License-Identifier: Apache-2.0
//! Real scoped workers, native frontend, CLI call/console/info, and stdio MCP.
use super::*;

async fn cli_catalog(f:&Fixture)->Result<Value> {
    let mut command=Command::new(&f.candidate);
    command.arg("--combined-session").arg(serde_json::to_string(&f.session)?).arg("--service-runtime-dir").arg(&f.control).args(["tools","list","--json"]);
    let output=tokio::task::spawn_blocking(move||read_retry::output(&mut command)).await??;
    ensure!(output.status.success(),"native CLI catalog failed: {}",String::from_utf8_lossy(&output.stderr));
    Ok(serde_json::from_slice(&output.stdout)?)
}
async fn console(f:&Fixture,args:&Value)->Result<Value> {
    let input=f.owner.path().join("profile-families-console.in");
    let stdout=f.owner.path().join("profile-families-console.out");
    let stderr=f.owner.path().join("profile-families-console.err");
    fs::write(&input,format!("call airk_file_info {args}\\nexit\\n"))?;
    let mut command=Command::new(&f.candidate);
    command.arg("--combined-session").arg(serde_json::to_string(&f.session)?).arg("--service-runtime-dir").arg(&f.control).args(["tools","shell"])
        .stdin(fs::File::open(input)?).stdout(fs::OpenOptions::new().create_new(true).write(true).open(&stdout)?).stderr(fs::OpenOptions::new().create_new(true).write(true).open(&stderr)?);
    let mut process=OwnedProcess::start(command)?;
    let deadline=Instant::now()+Duration::from_secs(60);
    loop {if let Some(status)=process.child.try_wait()? {ensure!(status.success(),"console failed");break;}
        ensure!(Instant::now()<deadline,"native console timeout");tokio::time::sleep(Duration::from_millis(25)).await;}
    drop(process);
    ensure!(fs::metadata(&stdout)?.len()<=16<<20 && fs::metadata(&stderr)?.len()<=16<<20,"console diagnostics exceed bound");
    ensure!(fs::read(&stderr)?.is_empty(),"console tool failed: {}",fs::read_to_string(&stderr)?);
    let text=fs::read_to_string(stdout)?;
    let offset=text.find("airk> {").context("console result absent")?+"airk> ".len();
    serde_json::Deserializer::from_str(&text[offset..]).into_iter::<Value>().next().context("console JSON result absent")?.map_err(Into::into)
}
async fn convenience(f:&Fixture)->Result<Value> {
    let mut command=Command::new(&f.candidate);
    command.arg("--combined-session").arg(serde_json::to_string(&f.session)?).arg("--service-runtime-dir").arg(&f.control)
        .arg("--project-ids").arg(f.projects[0].to_string()).arg("--index-ids").arg(f.indexes[0].to_string())
        .args(["--member-availability","require","info","api/policy.md","--include-profile-families","--verification","off","--reply-detail","rich"])
        .env("AIRK_MCP_RESPONSE_MODE","full");
    let output=tokio::task::spawn_blocking(move||read_retry::output(&mut command)).await??;
    ensure!(output.status.success(),"native convenience info failed: {}",String::from_utf8_lossy(&output.stderr));
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[tokio::test]
async fn opted_file_family_report_keeps_full_native_cli_console_mcp_and_input_catalog_parity()->Result<()> {
    let mut f=Fixture::new(true)?;f.session.project_ids=f.projects.clone();let _service=f.start().await?;
    let expected_schema=airk_core::native_reads::file_info_profile_families_schema();
    let catalogue=cli_catalog(&f).await?;
    let cli_info=catalogue["tools"].as_array().context("CLI tool catalog absent")?.iter().find(|tool|tool["name"]=="airk_file_info").context("CLI FileInfo absent")?;
    assert_eq!(cli_info["inputSchema"]["properties"]["include_profile_families"],expected_schema);
    assert_eq!(airk_kernel::workbench::operation_catalog()["operations"]["file_info"]["properties"]["include_profile_families"],expected_schema);
    let mut report_identity=None;
    for mode in ["full","terse"] {
        for detail in [None,Some("lean"),Some("rich")] {
            // Fresh response sessions avoid sharing omission observations across
            // transports; actual payload/provenance/scope fields remain intact.
            let backend=airk_cli::combined_frontend::resolve(None,Some(&f.control),&f.session).await?;
            let mut mcp=McpSession::start(&f)?;
            let catalog=mcp.request(2,"tools/list",json!({}))?;
            let public=catalog["result"]["tools"].as_array().context("MCP tool catalog absent")?.iter().find(|tool|tool["name"]=="file_info").context("MCP FileInfo absent")?;
            let native=serde_json::to_value(backend.catalog().into_iter().find(|tool|tool.name=="airk_file_info").context("native FileInfo absent")?)?;
            assert_eq!(public["inputSchema"]["properties"]["include_profile_families"],expected_schema);
            assert_eq!(native["inputSchema"]["properties"]["include_profile_families"],expected_schema);
            assert_eq!(cli_info["inputSchema"],native["inputSchema"]);
            let native_keys=native["inputSchema"]["properties"].as_object().unwrap().keys().collect::<std::collections::BTreeSet<_>>();
            let public_keys=public["inputSchema"]["properties"].as_object().unwrap().keys().collect::<std::collections::BTreeSet<_>>();
            assert_eq!(native_keys,public_keys,"aliases must expose the same complete input inventory");
            let mut arguments=json!({"path":"api/policy.md","include_profile_families":true,"project_ids":[f.projects[0]],"index_ids":[f.indexes[0]],"member_availability":"require","verification":"off","response_mode":mode});
            if let Some(detail)=detail {arguments["reply_detail"]=json!(detail);}
            let native=native_read(backend.as_ref(),"airk_file_info",&arguments).await?;
            let cli=f.cli("airk_file_info",&arguments).await?;
            let wire=mcp.call(3,"file_info",arguments.clone())?;
            let native=body(&native)?;let cli=body(&cli)?;let wire=body(&wire["result"])?;
            assert_parity("complete opted file-info CLI result",&native,&cli)?;
            assert_parity("complete opted file-info MCP alias result",&native,&wire)?;
            let member=native["members"].as_array().and_then(|members|members.first()).context("positive selected member absent")?;
            assert_eq!(member["outcome"],"complete");assert_eq!(member["is_error"],false);
            let result=member_body(&native,member)?;
            let report=&result["content"]["profile_families"];
            assert_eq!(report["families"].as_array().context("family rows absent")?.len(),18);
            assert_eq!(report["configured_collection_policy"]["state"],"unavailable");
            assert_eq!(report["configured_collection_policy"]["reason"],"selected_scope");
            assert_eq!(report["published_collection_policy"]["state"],"unavailable");
            assert!(report["pending_collection_reindex"].is_null());
            assert_eq!(report["file"]["publication"],result["content"]["publication"]["token"]);
            assert_eq!(report["file"]["path"],"api/policy.md");assert_eq!(report["live_source_checked"],false);
            if let Some(expected)=&report_identity {assert_eq!(report,expected);}else{report_identity=Some(report.clone());}
            if mode=="full" && detail==Some("rich") {
                let console=console(&f,&arguments).await?;
                assert_parity("complete actual tools console file-info",&native,&body(&console)?)?;
                assert_parity("complete convenience info family report",&native,&body(&convenience(&f).await?)?)?;
            }
            drop(mcp);backend.close();backend.shutdown().await?;
        }
    }
    // Absent/false preserves the exact legacy conditional representation key.
    let backend=airk_cli::combined_frontend::resolve(None,Some(&f.control),&f.session).await?;
    let base_args=json!({"path":"api/policy.md","project_ids":[f.projects[0]],"index_ids":[f.indexes[0]],"member_availability":"require","verification":"off","response_mode":"full"});
    let legacy=body(&native_read(backend.as_ref(),"airk_file_info",&base_args).await?)?;
    let mut explicit_false=base_args.clone();explicit_false["include_profile_families"]=json!(false);
    let explicit_false=body(&native_read(backend.as_ref(),"airk_file_info",&explicit_false).await?)?;
    let legacy=member_body(&legacy,&legacy["members"][0])?;let explicit_false=member_body(&explicit_false,&explicit_false["members"][0])?;
    assert!(legacy["content"].get("profile_families").is_none());
    assert!(explicit_false["content"].get("profile_families").is_none());
    assert_eq!(legacy["read"]["validator"],explicit_false["read"]["validator"]);
    let mut conditional=base_args.clone();conditional["include_profile_families"]=json!(true);conditional["if_evidence"]=legacy["read"]["validator"].clone();
    let mut conditional_mcp=McpSession::start(&f)?;
    let native=body(&native_read(backend.as_ref(),"airk_file_info",&conditional).await?)?;
    let cli=body(&f.cli("airk_file_info",&conditional).await?)?;
    let wire=conditional_mcp.call(3,"file_info",conditional.clone())?;
    assert_parity("opt-in conditional CLI representation",&native,&cli)?;
    assert_parity("opt-in conditional MCP representation",&native,&body(&wire["result"])?)?;
    let result=member_body(&native,&native["members"][0])?;
    assert_eq!(result["read"]["status"],"content","legacy validator suppressed the requested family representation");
    conditional["if_evidence"]=result["read"]["validator"].clone();
    let native=body(&native_read(backend.as_ref(),"airk_file_info",&conditional).await?)?;
    let cli=body(&f.cli("airk_file_info",&conditional).await?)?;
    let wire=conditional_mcp.call(4,"file_info",conditional)?;
    assert_parity("unchanged conditional CLI representation",&native,&cli)?;
    assert_parity("unchanged conditional MCP representation",&native,&body(&wire["result"])?)?;
    let result=member_body(&native,&native["members"][0])?;
    assert_eq!(result["read"]["status"],"not_modified");
    drop(conditional_mcp);
    backend.close();backend.shutdown().await?;
    // The full-root participant transports the known configured/published
    // policy observations as well; selected-scope unknowns must not become a
    // permanently reduced response contract for every native caller.
    let backend=airk_cli::combined_frontend::resolve(None,Some(&f.control),&f.session).await?;
    let mut mcp=McpSession::start(&f)?;
    let full_args=json!({"path":"api/policy.md","include_profile_families":true,"project_ids":[f.projects[2]],"index_ids":[f.indexes[2]],"member_availability":"require","verification":"off","response_mode":"full","reply_detail":"rich"});
    let native=body(&native_read(backend.as_ref(),"airk_file_info",&full_args).await?)?;
    let cli=body(&f.cli("airk_file_info",&full_args).await?)?;
    let wire=mcp.call(3,"file_info",full_args.clone())?;
    assert_parity("known collection policy CLI FileInfo",&native,&cli)?;
    assert_parity("known collection policy MCP FileInfo",&native,&body(&wire["result"])?)?;
    let result=member_body(&native,&native["members"][0])?;
    assert_eq!(result["content"]["profile_families"]["configured_collection_policy"]["state"],"known");
    assert_eq!(result["content"]["profile_families"]["published_collection_policy"]["state"],"known");
    let mut invalid=full_args;invalid["include_profile_families"]=json!("true");
    let refused=mcp.call(4,"file_info",invalid)?;
    assert!(refused.get("error").is_some() || refused["result"]["isError"]==true,"nonboolean opt-in accepted: {refused}");
    drop(mcp);backend.close();backend.shutdown().await?;
    f.assert_sources_unchanged()?;
    Ok(())
}
'''
changes['crates/airk-cli/tests/combined_reads/profile_families.rs']=['',native_test]
scratch=base/'p11a-transport-report-formatted';scratch.mkdir(exist_ok=True);copies=[]
for name,(_old,new) in changes.items():
    copy=scratch/(name.split('/')[1]+'-'+Path(name).name);copy.write_text(new,encoding='utf-8');copies.append(copy)
options=[str(Path.home()/'.cargo/bin/rustfmt.exe'),'--edition','2024','--config','skip_children=true']
subprocess.run([*options,*map(str,copies)],check=True,cwd=root);subprocess.run([*options,'--check',*map(str,copies)],check=True,cwd=root)
for (name,pair),copy in zip(changes.items(),copies):pair[1]=copy.read_text(encoding='utf-8')
patch=''.join(''.join(difflib.unified_diff(old.splitlines(keepends=True),new.splitlines(keepends=True),fromfile='a/'+name if old else '/dev/null',tofile='b/'+name)) for name,(old,new) in changes.items())
(base/'p11a-transport-report-review.patch').write_text(patch,encoding='utf-8')
(base/'p11a-transport-report-review-base.json').write_text(json.dumps({name:hashlib.sha256((root/name).read_bytes()).hexdigest() if old else None for name,(old,_new) in changes.items()},indent=2)+'\n',encoding='utf-8')
print('Prepared inert P11a transport delta:',len(changes),'files; depends on corrected report proposal; no compiler inputs changed')
