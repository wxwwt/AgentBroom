//! Bounded stdio transport for an owned, isolated test process. No daemon reuse.
use crate::{Error, fail};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
const BUDGET: usize = 4 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(90);
pub struct Rpc {
    child: Child,
    input: Option<ChildStdin>,
    frames: mpsc::Receiver<Result<Value, Error>>,
    readers: Vec<thread::JoinHandle<()>>,
    cancelled: Arc<AtomicBool>,
    counter: u64,
    pub notifications: Vec<Value>,
}
impl Rpc {
    pub fn start(mut command: Command) -> Result<Self, Error> {
        use std::os::unix::process::CommandExt;
        command
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|_| fail("runtimeUnavailable"))?;
        let input = child.stdin.take();
        let stdout = child.stdout.take().ok_or_else(|| fail("rpcPipe"))?;
        let stderr = child.stderr.take().ok_or_else(|| fail("rpcPipe"))?;
        let (tx, frames) = mpsc::channel();
        let size = Arc::new(AtomicUsize::new(0));
        let out_size = size.clone();
        let out_tx = tx.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let out_cancelled = cancelled.clone();
        let err_cancelled = cancelled.clone();
        let out = thread::spawn(move || {
            let mut pending = Vec::new();
            read_pipe(stdout, &out_cancelled, |chunk| {
                if out_size.fetch_add(chunk.len(), Ordering::Relaxed) + chunk.len() > BUDGET {
                    let _ = out_tx.send(Err(fail("rpcOutputBudget")));
                    return false;
                }
                pending.extend_from_slice(chunk);
                while let Some(end) = pending.iter().position(|b| *b == b'\n') {
                    let line: Vec<_> = pending.drain(..=end).collect();
                    match serde_json::from_slice::<Value>(&line) {
                        Ok(v) => {
                            if out_tx.send(Ok(v)).is_err() {
                                return false;
                            }
                        }
                        Err(_) => {
                            let _ = out_tx.send(Err(fail("rpcInvalidFrame")));
                            return false;
                        }
                    }
                }
                true
            });
        });
        let err = thread::spawn(move || {
            read_pipe(stderr, &err_cancelled, |chunk| {
                if size.fetch_add(chunk.len(), Ordering::Relaxed) + chunk.len() > BUDGET {
                    let _ = tx.send(Err(fail("rpcOutputBudget")));
                    return false;
                }
                true
            });
        });
        let mut rpc = Self {
            child,
            input,
            frames,
            readers: vec![out, err],
            cancelled,
            counter: 0,
            notifications: vec![],
        };
        rpc.request("initialize", json!({"clientInfo":{"name":"agentbroom_cleanup","version":"1"},"capabilities":{"experimentalApi":true}}))?;
        rpc.send(&json!({"method":"initialized"}))?;
        Ok(rpc)
    }
    fn send(&mut self, frame: &Value) -> Result<(), Error> {
        let input = self.input.as_mut().ok_or_else(|| fail("rpcClosed"))?;
        serde_json::to_writer(&mut *input, frame).map_err(|_| fail("rpcWrite"))?;
        input
            .write_all(b"\n")
            .and_then(|_| input.flush())
            .map_err(|_| fail("rpcWrite"))
    }
    fn receive(&self, deadline: Instant) -> Result<Value, Error> {
        self.frames
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|_| fail("rpcTimeoutOrExit"))?
    }
    pub fn request(&mut self, method: &str, params: Value) -> Result<Value, Error> {
        self.counter += 1;
        let id = self.counter;
        self.send(&json!({"id":id,"method":method,"params":params}))?;
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let frame = self.receive(deadline)?;
            if frame.get("method").is_some() && frame.get("id").is_some() {
                return Err(fail("rpcServerRequest"));
            }
            if frame.get("id").and_then(Value::as_u64) == Some(id) {
                if frame.get("error").is_some() {
                    return Err(fail("nativeRejected"));
                }
                return frame
                    .get("result")
                    .cloned()
                    .ok_or_else(|| fail("rpcMissingResult"));
            }
            if frame.get("id").is_some() {
                return Err(fail("rpcUnexpectedResponse"));
            }
            if self.notifications.len() >= 4096 {
                return Err(fail("rpcOutputBudget"));
            }
            self.notifications.push(frame);
        }
    }
    pub fn deleted(&mut self, count: usize) -> Result<Vec<String>, Error> {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let ids: Vec<_> = self
                .notifications
                .iter()
                .filter(|v| v["method"] == "thread/deleted")
                .filter_map(|v| v["params"]["threadId"].as_str().map(str::to_owned))
                .collect();
            if ids.len() >= count {
                return Ok(ids);
            }
            let frame = self.receive(deadline)?;
            if frame.get("id").is_some() {
                return Err(fail("rpcUnexpectedResponse"));
            }
            if self.notifications.len() >= 4096 {
                return Err(fail("rpcOutputBudget"));
            }
            self.notifications.push(frame);
        }
    }
    pub fn version(command: Command) -> Result<String, Error> {
        // Reuse the bounded readers/lifecycle without attempting JSON initialization.
        let mut cmd = command;
        use std::os::unix::process::CommandExt;
        let mut child = cmd
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| fail("runtimeUnavailable"))?;
        let stdout = child.stdout.take().ok_or_else(|| fail("rpcPipe"))?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let reader_cancelled = cancelled.clone();
        let reader = thread::spawn(move || {
            let mut bytes = Vec::new();
            read_pipe(stdout, &reader_cancelled, |chunk| {
                if bytes.len() + chunk.len() > 1024 {
                    return false;
                }
                bytes.extend_from_slice(chunk);
                true
            });
            bytes
        });
        let deadline = Instant::now() + TIMEOUT;
        let result = loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    break if status.success() {
                        Ok(())
                    } else {
                        Err(fail("runtimeVersion"))
                    };
                }
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
                _ => {
                    stop(&mut child);
                    break Err(fail("runtimeVersion"));
                }
            }
        };
        // Let an already exited process drain its small version frame, then bound reader shutdown.
        thread::sleep(Duration::from_millis(100));
        cancelled.store(true, Ordering::Relaxed);
        let bytes = reader.join().map_err(|_| fail("runtimeVersion"))?;
        result?;
        String::from_utf8(bytes).map_err(|_| fail("runtimeVersion"))
    }
}
/// Polling and cancellation also bound shutdown when another process holds a pipe open.
fn read_pipe(
    mut pipe: impl Read + std::os::fd::AsRawFd,
    cancelled: &AtomicBool,
    mut consume: impl FnMut(&[u8]) -> bool,
) {
    let mut buffer = [0u8; 8192];
    while !cancelled.load(Ordering::Relaxed) {
        let mut descriptor = libc::pollfd {
            fd: pipe.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut descriptor, 1, 100) };
        if ready < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            break;
        }
        if ready == 0 {
            continue;
        }
        match pipe.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if !consume(&buffer[..n]) {
                    break;
                }
            }
        }
    }
}
fn stop(child: &mut Child) {
    // Do not signal a reaped group ID, which may already have been reused.
    if matches!(child.try_wait(), Ok(Some(_))) {
        return;
    }
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.wait();
}
impl Drop for Rpc {
    fn drop(&mut self) {
        self.input.take();
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        stop(&mut self.child);
        self.cancelled.store(true, Ordering::Relaxed);
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fake(probe: &str) -> Rpc {
        let mut command = Command::new("/bin/sh");
        command.env_clear().arg("-c").arg(format!(
            "read a; printf '%s\\n' '{{\"id\":1,\"result\":{{}}}}'; read a; read a; {probe}"
        ));
        Rpc::start(command).unwrap()
    }
    #[test]
    fn notification_before_response_is_not_success() {
        let mut rpc = fake(
            "printf '%s\\n' '{\"method\":\"thread/deleted\",\"params\":{\"threadId\":\"synthetic\"}}' '{\"id\":2,\"result\":{}}'",
        );
        assert_eq!(rpc.request("probe", json!({})).unwrap(), json!({}));
        assert_eq!(rpc.deleted(1).unwrap(), vec!["synthetic"]);
    }
    #[test]
    fn server_request_cannot_masquerade_as_response() {
        let mut rpc = fake("printf '%s\\n' '{\"id\":2,\"method\":\"approval\",\"params\":{}}'");
        assert_eq!(
            rpc.request("probe", json!({})).unwrap_err().code,
            "rpcServerRequest"
        );
    }
    #[test]
    fn missing_result_and_rpc_error_are_never_success() {
        for (frame, code) in [
            ("{\"id\":2}", "rpcMissingResult"),
            ("{\"id\":2,\"error\":{\"code\":-1}}", "nativeRejected"),
        ] {
            let mut rpc = fake(&format!("printf '%s\\n' '{frame}'"));
            assert_eq!(rpc.request("probe", json!({})).unwrap_err().code, code);
        }
    }
    #[test]
    fn pipe_reader_can_stop_without_peer_eof() {
        let (pipe, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        let started = Instant::now();
        let reader = thread::spawn(move || read_pipe(pipe, &flag, |_| true));
        cancelled.store(true, Ordering::Relaxed);
        reader.join().unwrap();
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
