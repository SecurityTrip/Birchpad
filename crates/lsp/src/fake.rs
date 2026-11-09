//! A scripted server in the same process, for tests: it answers `initialize` with the
//! capabilities it was given and `shutdown` with nothing, other requests with the handlers it
//! was given, records every message, and sends what the test tells it to.

use std::collections::HashMap;
use std::io::{BufReader, PipeWriter};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::client::Transport;
use crate::framing::{read_message, write_message};
use crate::message::{Message, ResponseError};

type Handler = Box<dyn Fn(&Value) -> Result<Value, ResponseError> + Send>;

/// The server before it starts.
pub struct FakeServer {
    capabilities: Value,
    handlers: HashMap<String, Handler>,
}

/// The running server, seen from the test.
#[derive(Clone)]
pub struct FakeHandle {
    received: Arc<Mutex<Vec<Message>>>,
    writer: Arc<Mutex<Option<PipeWriter>>>,
}

impl FakeServer {
    /// A server with these capabilities (`ServerCapabilities` as JSON).
    pub fn new(capabilities: Value) -> Self {
        Self {
            capabilities,
            handlers: HashMap::new(),
        }
    }

    /// Answers requests of `method` with `handler`.
    pub fn on(
        mut self,
        method: &str,
        handler: impl Fn(&Value) -> Result<Value, ResponseError> + Send + 'static,
    ) -> Self {
        self.handlers.insert(method.to_owned(), Box::new(handler));
        self
    }

    /// Starts it on a thread of its own; the transport is the client's end.
    pub fn start(self) -> (Transport, FakeHandle) {
        let (client_reader, server_writer) = std::io::pipe().expect("a pipe");
        let (server_reader, client_writer) = std::io::pipe().expect("a pipe");
        let handle = FakeHandle {
            received: Arc::default(),
            writer: Arc::new(Mutex::new(Some(server_writer))),
        };
        let server = handle.clone();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(server_reader);
            while let Ok(Some(content)) = read_message(&mut reader) {
                let Some(message) = Message::from_bytes(&content) else {
                    continue;
                };
                server.received.lock().unwrap().push(message.clone());
                match message {
                    Message::Request { id, method, params } => {
                        let result = match method.as_str() {
                            "initialize" => Ok(json!({ "capabilities": self.capabilities })),
                            "shutdown" => Ok(Value::Null),
                            _ => match self.handlers.get(&method) {
                                Some(handler) => handler(&params),
                                None => Err(ResponseError::new(
                                    ResponseError::METHOD_NOT_FOUND,
                                    format!("{method} is not scripted"),
                                )),
                            },
                        };
                        server.send(&Message::Response { id, result });
                    }
                    Message::Notification { method, .. } if method == "exit" => break,
                    _ => {}
                }
            }
            server.close();
        });
        (
            Transport {
                reader: Box::new(client_reader),
                writer: Box::new(client_writer),
                child: None,
            },
            handle,
        )
    }
}

impl FakeHandle {
    fn send(&self, message: &Message) {
        if let Some(writer) = self.writer.lock().unwrap().as_mut() {
            let _ = write_message(writer, &message.to_bytes());
        }
    }

    /// Sends a notification to the client.
    pub fn notify(&self, method: &str, params: Value) {
        self.send(&Message::Notification {
            method: method.to_owned(),
            params,
        });
    }

    /// Sends a request to the client, with id `id`.
    pub fn request(&self, id: i64, method: &str, params: Value) {
        self.send(&Message::Request {
            id: crate::message::RequestId::Number(id),
            method: method.to_owned(),
            params,
        });
    }

    /// Sends raw bytes, as a broken server would.
    pub fn send_raw(&self, bytes: &[u8]) {
        use std::io::Write as _;
        if let Some(writer) = self.writer.lock().unwrap().as_mut() {
            let _ = writer.write_all(bytes);
            let _ = writer.flush();
        }
    }

    /// Ends the server's output, as a server that exits.
    pub fn close(&self) {
        self.writer.lock().unwrap().take();
    }

    /// Every message received so far.
    pub fn received(&self) -> Vec<Message> {
        self.received.lock().unwrap().clone()
    }

    /// The parameters of the notifications of `method` received so far.
    pub fn notifications(&self, method: &str) -> Vec<Value> {
        self.received()
            .into_iter()
            .filter_map(|message| match message {
                Message::Notification {
                    method: name,
                    params,
                } if name == method => Some(params),
                _ => None,
            })
            .collect()
    }

    /// The parameters of the requests of `method` received so far.
    pub fn requests(&self, method: &str) -> Vec<Value> {
        self.received()
            .into_iter()
            .filter_map(|message| match message {
                Message::Request {
                    method: name,
                    params,
                    ..
                } if name == method => Some(params),
                _ => None,
            })
            .collect()
    }

    /// The responses received so far (to the server's own requests).
    pub fn responses(&self) -> Vec<Message> {
        self.received()
            .into_iter()
            .filter(|message| matches!(message, Message::Response { .. }))
            .collect()
    }

    /// Waits up to five seconds for `done`.
    pub fn wait_for(&self, done: impl Fn(&Self) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if done(self) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        done(self)
    }
}
