// SPDX-License-Identifier: Apache-2.0
//! Bounded blocking-handle pumps for the dedicated local helper processes.
//! They never run on the canonical worker. Process exit closes a stuck OS pipe;
//! no blocking stdin task is submitted to Tokio's shutdown-waited pool.
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::{
    mpsc::{sync_channel, SyncSender},
    Arc, Mutex,
};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, watch};

pub(super) const LIMIT: usize = 1024 * 1024;
const QUEUE: usize = 8;
const WRITE_DEADLINE: Duration = Duration::from_secs(5);

pub(super) struct Framed {
    input: mpsc::Receiver<Vec<u8>>,
    output: SyncSender<(Vec<u8>, oneshot::Sender<Result<(), &'static str>>)>,
    lost: watch::Receiver<bool>,
    loss: Arc<Loss>,
}

struct Loss {
    state: Mutex<(bool, Option<Box<dyn Fn() + Send + Sync>>)>,
    changed: watch::Sender<bool>,
}
impl Loss {
    fn register(&self, invalidate: impl Fn() + Send + Sync + 'static) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.0 {
            invalidate();
        } else {
            state.1 = Some(Box::new(invalidate));
        }
    }
    fn close(&self) {
        // Registration and invalidation share this lock: EOF cannot fall between
        // testing the old loss state and installing the host admission fence.
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if !state.0 {
            state.0 = true;
            if let Some(invalidate) = state.1.take() {
                invalidate();
            }
            let _ = self.changed.send(true);
        }
    }
}

fn line(input: &mut impl BufRead) -> Result<Option<Vec<u8>>, &'static str> {
    let mut frame = Vec::new();
    let count = input
        .take((LIMIT + 2) as u64)
        .read_until(b'\n', &mut frame)
        .map_err(|_| "local transport read failed")?;
    if count == 0 {
        return Ok(None);
    }
    if count > LIMIT + 1 || frame.last() != Some(&b'\n') || std::str::from_utf8(&frame).is_err() {
        return Err("invalid or oversized local frame");
    }
    Ok(Some(frame))
}

impl Framed {
    pub(super) fn new(
        input: impl Read + Send + 'static,
        mut output: impl Write + Send + 'static,
    ) -> Result<Self, String> {
        let (send, receive) = mpsc::channel(QUEUE);
        let (write, writes) =
            sync_channel::<(Vec<u8>, oneshot::Sender<Result<(), &'static str>>)>(QUEUE);
        let (lost, loss) = watch::channel(false);
        let closed = Arc::new(Loss {
            state: Mutex::new((false, None)),
            changed: lost,
        });
        let reader_lost = closed.clone();
        let writer_lost = closed.clone();
        std::thread::Builder::new()
            .name("local-frame-reader".into())
            .spawn(move || {
                let mut input = BufReader::new(input);
                while let Ok(Some(frame)) = line(&mut input) {
                    // A slow consumer is disconnected, never allowed to stall
                    // EOF detection indefinitely or allocate another queue.
                    if send.try_send(frame).is_err() {
                        break;
                    }
                }
                reader_lost.close();
            })
            .map_err(|_| "local reader unavailable")?;
        std::thread::Builder::new()
            .name("local-frame-writer".into())
            .spawn(move || {
                while let Ok((frame, reply)) = writes.recv() {
                    let result = output
                        .write_all(&frame)
                        .and_then(|_| output.flush())
                        .map_err(|_| "local transport write failed");
                    let failed = result.is_err();
                    if failed {
                        writer_lost.close();
                    }
                    let _ = reply.send(result);
                    if failed {
                        break;
                    }
                }
                writer_lost.close();
            })
            .map_err(|_| "local writer unavailable")?;
        Ok(Self {
            input: receive,
            output: write,
            lost: loss,
            loss: closed,
        })
    }

    pub(super) fn on_loss(&self, invalidate: impl Fn() + Send + Sync + 'static) {
        self.loss.register(invalidate);
    }

    pub(super) async fn receive(&mut self) -> Result<Vec<u8>, String> {
        if *self.lost.borrow() {
            return Err("local transport closed".into());
        }
        tokio::select! {
            biased;
            _ = self.lost.changed() => Err("local transport closed".into()),
            frame = self.input.recv() => frame.ok_or_else(|| "local transport closed".into()),
        }
    }

    pub(super) fn loss(&self) -> watch::Receiver<bool> {
        self.lost.clone()
    }

    pub(super) async fn send(&self, frame: Vec<u8>) -> Result<(), String> {
        if frame.len() > LIMIT + 1 || frame.last() != Some(&b'\n') || *self.lost.borrow() {
            return Err("local output unavailable".into());
        }
        let (reply, result) = oneshot::channel();
        self.output
            .try_send((frame, reply))
            .map_err(|_| "local output queue full")?;
        let mut lost = self.loss();
        tokio::select! {
            biased;
            _ = lost.wait_for(|lost| *lost) => Err("local transport closed".into()),
            result = tokio::time::timeout(WRITE_DEADLINE, result) => result
                .map_err(|_| "local output stalled")?
                .map_err(|_| "local output closed")?
                .map_err(str::to_owned),
        }
    }

    pub(super) async fn send_value(&self, value: &impl serde::Serialize) -> Result<(), String> {
        let frame = vcp_protocol::jsonrpc::encode_frame(value, LIMIT)
            .map_err(|_| "local frame encoding failed")?;
        self.send(frame).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn broken_output_fences_connection_before_await_returns() {
        use std::sync::atomic::{AtomicBool, Ordering};
        struct OpenInput(std::sync::mpsc::Receiver<()>);
        impl Read for OpenInput {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                let _ = self.0.recv();
                Ok(0)
            }
        }
        struct BrokenOutput;
        impl Write for BrokenOutput {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let (release, input) = std::sync::mpsc::channel();
        let io = Framed::new(OpenInput(input), BrokenOutput).unwrap();
        let invalidated = Arc::new(AtomicBool::new(false));
        let flag = invalidated.clone();
        io.on_loss(move || flag.store(true, Ordering::SeqCst));
        assert!(io.send(b"{}\n".to_vec()).await.is_err());
        assert!(invalidated.load(Ordering::SeqCst));
        drop(release);
    }
    #[test]
    fn loss_invalidates_before_notification_and_late_registration() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let (changed, received) = watch::channel(false);
        let invalidated = Arc::new(AtomicBool::new(false));
        let flag = invalidated.clone();
        let notification = received.clone();
        let loss = Loss {
            state: Mutex::new((
                false,
                Some(Box::new(move || {
                    assert!(
                        !*notification.borrow(),
                        "admission must be fenced before notification"
                    );
                    flag.store(true, Ordering::SeqCst);
                })),
            )),
            changed,
        };
        loss.close();
        assert!(*received.borrow());
        assert!(invalidated.load(Ordering::SeqCst));
        // Duplicate notifications are inert; the callback is not retained after loss.
        assert!(loss.state.lock().unwrap().1.is_none());
        loss.close();
        invalidated.store(false, Ordering::SeqCst);
        let flag = invalidated.clone();
        loss.register(move || flag.store(true, Ordering::SeqCst));
        assert!(
            invalidated.load(Ordering::SeqCst),
            "already-closed transport cannot admit a newly bound connection"
        );
    }
    #[test]
    fn rejects_partial_invalid_and_oversized_frames_without_unbounded_reads() {
        for input in [
            b"partial".to_vec(),
            vec![0xff, b'\n'],
            vec![b'x'; LIMIT + 200],
        ] {
            let mut reader = std::io::Cursor::new(input);
            assert!(line(&mut reader).is_err());
            assert!(reader.position() <= (LIMIT + 2) as u64);
        }
        let mut input = std::io::Cursor::new(b"{\"x\":\"a\\nb\"}\n{}\n");
        assert_eq!(line(&mut input).unwrap().unwrap(), b"{\"x\":\"a\\nb\"}\n");
        assert_eq!(line(&mut input).unwrap().unwrap(), b"{}\n");
        assert!(line(&mut input).unwrap().is_none());
        let mut boundary = vec![b'x'; LIMIT];
        boundary.push(b'\n');
        assert_eq!(
            line(&mut std::io::Cursor::new(&boundary)).unwrap().unwrap(),
            boundary
        );
        boundary.insert(0, b'x');
        assert!(line(&mut std::io::Cursor::new(boundary)).is_err());
    }
}
