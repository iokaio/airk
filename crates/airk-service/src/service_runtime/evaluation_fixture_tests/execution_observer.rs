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
/// The census bound of a lifetime that names none: the bound every lifetime had before LH.
const DEFAULT_DISPATCHES: usize = 1024;
/// The largest census a lifetime may ask for. The recorder keeps one binding per dispatch in memory, so the
/// bound protects the observer's memory; this ceiling is about 900 Answers at the /3 default budget's 72
/// model calls each.
const MAX_DISPATCHES: usize = 65_536;
/// Records written per admitted dispatch (admitted, score returned, job settled), with one spare.
const RECORDS_PER_DISPATCH: usize = 4;
/// Bytes allowed per record; the LG gate's records were about 1,450 bytes each.
const BYTES_PER_RECORD: u64 = 4096;
/// The variable through which a fixture lifetime sets its dispatch census (the harness derives it from the
/// lifetime's plan).
pub(super) const CENSUS_VARIABLE: &str = "AIRK_NATIVE_DISPATCH_CENSUS_BOUND";

/// The recorder's bounds for one lifetime: dispatches, records and bytes. The LG gate (23 November 2026)
/// made more than 1,024 dispatches in each arm's lifetime, so the fixed bound refused the 1,025th dispatch
/// and no lifetime settled its close.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Bounds {
    dispatches: usize,
    records: usize,
    bytes: u64,
}
impl Bounds {
    fn for_dispatches(dispatches: usize) -> Result<Self> {
        ensure!(
            (1..=MAX_DISPATCHES).contains(&dispatches),
            "{CENSUS_VARIABLE} must be a whole number from 1 to {MAX_DISPATCHES}, not {dispatches}"
        );
        let records = MAX_RECORDS.max(dispatches * RECORDS_PER_DISPATCH);
        Ok(Self {
            dispatches,
            records,
            bytes: MAX_BYTES.max(records as u64 * BYTES_PER_RECORD),
        })
    }
    pub(super) fn from_value(value: std::result::Result<String, std::env::VarError>) -> Result<Self> {
        match value {
            Err(std::env::VarError::NotPresent) => Self::for_dispatches(DEFAULT_DISPATCHES),
            Ok(text) => {
                let dispatches = text
                    .parse::<usize>()
                    .ok()
                    .filter(|_| text.bytes().all(|byte| byte.is_ascii_digit()))
                    .with_context(|| {
                        format!(
                            "{CENSUS_VARIABLE} must be a whole number from 1 to {MAX_DISPATCHES}, not {text:?}"
                        )
                    })?;
                Self::for_dispatches(dispatches)
            }
            Err(error) => bail!("{CENSUS_VARIABLE} is not readable: {error}"),
        }
    }
    /// Admits the `dispatched`-th dispatch of the lifetime (counting from 1), or says the bound is too small.
    pub(super) fn admit_dispatch(&self, dispatched: usize) -> Result<()> {
        ensure!(
            dispatched <= self.dispatches,
            "private native dispatch census bound exceeded: dispatch {dispatched} of a lifetime bounded at {} \
             ({CENSUS_VARIABLE}; the harness sets it from the lifetime's plan, at most {MAX_DISPATCHES})",
            self.dispatches
        );
        Ok(())
    }
    pub(super) fn admit_record(&self, records: usize, bytes: u64) -> Result<()> {
        ensure!(
            records <= self.records && bytes <= self.bytes,
            "private observer record/byte bound exceeded: {records} records and {bytes} bytes against {} and {} \
             (derived from {CENSUS_VARIABLE} = {})",
            self.records,
            self.bytes,
            self.dispatches
        );
        Ok(())
    }
}

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
        let bounds = Bounds::from_value(std::env::var(CENSUS_VARIABLE))?;
        Self::start(work, bounds).map(Some)
    }
    fn start(work: &Path, bounds: Bounds) -> Result<Self> {
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
       bounds.admit_record(records,bytes)?;
       file.write_all(&raw)?;file.flush()?;
       if let Some(proof)=event.release{
        ensure!(event.record.kind==Kind::DispatchAdmitted,"charge proof outside native dispatch");
        let id=event.record.binding.charge_identity.clone();ensure!(!released.contains(&id),"native charge dispatched after actual release");
        dispatched+=1;bounds.admit_dispatch(dispatched)?;
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

#[test]
fn census_default_keeps_the_old_bound_and_a_lifetime_may_raise_it() -> Result<()> {
    use std::env::VarError;
    let default = Bounds::from_value(Err(VarError::NotPresent))?;
    default.admit_dispatch(1024)?;
    let refused = default.admit_dispatch(1025).unwrap_err().to_string();
    assert!(refused.contains("dispatch 1025 of a lifetime bounded at 1024"), "{refused}");
    assert!(refused.contains("AIRK_NATIVE_DISPATCH_CENSUS_BOUND"), "{refused}");
    default.admit_record(8192, 64 << 20)?;
    assert!(default.admit_record(8193, 1).is_err());

    // A gate-length lifetime: the LG arms each passed 1,024 dispatches; the old fixed bound refused this.
    let gate = Bounds::from_value(Ok("20000".into()))?;
    gate.admit_dispatch(1025)?;
    gate.admit_dispatch(20000)?;
    assert!(gate.admit_dispatch(20001).is_err());
    // Three records per dispatch at about 1,450 bytes each were observed; the derived bounds admit them.
    gate.admit_record(60000, 60000 * 1450)?;
    assert!(gate.admit_record(80001, 1).is_err());

    for bad in ["0", "", "-1", "+5", "1e4", " 10", "65537"] {
        assert!(Bounds::from_value(Ok(bad.into())).is_err(), "{bad:?} was accepted");
    }
    assert!(Bounds::from_value(Ok("65536".into())).is_ok());
    Ok(())
}
