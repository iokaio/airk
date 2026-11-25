// SPDX-License-Identifier: Apache-2.0
//! Private CS2 card-call recorder for the ignored same-install CUDA fixture.
use super::*;
use crate::service_runtime::arbiter::card_capture::{MAX_RECORDS, Observer, Record};
use std::io::Write;
use tokio::{sync::oneshot, task::JoinHandle};

const MAX_BYTES: usize = 64 << 20;
const MAX_RECORD_BYTES: usize = 512 << 10;

#[derive(Serialize)]
struct Closure {
    schema: &'static str,
    evaluation: bool,
    release_qualified: bool,
    complete: bool,
    service_completed: bool,
    records: usize,
    bytes: usize,
    sha256: Option<Sha256Digest>,
    dropped: usize,
    writer_failed: bool,
}

pub(super) struct Recorder {
    observer: Observer,
    stop: oneshot::Sender<bool>,
    task: JoinHandle<Result<()>>,
}

impl Recorder {
    pub(super) fn from_env(work: &Path) -> Result<Option<Self>> {
        match std::env::var("AIRK_ARBITER_EVALUATION_CARD_CAPTURE") {
            Err(std::env::VarError::NotPresent) => Ok(None),
            Ok(value) if value == "false" => Ok(None),
            Ok(value) if value == "true" => Self::start(work).map(Some),
            _ => bail!("private card capture mode must be absent, false or true"),
        }
    }

    fn start(work: &Path) -> Result<Self> {
        let (observer, mut receiver) = Observer::channel();
        let (stop, mut stopped) = oneshot::channel();
        let recorder_observer = observer.clone();
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(work.join("development-card-calls.jsonl"))?;
        let work = work.to_owned();
        let task = tokio::spawn(async move {
            let mut digest = Sha256::new();
            let mut records = 0;
            let mut bytes = 0;
            let mut service_completed = false;
            let result = async {
                let mut append = |record: Record| -> Result<()> {
                    let mut raw = serde_json::to_vec(&record)?;
                    raw.push(b'\n');
                    ensure!(
                        records < MAX_RECORDS
                            && raw.len() <= MAX_RECORD_BYTES
                            && raw.len() <= MAX_BYTES - bytes,
                        "private development card capture writer bound exceeded"
                    );
                    file.write_all(&raw)?;
                    digest.update(&raw);
                    records += 1;
                    bytes += raw.len();
                    Ok(())
                };
                loop {
                    tokio::select! {
                        status = &mut stopped => {
                            service_completed = status.unwrap_or(false);
                            receiver.close();
                            while let Some(record) = receiver.recv().await {
                                append(record)?;
                            }
                            break;
                        }
                        record = receiver.recv() => {
                            append(record.context("private card capture channel closed before shutdown")?)?;
                        }
                    }
                }
                file.sync_all()?;
                Ok::<(), anyhow::Error>(())
            }
            .await;
            receiver.close();
            let dropped = recorder_observer.dropped();
            let complete = service_completed && result.is_ok() && dropped == 0 && records != 0;
            let closure = Closure {
                schema: "airk.private.development-card-capture-closure/1",
                evaluation: true,
                release_qualified: false,
                complete,
                service_completed,
                records,
                bytes,
                sha256: result
                    .is_ok()
                    .then(|| Sha256Digest::from_bytes(digest.finalize().into())),
                dropped,
                writer_failed: result.is_err(),
            };
            write(&work, "development-card-calls.closed.json", &closure)?;
            result?;
            ensure!(
                complete,
                "private development card capture was empty, lossy or unsettled"
            );
            Ok(())
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

    pub(super) async fn finish(self, service_completed: bool) -> Result<()> {
        let _ = self.stop.send(service_completed);
        self.task
            .await
            .context("private card recorder task closed without a result")?
    }
}
