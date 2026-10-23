// SPDX-License-Identifier: Apache-2.0
//! Bounded native embedding child adapter; no shell, network or executable PATH lookup.
use crate::{
    runtime_pair,
    service_ipc::{read_frame_limited, write_frame_limited},
};
use airk_retrieval::embedding_worker::{
    self, Connector, MAX_FRAME_BYTES, ModelLocation, PROTOCOL, RemoteSession, Request, Response,
};
use anyhow::{Context, Result, bail, ensure};
use std::{
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

pub fn install() -> Result<()> {
    embedding_worker::install_connector(Arc::new(ProcessConnector))
}

struct ProcessConnector;

impl Connector for ProcessConnector {
    fn tokenizer(&self, _path: &Path) -> Result<Arc<dyn RemoteSession>> {
        Ok(Arc::new(Session::spawn()?))
    }
    fn open(&self, location: ModelLocation, identity: &str) -> Result<Arc<dyn RemoteSession>> {
        let session = Session::spawn()?;
        match session.request(Request::Open {
            protocol: PROTOCOL,
            version: env!("CARGO_PKG_VERSION").into(),
            location,
            identity: identity.into(),
        })? {
            Response::Opened {
                protocol,
                version,
                identity: actual,
            } if protocol == PROTOCOL
                && version == env!("CARGO_PKG_VERSION")
                && actual == identity =>
            {
                Ok(Arc::new(session))
            }
            _ => bail!("embedding worker identity/protocol mismatch"),
        }
    }
}

struct Work {
    request: Request,
    reply: mpsc::SyncSender<Result<Response>>,
}

struct Session {
    child: Arc<Mutex<Child>>,
    sender: mpsc::SyncSender<Work>,
    failed: AtomicBool,
}

impl Session {
    fn spawn() -> Result<Self> {
        let helper = runtime_pair::verify_helper(&std::env::current_exe()?.canonicalize()?)?;
        let mut command = Command::new(helper);
        command
            .arg("worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = crate::native_process::spawn(&mut command)
            .context("start verified embedding worker")?;
        let mut input = child.stdin.take().context("embedding input missing")?;
        let mut output = child.stdout.take().context("embedding output missing")?;
        let child = Arc::new(Mutex::new(child));
        let (sender, receiver) = mpsc::sync_channel::<Work>(1);
        let process = child.clone();
        if let Err(error) = std::thread::Builder::new()
            .name("airk-embed-ipc".into())
            .spawn(move || {
                while let Ok(work) = receiver.recv() {
                    let result = (|| {
                        write_frame_limited(&mut input, &work.request, MAX_FRAME_BYTES)?;
                        read_frame_limited(&mut output, MAX_FRAME_BYTES)?
                            .context("embedding worker disconnected")
                    })();
                    let failed = result.is_err();
                    let _ = work.reply.send(result);
                    if failed {
                        break;
                    }
                }
                if let Ok(mut child) = process.lock() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            })
        {
            if let Ok(mut child) = child.lock() {
                let _ = child.kill();
                let _ = child.wait();
            }
            return Err(error.into());
        }
        Ok(Self {
            child,
            sender,
            failed: AtomicBool::new(false),
        })
    }

    fn stop(&self) {
        self.failed.store(true, Ordering::Release);
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
        }
    }
}

impl RemoteSession for Session {
    fn request(&self, request: Request) -> Result<Response> {
        ensure!(
            !self.failed.load(Ordering::Acquire),
            "embedding worker connection failed"
        );
        let (reply, receiver) = mpsc::sync_channel(1);
        self.sender
            .try_send(Work { request, reply })
            .map_err(|_| anyhow::anyhow!("embedding worker is busy or disconnected"))?;
        match receiver.recv_timeout(Duration::from_secs(120)) {
            Ok(Ok(Response::Failed { reason })) => {
                bail!("embedding worker rejected request: {reason}")
            }
            Ok(result) => {
                if result.is_err() {
                    self.stop();
                }
                result
            }
            Err(_) => {
                self.stop();
                bail!("embedding worker deadline exceeded or disconnected")
            }
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn extract_bundled(directory: &Path) -> Result<()> {
    let session = Session::spawn()?;
    match session.request(Request::Extract {
        directory: directory.canonicalize()?,
    })? {
        Response::Extracted => Ok(()),
        _ => bail!("invalid embedding asset extraction response"),
    }
}
