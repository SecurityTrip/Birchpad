//! The client's side of a connection to a server: a thread that writes the messages sent, a
//! thread that reads the server's, answers matched to their requests, the server's own
//! requests answered at once, and its notifications passed on.

use std::collections::HashMap;
use std::io::{self, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use futures::channel::{mpsc as async_mpsc, oneshot};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::command_line::CommandLine;
use crate::framing::{read_message, write_message};
use crate::message::{Message, RequestId, ResponseError};

/// How long a server has to answer `shutdown`, and then to exit, before it is killed.
pub const STOP_TIMEOUT: Duration = Duration::from_secs(2);

/// The two ends of a connection, and the server's process when it is one.
pub struct Transport {
    pub reader: Box<dyn Read + Send>,
    pub writer: Box<dyn Write + Send>,
    pub child: Option<Child>,
}

impl Transport {
    /// Starts `command` in `folder`, talking through its standard input and output. What it
    /// writes to its standard error is dropped.
    pub fn spawn(command: &CommandLine, folder: &Path) -> io::Result<Self> {
        let mut process = Command::new(&command.program);
        process
            .args(&command.args)
            .current_dir(folder)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            // CREATE_NO_WINDOW: no console flashes up for the server.
            process.creation_flags(0x0800_0000);
        }
        let mut child = process.spawn()?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return Err(io::Error::other(
                "the server's input and output are not piped",
            ));
        };
        Ok(Self {
            reader: Box::new(stdout),
            writer: Box::new(stdin),
            child: Some(child),
        })
    }
}

/// What the server says on its own.
#[derive(Debug, Clone, PartialEq)]
pub enum ServerEvent {
    Notification {
        method: String,
        params: Value,
    },
    /// The server's output ended or could not be read: it exited, or broke the protocol.
    Exited {
        reason: String,
    },
}

/// Why a request got no result.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RequestError {
    #[error("{}", t!("lsp.server-answered", error = .0))]
    Server(ResponseError),
    #[error("{}", t!("lsp.server-not-running"))]
    Exited,
    #[error("{}", t!("lsp.answer-unreadable", reason = .0))]
    Invalid(String),
}

type Responder = Box<dyn FnOnce(Result<Value, RequestError>) + Send>;

struct Inner {
    outgoing: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
    pending: Mutex<HashMap<i64, Responder>>,
    next_id: AtomicI64,
    child: Mutex<Option<Child>>,
    exited: AtomicBool,
}

impl Inner {
    fn send(&self, message: &Message) -> bool {
        let outgoing = self.outgoing.lock().unwrap_or_else(|e| e.into_inner());
        outgoing
            .as_ref()
            .is_some_and(|sender| sender.send(message.to_bytes()).is_ok())
    }

    /// The server is gone: every request waiting fails.
    fn close(&self) {
        self.exited.store(true, Ordering::SeqCst);
        self.outgoing
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        let pending: Vec<Responder> = self
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain()
            .map(|(_, responder)| responder)
            .collect();
        for responder in pending {
            responder(Err(RequestError::Exited));
        }
    }
}

/// A connection to a running server; clones share it.
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

impl Client {
    /// Starts talking over `transport`. The receiver gets the server's notifications, and
    /// [`ServerEvent::Exited`] last.
    pub fn start(transport: Transport) -> (Self, async_mpsc::UnboundedReceiver<ServerEvent>) {
        let (outgoing, to_write) = mpsc::channel::<Vec<u8>>();
        let inner = Arc::new(Inner {
            outgoing: Mutex::new(Some(outgoing)),
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicI64::new(1),
            child: Mutex::new(transport.child),
            exited: AtomicBool::new(false),
        });
        let mut writer = transport.writer;
        std::thread::Builder::new()
            .name("language server writer".into())
            .spawn(move || {
                for message in to_write {
                    if write_message(&mut writer, &message).is_err() {
                        break;
                    }
                }
            })
            .ok();
        let (events, received) = async_mpsc::unbounded();
        let reader_inner = inner.clone();
        let reader = transport.reader;
        std::thread::Builder::new()
            .name("language server reader".into())
            .spawn(move || read_loop(reader, &reader_inner, &events))
            .ok();
        (Self { inner }, received)
    }

    /// Whether the server's output ended.
    pub fn has_exited(&self) -> bool {
        self.inner.exited.load(Ordering::SeqCst)
    }

    /// Sends a request; `respond` gets its result, or the reason there is none.
    fn send_request(&self, method: &str, params: Value, respond: Responder) {
        if self.has_exited() {
            respond(Err(RequestError::Exited));
            return;
        }
        let id = self.inner.next_id.fetch_add(1, Ordering::SeqCst);
        self.inner
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, respond);
        let message = Message::Request {
            id: RequestId::Number(id),
            method: method.to_owned(),
            params,
        };
        if !self.inner.send(&message) {
            let respond = self
                .inner
                .pending
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&id);
            if let Some(respond) = respond {
                respond(Err(RequestError::Exited));
            }
        }
    }

    /// Sends a request; the future is its result.
    pub fn request(
        &self,
        method: &str,
        params: Value,
    ) -> impl Future<Output = Result<Value, RequestError>> + Send + 'static {
        let (sender, receiver) = oneshot::channel();
        self.send_request(
            method,
            params,
            Box::new(move |result| {
                let _ = sender.send(result);
            }),
        );
        async move { receiver.await.unwrap_or(Err(RequestError::Exited)) }
    }

    /// Sends the request `R`; the future is its result, read as `R`'s.
    pub fn send<R>(
        &self,
        params: R::Params,
    ) -> impl Future<Output = Result<R::Result, RequestError>> + Send + 'static
    where
        R: lsp_types::request::Request,
        R::Result: Send + 'static,
    {
        let params = serde_json::to_value(params).unwrap_or(Value::Null);
        let answer = self.request(R::METHOD, params);
        async move {
            let value = answer.await?;
            serde_json::from_value(value).map_err(|error| RequestError::Invalid(error.to_string()))
        }
    }

    /// Sends a notification; false when the server is gone.
    pub fn notify(&self, method: &str, params: impl Serialize) -> bool {
        self.inner.send(&Message::Notification {
            method: method.to_owned(),
            params: serde_json::to_value(params).unwrap_or(Value::Null),
        })
    }

    /// Sends the notification `N`.
    pub fn notify_with<N: lsp_types::notification::Notification>(&self, params: N::Params) -> bool {
        self.notify(N::METHOD, params)
    }

    /// Asks the server to stop, on a thread of its own: `shutdown`, then `exit`, and the
    /// process killed if it is still running [`STOP_TIMEOUT`] after either.
    pub fn stop(&self) {
        let client = self.clone();
        std::thread::Builder::new()
            .name("language server stop".into())
            .spawn(move || client.stop_now())
            .ok();
    }

    /// [`Client::stop`] on the calling thread, which waits for it.
    pub fn stop_now(&self) {
        let (sender, receiver) = mpsc::channel();
        self.send_request(
            "shutdown",
            Value::Null,
            Box::new(move |result| {
                let _ = sender.send(result);
            }),
        );
        let _ = receiver.recv_timeout(STOP_TIMEOUT);
        self.notify("exit", Value::Null);
        let child = self
            .inner
            .child
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(mut child) = child {
            let deadline = Instant::now() + STOP_TIMEOUT;
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    _ => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break;
                    }
                }
            }
        }
        self.inner.close();
    }
}

/// Reads the server's messages until its output ends.
fn read_loop(
    reader: Box<dyn Read + Send>,
    inner: &Inner,
    events: &async_mpsc::UnboundedSender<ServerEvent>,
) {
    let mut reader = BufReader::new(reader);
    let reason = loop {
        let content = match read_message(&mut reader) {
            Ok(Some(content)) => content,
            Ok(None) => break t!("lsp.server-closed-output"),
            Err(error) => break t!("lsp.server-output-unreadable", reason = error),
        };
        match Message::from_bytes(&content) {
            Some(Message::Response { id, result }) => {
                let RequestId::Number(id) = id else { continue };
                let respond = inner
                    .pending
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&id);
                if let Some(respond) = respond {
                    respond(result.map_err(RequestError::Server));
                }
            }
            Some(Message::Request { id, method, params }) => {
                let result = answer_server_request(&method, &params);
                inner.send(&Message::Response { id, result });
            }
            Some(Message::Notification { method, params }) => {
                let _ = events.unbounded_send(ServerEvent::Notification { method, params });
            }
            // Not a message: left alone, as the protocol asks of what it does not know.
            None => {}
        }
    };
    inner.close();
    let _ = events.unbounded_send(ServerEvent::Exited { reason });
}

/// The answer to a request the server makes: nothing configured, registrations and progress
/// accepted, edits of the workspace refused (ADR 0065).
pub fn answer_server_request(method: &str, params: &Value) -> Result<Value, ResponseError> {
    match method {
        "workspace/configuration" => {
            let count = params
                .get("items")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            Ok(Value::Array(vec![Value::Null; count]))
        }
        "client/registerCapability"
        | "client/unregisterCapability"
        | "window/workDoneProgress/create"
        | "window/showMessageRequest"
        | "workspace/semanticTokens/refresh"
        | "workspace/inlayHint/refresh"
        | "workspace/codeLens/refresh"
        | "workspace/diagnostic/refresh" => Ok(Value::Null),
        "workspace/applyEdit" => Ok(json!({
            "applied": false,
            "failureReason": "Birchpad does not apply edits a server starts",
        })),
        "workspace/workspaceFolders" => Ok(Value::Null),
        _ => Err(ResponseError::new(
            ResponseError::METHOD_NOT_FOUND,
            format!("{method} is not handled"),
        )),
    }
}

/// Reads `value` as `T`, for the parameters of a notification.
pub fn params<T: DeserializeOwned>(value: &Value) -> Option<T> {
    serde_json::from_value(value.clone()).ok()
}
