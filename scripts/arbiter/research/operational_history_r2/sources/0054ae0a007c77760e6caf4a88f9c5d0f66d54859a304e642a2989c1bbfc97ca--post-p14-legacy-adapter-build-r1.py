# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,difflib
A=Path(__file__).parent; W=A/'p10-worktree'; O=A/'post-p14-legacy-adapters-proposed-r4'
(O/'source').mkdir(parents=True,exist_ok=True); (O/'base').mkdir(exist_ok=True)
files={}
def put(p,s):
 b=(W/p).read_bytes() if (W/p).exists() else b''
 if b'\r\n' in b:s=s.replace('\r\n','\n').replace('\n','\r\n')
 q=O/'base'/p;q.parent.mkdir(parents=True,exist_ok=True);q.write_bytes(b)
 q=O/'source'/p;q.parent.mkdir(parents=True,exist_ok=True);q.write_bytes(s.encode());files[p]=(b,s.encode())
def change(p,a,b):
 s=files[p][1].decode() if p in files else (W/p).read_text(encoding='utf-8'); assert a in s,p;put(p,s.replace(a,b,1))
put('crates/airk-cli/src/runtime_legacy_retirement.rs',r'''// SPDX-License-Identifier: Apache-2.0
//! Explicit local runtime administration; never an evidence or MCP write route.
use super::*;
use airk_core::{catalog_upgrade_recovery::{UpgradeRetirementPreview, UpgradeRetirementReply,
    UpgradeRetirementRequest}, identity::Sha256Digest, index_catalog::{IndexId, SourceImportState}};
use airk_store::service::registry::catalog::upgrade::{self, CatalogUpgrade};
use std::io::Read;

fn input<T: serde::de::DeserializeOwned>() -> Result<T> {
    let mut bytes = Vec::new();
    std::io::stdin().take(airk_core::service::MAX_FRAME_BYTES as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= airk_core::service::MAX_FRAME_BYTES, "legacy retirement input exceeds bound");
    Ok(serde_json::from_slice(&bytes)?)
}
fn installer(root: &Path) -> Result<(PathBuf, Sha256Digest)> {
    let identity = runtime_pair::engine_identity()?;
    let control = layout::catalog_control(root, identity)?;
    Ok((control, identity))
}
fn observation(root: &Path, request: &UpgradeRetirementRequest) -> Result<Option<UpgradeRetirementReply>> {
    request.validate()?;
    ensure!(request.caller == crate::local_ipc::caller_identity()?, "legacy retirement caller differs from authenticated native caller");
    let control = layout::saved_control(root)?;
    let saved = upgrade::pending(&control)?.context("retained legacy upgrade journal unavailable")?;
    ensure!(saved.plan.id == request.upgrade_id && saved.plan_digest == request.plan_digest
        && saved.plan.installer_identity == request.installer_identity, "retirement plan differs");
    layout::validate_catalog_control(&control, saved.plan.namespace)?;
    let reply = upgrade::recovery::legacy_retirement_observation(&control, request)?;
    if let Some(reply) = &reply { reply.validate()?; }
    Ok(reply)
}
pub(super) fn candidates(root: &Path, after: Option<IndexId>) -> Result<serde_json::Value> {
    let _lock = existing_install_lock(root)?;
    let control = layout::saved_control(root)?;
    let saved = upgrade::pending(&control)?.context("no pending legacy upgrade")?;
    layout::validate_catalog_control(&control, saved.plan.namespace)?;
    let mut eligible = saved.plan.indexes.iter()
        .filter(|index| index.source_import == SourceImportState::PendingMetadata).collect::<Vec<_>>();
    eligible.sort_by_key(|index| index.index_id);
    let start = if let Some(after) = after {
        eligible.iter().position(|index| index.index_id == after)
            .context("legacy retirement continuation differs from saved candidates")? + 1
    } else { 0 };
    let mut items = Vec::new();
    let page = |items: &Vec<serde_json::Value>| {
        let continuation = if start + items.len() < eligible.len() && !items.is_empty() {
            Some(eligible[start + items.len() - 1].index_id)
        } else { None };
        serde_json::json!({"schema":1,"upgrade_id":saved.plan.id,"plan_digest":saved.plan_digest,
            "installer_identity":saved.plan.installer_identity,"namespace":saved.plan.namespace,
            "after":after,"items":items,"continuation":continuation,
            "subset":after.is_some() || continuation.is_some(),"total_eligible":eligible.len()})
    };
    for index in eligible.iter().skip(start).take(16) {
        let authorization = saved.retirement_recovery.as_ref().and_then(|journal|
            journal.authorizations.iter().find(|authorization| authorization.preview.request.index_id == index.index_id));
        let observed = if let Some(authorization) = authorization {
            Some(observation(root, &authorization.preview.request)?
                .context("saved retirement observation unavailable")?)
        } else { None };
        items.push(serde_json::json!({"index_id":index.index_id,"registration":index.registration,
            "source_import":index.source_import,"observation":observed}));
        if serde_json::to_vec(&page(&items))?.len() > airk_core::service::MAX_FRAME_BYTES {
            items.pop();
            ensure!(!items.is_empty(), "one legacy retirement candidate exceeds reply budget");
            break;
        }
    }
    let value = page(&items);
    ensure!(serde_json::to_vec(&value)?.len() <= airk_core::service::MAX_FRAME_BYTES,
        "legacy candidate list exceeds reply budget");
    Ok(value)
}
pub(super) fn review(root: &Path, index: IndexId) -> Result<UpgradeRetirementReply> {
    let _lock = existing_install_lock(root)?;
    let (control, identity) = installer(root)?;
    let owner = CatalogUpgrade::resume_for_installer(&control, identity)?;
    let request = owner.prepare_legacy_retirement_request(index, crate::local_ipc::caller_identity()?,
        airk_kernel::service_broker::new_instance_id()?)?;
    let preview = owner.preview_legacy_retirement(&request)?;
    let reply = UpgradeRetirementReply::Review { review_digest: preview.identity()?, preview };
    reply.validate()?;
    Ok(reply)
}
pub(super) fn authorize(root: &Path) -> Result<UpgradeRetirementReply> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Reviewed { preview: UpgradeRetirementPreview, review_digest: Sha256Digest }
    let reviewed: Reviewed = input()?;
    reviewed.preview.validate()?;
    ensure!(reviewed.preview.identity()? == reviewed.review_digest, "legacy review changed");
    let request = reviewed.preview.request;
    ensure!(request.caller == crate::local_ipc::caller_identity()?, "legacy retirement caller differs from authenticated native caller");
    let _lock = existing_install_lock(root)?;
    let (control, identity) = installer(root)?;
    ensure!(identity == request.installer_identity, "crossed retirement candidate");
    activation::legacy_retirement_candidate(root, &control, identity)?;
    let owner = CatalogUpgrade::resume_for_installer(&control, identity)?;
    let authorization = owner.authorize_legacy_retirement(&request, reviewed.review_digest)?;
    let reply = UpgradeRetirementReply::Authorized { authorization };
    reply.validate()?;
    Ok(reply) // No activation here. A separately confirmed Resume releases/reacquires owners.
}
pub(super) fn receipt(root: &Path) -> Result<serde_json::Value> {
    let request: UpgradeRetirementRequest = input()?;
    // Exact durable lookup only: no owner acquisition, source/data open, resume or replay.
    Ok(serde_json::json!({"schema":1,"observation":observation(root, &request)?}))
}
pub(super) async fn resume(root: &Path) -> Result<UpgradeRetirementReply> {
    let request: UpgradeRetirementRequest = input()?;
    ensure!(runtime_pair::engine_identity()? == request.installer_identity,
        "legacy retirement resume requires the originally admitted candidate");
    match observation(root, &request)? {
        Some(UpgradeRetirementReply::Applied { .. }) => (),
        Some(UpgradeRetirementReply::Authorized { .. }) => (),
        _ => bail!("no exact accepted retirement to resume; authorization is never replayed"),
    }
    let candidate = {
        let _lock = existing_install_lock(root)?;
        let Some(pending) = activation::pending(root)? else {
            return observation(root, &request)?.context("accepted retirement outcome unavailable");
        };
        let _ = pending;
        let control = layout::saved_control(root)?;
        activation::legacy_retirement_candidate(root, &control, request.installer_identity)?
    };
    activate(root, &candidate).await?;
    observation(root, &request)?.context("accepted retirement outcome remains unavailable")
}
''')
change('crates/airk-cli/src/runtime_activation.rs','/// Resume under the same installer ownership, preserving the public selection.',r'''/// Read-only binding for explicit legacy retirement before its authorization effect.
pub(super) fn legacy_retirement_candidate(
    root: &Path,
    control: &Path,
    installer: airk_core::identity::Sha256Digest,
) -> Result<String> {
    let intent = pending(root)?.context("legacy activation intent missing")?;
    intent.validate(root)?;
    ensure!(intent.phase == Phase::Probe && intent.control == control
        && (selection(root)? == Some(intent.selected())
            || (intent.prior.is_none() && selection(root)?.is_none())),
        "legacy retirement requires its exact saved installer activation");
    let (main, _) = verify_runtime(root, &intent.candidate)?;
    ensure!(runtime_pair::engine_identity_at(&main)? == installer,
        "pending legacy activation candidate changed");
    Ok(intent.candidate)
}

/// Resume under the same installer ownership, preserving the public selection.''')
change('crates/airk-cli/src/runtime_install.rs','#[path = "runtime_retirement.rs"]','#[path = "runtime_legacy_retirement.rs"]\nmod legacy_retirement;\n#[path = "runtime_retirement.rs"]')
change('crates/airk-cli/src/runtime_install.rs','    /// Bounded executable/paired-helper self-test,', '''    /// Review saved unavailable legacy indexes. No source/data reads or activation.
    LegacyRetirementCandidates { #[arg(long)] install_dir: PathBuf,
        #[arg(long)] after: Option<airk_core::index_catalog::IndexId> },
    /// Create one exact retirement review from a saved upgrade index identity.
    LegacyRetirementReview { #[arg(long)] install_dir: PathBuf,
        #[arg(long)] index_id: airk_core::index_catalog::IndexId },
    /// Explicitly authorize exact reviewed stdin JSON; activation requires separate Resume.
    LegacyRetirementAuthorize { #[arg(long)] install_dir: PathBuf },
    /// Read only the exact saved request on stdin; never authorize/replay/resume.
    LegacyRetirementReceipt { #[arg(long)] install_dir: PathBuf },
    /// Explicit continuation of an already accepted exact request on stdin.
    LegacyRetirementResume { #[arg(long)] install_dir: PathBuf },
    /// Bounded executable/paired-helper self-test,''')
change('crates/airk-cli/src/runtime_install_commands.rs','    let output = match command {','''    let output = match command {
        RuntimeCommand::LegacyRetirementCandidates { install_dir, after } => legacy_retirement::candidates(install_dir, *after)?,
        RuntimeCommand::LegacyRetirementReview { install_dir, index_id } => serde_json::to_value(legacy_retirement::review(install_dir, *index_id)?)?,
        RuntimeCommand::LegacyRetirementAuthorize { install_dir } => serde_json::to_value(legacy_retirement::authorize(install_dir)?)?,
        RuntimeCommand::LegacyRetirementReceipt { install_dir } => legacy_retirement::receipt(install_dir)?,
        RuntimeCommand::LegacyRetirementResume { install_dir } => serde_json::to_value(legacy_retirement::resume(install_dir).await?)?,''')
put('packages/vscode/src/runtimeLegacyRetirement.ts',r'''// SPDX-License-Identifier: Apache-2.0
// Trusted native runtime administration. In-memory only; never a saved evidence session.
import { ServiceError, record, RecordValue, identifier } from './serviceClient';
import { catalogId } from './indexingCatalog';
import { isDeepStrictEqual } from 'node:util';
const LIMIT=64*1024;
export interface LegacyRetirementHost {
    legacyRetirementCandidates(after?:string):Promise<RecordValue>;
    legacyRetirementReview(index:string):Promise<RecordValue>;
    legacyRetirementAuthorize(review:RecordValue,admit:()=>void):Promise<RecordValue>;
    legacyRetirementReceipt(request:RecordValue):Promise<RecordValue|null>;
    legacyRetirementResume(request:RecordValue,admit:()=>void):Promise<RecordValue>;
}
function bounded(value:unknown):RecordValue {
    if(Buffer.byteLength(JSON.stringify(value))>LIMIT)throw new ServiceError('legacy_retirement_reply_oversize');
    return record(value);
}
function nonnegative(value:unknown):number{
    if(typeof value!=='number'||!Number.isSafeInteger(value)||value<0)throw new ServiceError('invalid_legacy_retirement_count');return value;
}
export function validateRetirementReply(value:unknown,expected?:RecordValue):RecordValue {
    const reply=bounded(value);
    if(!['review','authorized','applied'].includes(String(reply.status)))throw new ServiceError('invalid_legacy_retirement_reply');
    const auth=reply.status==='applied'?record(record(reply.receipt).authorization):reply.status==='authorized'?record(reply.authorization):reply;
    const preview=record(auth.preview),request=record(preview.request);
    if(preview.schema!==1||request.schema!==1||preview.source_configuration!=='pending_metadata'
        ||preview.effect!=='retire_and_cancel_with_cleanup_pending')throw new ServiceError('invalid_legacy_retirement_review');
    identifier(auth.review_digest,32);identifier(preview.request_digest,32);catalogId(request.index_id,'idx_');
    const key=record(request.key),registration=record(request.registration),work=record(preview.affected_work);
    identifier(request.caller,16);identifier(request.upgrade_id,16);identifier(key.namespace,16);identifier(key.nonce,16);nonnegative(key.epoch);
    for(const value of [request.plan_digest,request.installer_identity,request.expected_authorizations_digest,
        preview.original_backup_digest,preview.original_legacy_data_digest,work.job_ids_digest])identifier(value,32);
    if(nonnegative(work.jobs)>1024)throw new ServiceError('invalid_legacy_retirement_work');
    identifier(registration.binding,32);
    for(const value of [registration.root,registration.data_root]){
        if(typeof value!=='string'||!value.length||value.length>4096||/[\u0000-\u001f\u007f]/.test(value))throw new ServiceError('invalid_legacy_retirement_registration');
    }
    if(typeof registration.collection!=='string'||!registration.collection.length||registration.collection.length>128)throw new ServiceError('invalid_legacy_retirement_registration');
    if(expected&&!isDeepStrictEqual(request,expected))throw new ServiceError('crossed_legacy_retirement_request');
    if(reply.status==='applied'){
        const receipt=record(reply.receipt);
        if(receipt.schema!==1||receipt.cleanup!=='pending'||nonnegative(receipt.catalog_revision)<1
            ||!isDeepStrictEqual(receipt.canceled_work,work))throw new ServiceError('invalid_legacy_retirement_cleanup');
        identifier(receipt.post_effect_legacy_data_digest,32);
    }
    return reply; // Full original body, including future native fields, is retained and exported.
}
export function retirementRequest(reply:RecordValue):RecordValue {
    const auth=reply.status==='applied'?record(record(reply.receipt).authorization):reply.status==='authorized'?record(reply.authorization):reply;
    return record(record(auth.preview).request);
}
export class LegacyRetirementSession {
    private request?:RecordValue;
    private review?:RecordValue;
    private dispatched=false;
    private generation=0;
    private pageIdentity?:RecordValue;
    last?:RecordValue;
    get uncertainOrAccepted():boolean{return this.dispatched;}
    constructor(private readonly host:LegacyRetirementHost,private readonly trusted:()=>boolean){}
    restore(value:unknown):RecordValue {
        this.check(this.generation);if(this.request)throw new ServiceError('legacy_retirement_session_already_selected');
        const reply=validateRetirementReply(value);
        if(reply.status==='review')throw new ServiceError('legacy_retirement_saved_authorization_required');
        const auth=reply.status==='applied'?record(record(reply.receipt).authorization):record(reply.authorization);
        this.review={status:'review',preview:structuredClone(auth.preview),review_digest:auth.review_digest};
        this.request=structuredClone(retirementRequest(reply));this.dispatched=true;return this.last=reply;
    }
    stop():void {this.generation++;} // Stops UI work; accepted native work is not canceled or replayed.
    private check(g:number):void {if(!this.trusted()||g!==this.generation)throw new ServiceError('legacy_retirement_review_stopped');}
    async candidates(after?:string):Promise<RecordValue>{
        const g=this.generation;this.check(g);if(after)catalogId(after,'idx_');
        const r=bounded(await this.host.legacyRetirementCandidates(after));this.check(g);
        if(r.schema!==1||r.after!==(after??null)||!Array.isArray(r.items)||r.items.length>16
            ||typeof r.subset!=='boolean'||nonnegative(r.total_eligible)>1024)throw new ServiceError('invalid_legacy_retirement_page');
        identifier(r.upgrade_id,16);identifier(r.plan_digest,32);identifier(r.installer_identity,32);identifier(r.namespace,16);
        const identity={upgrade_id:r.upgrade_id,plan_digest:r.plan_digest,installer_identity:r.installer_identity,namespace:r.namespace,total_eligible:r.total_eligible};
        if(after&&(!this.pageIdentity||!isDeepStrictEqual(identity,this.pageIdentity)))throw new ServiceError('legacy_retirement_page_changed');
        this.pageIdentity=identity;
        for(const item of r.items){const row=record(item);catalogId(row.index_id,'idx_');record(row.registration);
            if(row.source_import!=='pending_metadata'||!Object.hasOwn(row,'observation'))throw new ServiceError('invalid_legacy_retirement_candidate');
            if(row.observation!==null){const reply=validateRetirementReply(row.observation),request=retirementRequest(reply);
                if(request.index_id!==row.index_id||request.upgrade_id!==r.upgrade_id||request.plan_digest!==r.plan_digest
                    ||request.installer_identity!==r.installer_identity||!isDeepStrictEqual(request.registration,row.registration))throw new ServiceError('crossed_legacy_retirement_candidate');}
        }
        if(r.continuation!==null){catalogId(r.continuation,'idx_');
            if(!r.items.length||record(r.items[r.items.length-1]).index_id!==r.continuation||r.continuation===after)throw new ServiceError('invalid_legacy_retirement_continuation');}
        if(r.subset!==(!!after||r.continuation!==null))throw new ServiceError('invalid_legacy_retirement_subset');
        return r;
    }
    async prepare(index:string):Promise<RecordValue>{
        if(this.dispatched)throw new ServiceError('check_exact_legacy_retirement_receipt_first');
        const g=this.generation;this.check(g);catalogId(index,'idx_');
        const reply=validateRetirementReply(await this.host.legacyRetirementReview(index));this.check(g);
        if(reply.status!=='review'||retirementRequest(reply).index_id!==index)throw new ServiceError('crossed_legacy_retirement_review');
        this.review=structuredClone(reply);this.request=structuredClone(retirementRequest(reply));return this.last=reply;
    }
    async authorize(confirmed:RecordValue):Promise<RecordValue>{
        const g=this.generation;this.check(g);
        if(this.dispatched||!this.review||!isDeepStrictEqual(confirmed,this.review)||!this.request)throw new ServiceError('legacy_retirement_review_changed');
        this.dispatched=true; // Set before dispatch; a lost reply can only trigger a read-only exact lookup.
        const reply=this.accept(await this.host.legacyRetirementAuthorize(this.review,()=>this.check(g)));this.check(g);
        return this.last=reply;
    }
    async reconcile():Promise<RecordValue|null>{
        const g=this.generation;this.check(g);if(!this.request)throw new ServiceError('legacy_retirement_request_missing');
        const value=await this.host.legacyRetirementReceipt(this.request);this.check(g);
        if(value===null)return null; // Keep last complete evidence; absence never authorizes replay.
        return this.last=this.accept(value);
    }
    async resume():Promise<RecordValue>{
        const g=this.generation;this.check(g);
        if(!this.request||!['authorized','applied'].includes(String(this.last?.status)))throw new ServiceError('legacy_retirement_not_authorized');
        const reply=this.accept(await this.host.legacyRetirementResume(this.request,()=>this.check(g)));this.check(g);
        return this.last=reply;
    }
    private accept(value:unknown):RecordValue {
        const reply=validateRetirementReply(value,this.request);
        const auth=reply.status==='applied'?record(record(reply.receipt).authorization):record(reply.authorization);
        if(!this.review||reply.status==='review'||!isDeepStrictEqual(auth.preview,this.review.preview)
            ||auth.review_digest!==this.review.review_digest)throw new ServiceError('crossed_legacy_retirement_review');
        return reply;
    }
}
export function retirementJson(reply:RecordValue):string{return JSON.stringify(reply,null,2);}
export function retirementMarkdown(reply:RecordValue):string {
    return '# AIRK legacy upgrade retirement\n\nNative runtime administration; source files are preserved. Physical cleanup remains pending. Applied is a historical logical receipt, not current activation readiness or indexing/read authority. No recovery evidence is persisted by this view.\n\n```json\n'+retirementJson(reply)+'\n```\n';
}
''')
change('packages/vscode/src/serviceBootstrap.ts',"import type { LifecycleIntent } from './serviceIntents';", "import type { LifecycleIntent } from './serviceIntents';\nimport { validateRetirementReply } from './runtimeLegacyRetirement';")
change('packages/vscode/src/serviceBootstrap.ts','    async repair(binding:RuntimeBinding,preview?:string):Promise<RecordValue>{',r'''    private async legacyAdmin(action:string,input?:RecordValue,index?:string,admit?:()=>void,after?:string):Promise<RecordValue|null>{
        this.active();admit?.();const bundle=await this.verified();this.active();admit?.();
        const location=await this.invoke(bundle.main,['runtime','location','--channel',this.options.channel]);
        const install=this.options.installDir??safePath(location.install_dir);
        const args=['runtime',`legacy-retirement-${action}`,'--install-dir',install];
        if(index){if(!/^idx_[0-9a-f]{32}$/.test(index))throw new ServiceError('invalid_legacy_retirement_index');args.push('--index-id',index);}
        if(after){if(!/^idx_[0-9a-f]{32}$/.test(after))throw new ServiceError('invalid_legacy_retirement_continuation');args.push('--after',after);}
        // Use the paired verified bundled candidate, never an older selected predecessor.
        // Native admission must independently match the saved installer identity.
        this.active();admit?.(); // No await between the session's final Stop/trust check and dispatch.
        const result=await this.run(bundle.main,args,180000,input?JSON.stringify(input):undefined);this.active();
        if(action==='receipt'){
            if(result.schema!==1||!Object.hasOwn(result,'observation'))throw new ServiceError('invalid_legacy_retirement_observation');
            return result.observation===null?null:validateRetirementReply(result.observation);
        }
        return action==='candidates'?record(result):validateRetirementReply(result);
    }
    async legacyRetirementCandidates(after?:string):Promise<RecordValue>{return record(await this.legacyAdmin('candidates',undefined,undefined,undefined,after));}
    async legacyRetirementReview(index:string):Promise<RecordValue>{return record(await this.legacyAdmin('review',undefined,index));}
    async legacyRetirementAuthorize(review:RecordValue,admit:()=>void):Promise<RecordValue>{
        return record(await this.legacyAdmin('authorize',{preview:review.preview,review_digest:review.review_digest},undefined,admit));
    }
    async legacyRetirementReceipt(request:RecordValue):Promise<RecordValue|null>{return this.legacyAdmin('receipt',request);}
    async legacyRetirementResume(request:RecordValue,admit:()=>void):Promise<RecordValue>{return record(await this.legacyAdmin('resume',request,undefined,admit));}
    async repair(binding:RuntimeBinding,preview?:string):Promise<RecordValue>{''')
change('packages/vscode/src/serviceExtension.ts',"import { automaticUpgrade,", "import { LegacyRetirementSession, retirementJson, retirementMarkdown } from './runtimeLegacyRetirement';\nimport { automaticUpgrade,")
change('packages/vscode/src/serviceExtension.ts','    private host?:RuntimeHost;', '    private host?:RuntimeHost;\n    private legacyRetirement?:LegacyRetirementSession;')
change('packages/vscode/src/serviceExtension.ts','    async reviewData(run=nativeRunner):Promise<void>{',r'''    /** Runtime administration remains separate from evidence tools and SessionStore. */
    async reviewLegacyUpgrade():Promise<void>{
        this.trusted();const generation=this.generation;let stopped=false;
        const current=()=>{this.trusted();if(stopped||generation!==this.generation)throw new ServiceError('legacy_retirement_review_stopped');};
        const session=this.legacyRetirement??=new LegacyRetirementSession(this.runtime(),()=>{
            try{this.trusted();return generation===this.generation;}catch{return false;}
        });
        return vscode.window.withProgress({location:vscode.ProgressLocation.Notification,title:'AIRK legacy upgrade retirement',cancellable:true},async(_progress,token)=>{
        const cancellation=token.onCancellationRequested(()=>{stopped=true;session.stop();});
        try{
        if(!session.last){
            let after:string|undefined;
            for(;;){
            const candidates=await session.candidates(after);current();
            if(!Array.isArray(candidates.items))throw new ServiceError('invalid_legacy_retirement_candidates');
            const items:{label:string;description:string;row:Record<string,unknown>;saved?:unknown;next?:string}[]=candidates.items.map(value=>{
                const row=record(value),registration=record(row.registration);
                return {label:String(registration.root),description:row.observation===null?String(row.index_id):'Check saved native retirement receipt',row,saved:row.observation??undefined};
            });
            if(typeof candidates.continuation==='string'){
                items.push({label:'Next page',description:'Explicitly retrieve the next bounded subset of saved pending-metadata indexes',row:{},next:candidates.continuation});
            }
            const selected=await vscode.window.showQuickPick(items,{title:'AIRK: Review unavailable legacy index retirement',
                placeHolder:`${candidates.items.length} shown of ${candidates.total_eligible} saved pending-metadata indexes. Sources and physical storage are preserved.`});
            current();if(!selected){session.stop();return;}
            if(selected.next){after=selected.next;continue;}
            if(selected.saved)session.restore(selected.saved);else await session.prepare(String(selected.row.index_id));current();
            break;
            }
        }
        for(;;){
            current();const reply=session.last;if(!reply)throw new ServiceError('legacy_retirement_review_missing');
            const actions=['Show complete details','Copy JSON','Copy Markdown','Export JSON','Export Markdown'];
            if(!session.uncertainOrAccepted)actions.push('Authorize reviewed retirement');
            else actions.push('Check exact receipt');
            if(['authorized','applied'].includes(String(reply.status)))actions.push('Resume accepted same-candidate activation');
            actions.push('Stop');
            const action=await vscode.window.showQuickPick(actions,{title:`AIRK legacy retirement: ${reply.status}`,
                placeHolder:'Logical retirement, physical cleanup and activation readiness are separate. Recovery data stays in memory only.'});
            current();if(!action||action==='Stop'){session.stop();return;}
            if(action==='Show complete details'){
                const document=await vscode.workspace.openTextDocument({language:'json',content:retirementJson(reply)});current();
                await vscode.window.showTextDocument(document);continue;
            }
            if(action==='Copy JSON'||action==='Copy Markdown'){
                await vscode.env.clipboard.writeText(action==='Copy JSON'?retirementJson(reply):retirementMarkdown(reply));continue;
            }
            if(action==='Export JSON'||action==='Export Markdown'){
                const target=await vscode.window.showSaveDialog({title:'Export complete native legacy retirement receipt',
                    filters:action==='Export JSON'?{JSON:['json']}:{Markdown:['md']}});current();
                if(target)await vscode.workspace.fs.writeFile(target,Buffer.from(action==='Export JSON'?retirementJson(reply):retirementMarkdown(reply)));
                continue;
            }
            try{
                if(action==='Authorize reviewed retirement'){
                    const yes=await vscode.window.showWarningMessage('Retire only this reviewed unavailable legacy index and cancel its exact accepted work?',
                        {modal:true,detail:'Saves this exact native authorization only. Sources and physical storage are preserved. Physical cleanup remains pending. Resume is a separate explicit action; this confirmation does not start activation.'},'Authorize retirement');
                    current();if(yes!=='Authorize retirement')continue;
                    await session.authorize(reply);current();
                }else if(action==='Check exact receipt'){
                    const saved=await session.reconcile();current();
                    if(saved===null)await vscode.window.showWarningMessage('The exact receipt is unavailable. The previous full observation is retained; no authorization or job was replayed.');
                }else if(action==='Resume accepted same-candidate activation'){
                    const yes=await vscode.window.showWarningMessage('Resume this already accepted retirement through the same verified candidate?',
                        {modal:true,detail:'This continues native activation. It creates no new retirement authorization and does not infer readiness from the logical receipt.'},'Resume accepted activation');
                    current();if(yes==='Resume accepted activation'){await session.resume();current();}
                }
            }catch(error){
                current();this.output.appendLine(String(error));
                await vscode.window.showWarningMessage('The native outcome may be incomplete or unknown. Check the exact saved receipt; authorization is never replayed automatically.');
            }
        }
        }finally{cancellation.dispose();}
        });
    }
    async reviewData(run=nativeRunner):Promise<void>{''')
change('packages/vscode/src/serviceExtension.ts','    commands.reviewData=()=>controller.reviewData();','    commands.reviewData=()=>controller.reviewData();\n    commands.reviewLegacyUpgrade=()=>controller.reviewLegacyUpgrade();')
change('packages/vscode/src/serviceExtension.ts','    reset():void{this.generation++;', '    reset():void{this.legacyRetirement?.stop();this.legacyRetirement=undefined;this.generation++;')
change('packages/vscode/src/serviceExtension.ts','    dispose():void{if(this.disposed)return;this.disposed=true;', '    dispose():void{if(this.disposed)return;this.legacyRetirement?.stop();this.legacyRetirement=undefined;this.host=undefined;this.disposed=true;')
# Both actual package allowlists must ship the imported host helper.
p='packages/vscode/package.json';s=(W/p).read_text(encoding='utf-8')
s=s.replace('"out/serviceBootstrap.js",','"out/serviceBootstrap.js",\n    "out/runtimeLegacyRetirement.js",',1)
needle='"commands": ['
assert needle in s
s=s.replace(needle,needle+'\n      {"command":"airk.reviewLegacyUpgrade","title":"AIRK: Review unavailable legacy upgrade index retirement"},',1)
put(p,s)
change('scripts/package-extension.py','"serviceBootstrap.js",','"serviceBootstrap.js", "runtimeLegacyRetirement.js",')
put('packages/vscode/src/test/runtimeLegacyRetirement.test.ts',r'''// SPDX-License-Identifier: Apache-2.0
import {test} from 'node:test';import assert from 'node:assert/strict';
import {LegacyRetirementSession,LegacyRetirementHost,retirementJson,retirementMarkdown,validateRetirementReply} from '../runtimeLegacyRetirement';
import {RecordValue} from '../serviceClient';
const id='idx_'+'1'.repeat(32),hash='2'.repeat(64);
function review():RecordValue{return {status:'review',review_digest:hash,preview:{schema:1,request_digest:hash,source_configuration:'pending_metadata',
 original_backup_digest:hash,original_legacy_data_digest:hash,effect:'retire_and_cancel_with_cleanup_pending',affected_work:{jobs:2,job_ids_digest:hash},
 request:{schema:1,index_id:id,caller:'1'.repeat(32),upgrade_id:'3'.repeat(32),plan_digest:hash,installer_identity:hash,expected_authorizations_digest:hash,
 key:{namespace:'4'.repeat(32),epoch:0,nonce:'5'.repeat(32)},registration:{binding:hash,root:'/fictional/source',data_root:'/fictional/private',collection:'a'},future:'retained'},future:{proof:'complete'}}};}
function accepted(r:RecordValue,status='authorized'):RecordValue {
 const authorization={preview:r.preview,review_digest:r.review_digest};
 return status==='authorized'?{status,authorization}:{status:'applied',receipt:{schema:1,authorization,catalog_revision:2,
 canceled_work:(r.preview as RecordValue).affected_work,post_effect_legacy_data_digest:hash,cleanup:'pending',future:{native:'complete'}}};
}
function fixture(){let r=review(),auth=0,reads=0,resumes=0,trusted=true,lost=false;let observed:RecordValue|null=null;
 const host:LegacyRetirementHost={legacyRetirementCandidates:async()=>({schema:1,upgrade_id:'3'.repeat(32),plan_digest:hash,installer_identity:hash,namespace:'4'.repeat(32),after:null,items:[],continuation:null,subset:false,total_eligible:0}),legacyRetirementReview:async()=>r,
 legacyRetirementAuthorize:async(_review,admit)=>{admit();auth++;if(lost)throw new Error('lost response');return observed=accepted(r);},
 legacyRetirementReceipt:async()=>{reads++;return observed;},legacyRetirementResume:async(_request,admit)=>{admit();resumes++;return accepted(r,'applied');}};
 const session=new LegacyRetirementSession(host,()=>trusted);
 return {session,r,host,get auth(){return auth;},get reads(){return reads;},get resumes(){return resumes;},lose(){lost=true;},
 untrust(){trusted=false;},observe(value:RecordValue|null){observed=value;}};
}
test('lost authorization response uses exact readonly receipt and never authorizes twice',async()=>{
 const f=fixture();await f.session.prepare(id);f.lose();await assert.rejects(f.session.authorize(f.r));
 await assert.rejects(f.session.authorize(f.r));assert.equal(await f.session.reconcile(),null);assert.equal(f.session.last?.status,'review');
 f.observe(accepted(f.r));assert.equal((await f.session.reconcile())?.status,'authorized');assert.equal(f.auth,1);assert.equal(f.reads,2);
});
test('crossed source, preview and review digest refusals preserve original complete observation',async()=>{
 const f=fixture();await f.session.prepare(id);f.lose();await assert.rejects(f.session.authorize(f.r));
 for(const change of ['request','preview','digest']){
  const crossed=structuredClone(f.r),p=crossed.preview as RecordValue;
  if(change==='request')(p.request as RecordValue).future='crossed';else if(change==='preview')p.future={proof:'crossed'};else crossed.review_digest='3'.repeat(64);
  f.observe(accepted(crossed));await assert.rejects(f.session.reconcile());
 }
 assert.deepEqual(f.session.last,f.r);
});
test('Stop and revoked trust across modal awaits cannot dispatch mutation',async()=>{
 const f=fixture();await f.session.prepare(id);f.untrust();await assert.rejects(f.session.authorize(f.r));assert.equal(f.auth,0);
 const g=fixture();const changed=structuredClone(g.r);await g.session.prepare(id);changed.review_digest='4'.repeat(64);
 await assert.rejects(g.session.authorize(changed));assert.equal(g.auth,0);
});
test('Stop during native review drops completion without dispatching authorization',async()=>{
 const f=fixture();let release!:(value:RecordValue)=>void;
 f.host.legacyRetirementReview=()=>new Promise(resolve=>{release=resolve;});
 const pending=f.session.prepare(id);f.session.stop();release(f.r);
 await assert.rejects(pending);assert.equal(f.auth,0);assert.equal(f.session.last,undefined);
});
test('logical Applied keeps cleanup pending and permits explicit same-candidate continuation',async()=>{
 const f=fixture();await f.session.prepare(id);await f.session.authorize(f.r);f.observe(accepted(f.r,'applied'));await f.session.reconcile();
 await f.session.resume();assert.equal(f.resumes,1);assert.equal(f.auth,1);
 assert.match(retirementJson(f.session.last!),/"future"/);assert.match(retirementMarkdown(f.session.last!),/"native": "complete"/);
 assert.match(retirementMarkdown(f.session.last!),/Physical cleanup remains pending/);
});
test('reload restores exact native accepted key for readonly reconciliation without new review or authorization',async()=>{
 const f=fixture(),saved=accepted(f.r,'applied');f.observe(saved);f.session.restore(saved);
 assert.deepEqual(await f.session.reconcile(),saved);assert.equal(f.auth,0);assert.equal(f.reads,1);
 await assert.rejects(f.session.authorize(f.r));assert.equal(f.auth,0);
});
test('candidate pages disclose subsets and reject changed native plan or looping continuation',async()=>{
 const f=fixture(),next='idx_'+'6'.repeat(32);let changed=false;
 f.host.legacyRetirementCandidates=async after=>({schema:1,upgrade_id:'3'.repeat(32),plan_digest:changed?'7'.repeat(64):hash,installer_identity:hash,namespace:'4'.repeat(32),
 after:after??null,items:[{index_id:after?next:id,registration:{root:'/fictional/source'},source_import:'pending_metadata',observation:null}],
 continuation:after?null:id,subset:true,total_eligible:2});
 const first=await f.session.candidates();assert.equal(first.continuation,id);assert.equal(first.subset,true);
 const second=await f.session.candidates(id);assert.equal(second.continuation,null);assert.equal(second.subset,true);
 changed=true;await assert.rejects(f.session.candidates(id),/page_changed/);assert.equal(f.auth,0);
});
test('arbitrary reply, malformed status and false physical completion refuse',()=>{
 assert.throws(()=>validateRetirementReply({status:'ready'}));const value=accepted(review(),'applied');
 (value.receipt as RecordValue).cleanup='complete';assert.throws(()=>validateRetirementReply(value));
 for(const epoch of [-1,'0',0.5,Number.MAX_SAFE_INTEGER+1]){
  const bad=review();const p=bad.preview as RecordValue;((p.request as RecordValue).key as RecordValue).epoch=epoch;
  assert.throws(()=>validateRetirementReply(bad));
 }
 validateRetirementReply(review()); // Canonical request epoch zero is valid.
});
''')
change('packages/vscode/src/test/serviceBootstrap.test.ts',"import { nativePayloadDirectory } from './nativeServiceFixture';", "import { nativePayloadDirectory } from './nativeServiceFixture';\nimport { LegacyRetirementSession } from '../runtimeLegacyRetirement';")
p='packages/vscode/src/test/serviceBootstrap.test.ts'
put(p,files[p][1].decode()+r'''
test('legacy retirement Stop during RuntimeHost location preparation prevents later authorize or resume dispatch',async()=>{
    for(const action of ['authorize','resume']){
        const f=await fixture(),entered=lazySignal<void>(),release=lazySignal<void>();let held=false,mutations=0;
        const hash='ab'.repeat(32),index='idx_'+'cd'.repeat(16),who='ef'.repeat(16);
        const preview={schema:1,request_digest:hash,original_backup_digest:hash,original_legacy_data_digest:hash,
            source_configuration:'pending_metadata',effect:'retire_and_cancel_with_cleanup_pending',affected_work:{jobs:0,job_ids_digest:hash},
            request:{schema:1,caller:who,key:{namespace:who,epoch:0,nonce:who},upgrade_id:who,plan_digest:hash,installer_identity:hash,index_id:index,
                registration:{binding:hash,root:'/fictional/source',data_root:'/fictional/private',collection:'a'},expected_authorizations_digest:hash}};
        const review={status:'review',preview,review_digest:hash};
        const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable'},async(_command,args)=>{
            if(args[1]==='location'){if(held){entered.resolve();await release.promise;}return {protocol:1,install_dir:path.join(f.root,'install')};}
            if(args[1]==='legacy-retirement-review')return review;
            mutations++;throw new Error('mutation must not dispatch after Stop');
        });
        const session=new LegacyRetirementSession(host,()=>true);let pending:Promise<unknown>|undefined;
        try{
            if(action==='authorize')await session.prepare(index);
            else session.restore({status:'authorized',authorization:{preview,review_digest:hash}});
            held=true;pending=action==='authorize'?session.authorize(review):session.resume();
            await entered.promise;session.stop();release.resolve();
            await assert.rejects(pending,/review_stopped/);assert.equal(mutations,0);
        }finally{release.resolve();await Promise.allSettled([pending]);await f.cleanup();}
    }
});
''')
change('packages/vscode/src/test/serviceExtension.test.ts',"import { RuntimeHost } from '../serviceBootstrap';", "import { RuntimeHost } from '../serviceBootstrap';\nimport { LegacyRetirementSession } from '../runtimeLegacyRetirement';")
change('packages/vscode/src/test/serviceExtension.test.ts',"        stub.workspace.isTrusted=true;extension.deactivate();await assert.rejects(provider.resolveMcpServerDefinition(definitions[0]),",r'''        stub.workspace.isTrusted=true;
        // Actual owning controller reset/dispose while the actual RuntimeHost is
        // awaiting location. A permissive host-level guard isolates the Session
        // generation defense; no native process is executed by this consumer test.
        for(const lifecycle of ['reset','dispose'])for(const action of ['authorize','resume']){
            const scoped=new controller.constructor(extensionContext),generation=scoped.generation;
            let enter!:()=>void,release!:()=>void,held=false,mutations=0;
            const entered=new Promise<void>(resolve=>{enter=resolve;}),released=new Promise<void>(resolve=>{release=resolve;});
            const digest='ab'.repeat(32),who='cd'.repeat(16),index='idx_'+'ef'.repeat(16);
            const preview={schema:1,request_digest:digest,original_backup_digest:digest,original_legacy_data_digest:digest,
                source_configuration:'pending_metadata',effect:'retire_and_cancel_with_cleanup_pending',affected_work:{jobs:0,job_ids_digest:digest},
                request:{schema:1,caller:who,key:{namespace:who,epoch:0,nonce:who},upgrade_id:who,plan_digest:digest,installer_identity:digest,index_id:index,
                    registration:{binding:digest,root:'/fictional/source',data_root:'/fictional/private',collection:'a'},expected_authorizations_digest:digest}};
            const review={status:'review',preview,review_digest:digest};
            const host=new RuntimeHost(root,'1.4.0',{channel:'stable'},async(_command,args)=>{
                if(args[1]==='legacy-retirement-review')return review;
                mutations++;throw new Error('stale controller dispatched legacy retirement');
            });
            context.mock.method(host as any,'verified',async()=>({main:path.join(root,'fictional-main')}));
            context.mock.method(host as any,'invoke',async(_command:string,args:string[])=>{
                assert.equal(args[1],'location');if(held){enter();await released;}return {protocol:1,install_dir:root};
            });
            const session=new LegacyRetirementSession(host,()=>{
                try{scoped.trusted();return generation===scoped.generation;}catch{return false;}
            });
            scoped.host=host;scoped.legacyRetirement=session;let pending:Promise<unknown>|undefined;
            try{
                if(action==='authorize')await session.prepare(index);
                else session.restore({status:'authorized',authorization:{preview,review_digest:digest}});
                held=true;pending=action==='authorize'?session.authorize(review):session.resume();await entered;
                scoped[lifecycle]();assert.equal(scoped.legacyRetirement,undefined);assert.notEqual(scoped.host,host);
                release();await assert.rejects(pending,/review_stopped/);assert.equal(mutations,0);
            }finally{release();await Promise.allSettled([pending]);scoped.dispose();}
        }
        extension.deactivate();await assert.rejects(provider.resolveMcpServerDefinition(definitions[0]),''')
manifest={'status':'INERT adapter proposal; not compiled/applied/qualified; CoreR2+Store recovery prerequisite','files':[],
 'limits':['Actual native ownership/transport tests pending; mocks qualify only consumer state machine','Core logical Applied is not global activation readiness','No automatic replay, source/data access, MCP write or persisted SessionStore state','Review/request retained in memory; reload recovers exact accepted native authorization via readonly saved observations, never regenerates a key','Generated CLI reference must use canonical writer after eventual application']}
patch=''
for p,(b,s) in files.items():
 manifest['files'].append({'path':p,'base_sha256':hashlib.sha256(b).hexdigest() if b else None,'proposed_sha256':hashlib.sha256(s).hexdigest()})
 patch+=''.join(difflib.unified_diff(b.decode().splitlines(keepends=True),s.decode().splitlines(keepends=True),fromfile='a/'+p if b else '/dev/null',tofile='b/'+p))
(O/'adapters.patch').write_bytes(patch.encode());(O/'manifest.json').write_bytes((json.dumps(manifest,indent=2)+'\n').encode())
print(O)
