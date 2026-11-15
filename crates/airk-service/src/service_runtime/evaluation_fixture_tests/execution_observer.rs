// SPDX-License-Identifier: Apache-2.0
//! Private fixture recorder. All filesystem I/O and release waits stay outside native gates.
use super::*;
use airk_inference::inference_engine::pool::arbiter::execution_observer::{
    Binding, Kind, Observer, RecorderSummary,
};
use std::io::Write;
use tokio::sync::oneshot;
use tokio::task::JoinSet;
const MAX_RECORDS: usize = 8192;
const MAX_BYTES: u64 = 64 << 20;
pub(super) struct Recorder {
    observer: Observer,
    stop: oneshot::Sender<()>,
    task: tokio::task::JoinHandle<Result<()>>,
}
impl Recorder {
    pub(super) fn from_env(work: &Path) -> Result<Option<Self>> {
        match std::env::var("AIRK_NATIVE_DISPATCH_OBSERVER") {
            Ok(value) if value == "true" => {}
            Ok(value) if value == "false" => return Ok(None),
            Err(std::env::VarError::NotPresent) => return Ok(None),
            _ => bail!("private native dispatch observer must be explicit true/false"),
        }
        Self::start(work).map(Some)
    }
    fn start(work: &Path) -> Result<Self> {
        let (observer, mut receiver) = Observer::channel(4096)?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(work.join("composed-observer.jsonl"))?;
        let (stop, mut stopped) = oneshot::channel();
        let recorded = observer.clone();
        let work = work.to_owned();
        let task = tokio::spawn(async move {
            let mut records = 0usize;
            let mut bytes = 0u64;
            let mut last_sequence = 0u64;
            let result=async{
    let mut watchers=JoinSet::new();
    let mut bindings:BTreeMap<String,Vec<Binding>>=BTreeMap::new();
    let mut released=BTreeSet::new();let mut finishing=false;let mut dispatched=0usize;
    loop{
     if finishing&&watchers.is_empty()&&receiver.is_empty(){recorded.close();break;}
     tokio::select!{biased;
      event=receiver.recv()=>{
       let event=event.context("native event channel closed before recorder settlement")?;
       event.record.validate()?;
       ensure!(event.record.sequence>last_sequence,"native observer sequence regressed");last_sequence=event.record.sequence;
       let mut raw=serde_json::to_vec(&event.record)?;raw.push(b'\n');
       records=records.checked_add(1).context("observer record counter overflow")?;bytes=bytes.checked_add(raw.len() as u64).context("observer byte counter overflow")?;
       ensure!(records<=MAX_RECORDS&&bytes<=MAX_BYTES,"private observer record/byte bound exceeded");
       file.write_all(&raw)?;file.flush()?;
       if let Some(proof)=event.release{
        ensure!(event.record.kind==Kind::DispatchAdmitted,"charge proof outside native dispatch");
        let id=event.record.binding.charge_identity.clone();ensure!(!released.contains(&id),"native charge dispatched after actual release");
        dispatched+=1;ensure!(dispatched<=1024,"private native dispatch census bound exceeded");
        let first=!bindings.contains_key(&id);let entry=bindings.entry(id.clone()).or_default();
        ensure!(!entry.iter().any(|value|value.dispatch_identity==event.record.binding.dispatch_identity),"duplicate dispatch identity");
        entry.push(event.record.binding.clone());
        if first{watchers.spawn(async move{(id,proof.wait().await)});}
       }
      },
      _=&mut stopped,if !finishing=>{finishing=true;},
      Some(completed)=watchers.join_next(),if !watchers.is_empty()=>{
       let (id,proof)=completed.context("actual charge release observer task failed")?;let proof=proof?;
       let observed=bindings.get(&id).context("actual charge completion lacks original dispatch binding")?;
       proof.emit_for(observed)?;ensure!(released.insert(id),"duplicate actual charge completion");
      }
     }
    }
    ensure!(recorded.dropped()==0,"native observer lost events; no complete acceptance evidence");file.sync_all()?;Ok::<(),anyhow::Error>(())
   }.await;
            recorded.close();
            let summary=RecorderSummary{schema:"airk.private.native-dispatch-recorder/1".into(),complete:result.is_ok()&&recorded.dropped()==0,evaluation:true,release_qualified:false,dropped:recorded.dropped(),writer_failed:usize::from(result.is_err()),records,bytes,last_sequence,error:result.as_ref().err().map(|error|format!("{error:#}")),proof:"Recorder closes only after actual service ownership settlement and native per-charge release futures; dispatch is the guarded score-call boundary, not an inside-FFI marker.".into()};
            summary.validate()?;
            write(&work, "composed-observer.closed.json", &summary)?;
            result
        });
        Ok(Self {
            observer,
            stop,
            task,
        })
    }
    pub(super) fn observer(&self) -> Observer {
        self.observer.clone()
    }
    pub(super) async fn finish(self) -> Result<()> {
        let _ = self.stop.send(());
        self.task
            .await
            .context("native recorder task closed without a result")?
    }
}
