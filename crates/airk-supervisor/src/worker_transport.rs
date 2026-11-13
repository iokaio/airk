// SPDX-License-Identifier: Apache-2.0
//! One private service-to-worker control protocol over either carrier.
//!
//! `project_protocol` and `source_protocol` remain the only definitions of the
//! frames. A stdio carrier serializes each value with `service_ipc` framing
//! (unchanged bytes, budgets and EOF rules); an in-process carrier moves the
//! same values through a bounded channel without serializing them. A clean end
//! of stream (EOF, or every sender dropped) is `Ok(None)`; any other failure
//! closes the control connection exactly as a codec failure does.
use crate::service_ipc;
use serde::{Serialize, de::DeserializeOwned};
#[cfg(test)]
use std::io::{Read, Write};
use std::{io, sync::mpsc};

/// Inbound frames for one control connection.
pub(crate) trait FrameSource<T>: Send {
    fn recv(&mut self) -> io::Result<Option<T>>;
}

/// Outbound frames for one control connection. Writes are serialized by the
/// single owner of the sink, as the protocol requires.
pub(crate) trait FrameSink<T>: Send {
    fn send(&mut self, value: T) -> io::Result<()>;
}

/// This process's standard input, locked once per frame like the original
/// role entry points.
pub(crate) struct StdinFrames;
impl<T: DeserializeOwned> FrameSource<T> for StdinFrames {
    fn recv(&mut self) -> io::Result<Option<T>> {
        service_ipc::read_frame(&mut io::stdin().lock())
    }
}

/// This process's standard output, locked once per frame.
pub(crate) struct StdoutFrames;
impl<T: Serialize> FrameSink<T> for StdoutFrames {
    fn send(&mut self, value: T) -> io::Result<()> {
        service_ipc::write_frame(&mut io::stdout().lock(), &value)
    }
}

/// A child's pipe end (or any byte stream) carrying length-prefixed frames.
/// Only child-owner fixtures still use one; the service's owners are in process.
#[cfg(test)]
pub(crate) struct StreamFrames<S>(pub(crate) S);
#[cfg(test)]
impl<T: DeserializeOwned, R: Read + Send> FrameSource<T> for StreamFrames<R> {
    fn recv(&mut self) -> io::Result<Option<T>> {
        service_ipc::read_frame(&mut self.0)
    }
}
#[cfg(test)]
impl<T: Serialize, W: Write + Send> FrameSink<T> for StreamFrames<W> {
    fn send(&mut self, value: T) -> io::Result<()> {
        service_ipc::write_frame(&mut self.0, &value)
    }
}

/// In-process inbound frames. A dropped sender is a clean end of stream.
pub(crate) struct ChannelSource<T>(mpsc::Receiver<T>);
impl<T: Send> FrameSource<T> for ChannelSource<T> {
    fn recv(&mut self) -> io::Result<Option<T>> {
        Ok(self.0.recv().ok())
    }
}

/// In-process outbound frames. Like a full pipe, a full channel blocks the
/// writer; a dropped receiver is a broken pipe.
pub(crate) struct ChannelSink<T>(mpsc::SyncSender<T>);
impl<T: Send> FrameSink<T> for ChannelSink<T> {
    fn send(&mut self, value: T) -> io::Result<()> {
        self.0
            .send(value)
            .map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))
    }
}

/// One direction of an in-process control connection.
pub(crate) fn channel<T>(bound: usize) -> (ChannelSink<T>, ChannelSource<T>) {
    let (sender, receiver) = mpsc::sync_channel(bound);
    (ChannelSink(sender), ChannelSource(receiver))
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_host::project_protocol::{Command, Reply};

    fn exchange(
        mut commands: impl FrameSink<Command>,
        mut source: impl FrameSource<Command>,
        mut replies: impl FrameSink<Reply>,
        mut observed: impl FrameSource<Reply>,
    ) -> io::Result<()> {
        commands.send(Command::Stop)?;
        drop(commands);
        assert!(matches!(source.recv()?, Some(Command::Stop)));
        assert!(source.recv()?.is_none(), "closed carrier must be clean EOF");
        replies.send(Reply::QueriesConfigured)?;
        assert!(matches!(observed.recv()?, Some(Reply::QueriesConfigured)));
        Ok(())
    }

    #[test]
    fn stdio_and_channel_carriers_share_one_protocol_and_eof_rule() -> io::Result<()> {
        let (commands, source) = channel::<Command>(1);
        let (replies, observed) = channel::<Reply>(1);
        exchange(commands, source, replies, observed)?;

        let mut command_bytes = Vec::new();
        FrameSink::<Command>::send(&mut StreamFrames(&mut command_bytes), Command::Stop)?;
        let mut reply_bytes = Vec::new();
        FrameSink::<Reply>::send(
            &mut StreamFrames(&mut reply_bytes),
            Reply::QueriesConfigured,
        )?;
        // Serialized frames are byte-identical to the direct service_ipc codec.
        let mut direct = Vec::new();
        service_ipc::write_frame(&mut direct, &Command::Stop)?;
        assert_eq!(command_bytes, direct);
        let mut stream = StreamFrames(io::Cursor::new(command_bytes));
        assert!(matches!(
            FrameSource::<Command>::recv(&mut stream)?,
            Some(Command::Stop)
        ));
        assert!(FrameSource::<Command>::recv(&mut stream)?.is_none());
        let mut stream = StreamFrames(io::Cursor::new(reply_bytes));
        assert!(matches!(
            FrameSource::<Reply>::recv(&mut stream)?,
            Some(Reply::QueriesConfigured)
        ));
        Ok(())
    }

    #[test]
    fn channel_carrier_reports_a_dropped_reader_as_a_broken_pipe() {
        let (mut replies, observed) = channel::<Reply>(1);
        drop(observed);
        assert_eq!(
            replies.send(Reply::Renewed).unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
    }
}
