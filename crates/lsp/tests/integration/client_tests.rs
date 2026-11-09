use std::time::{Duration, Instant};

use birchpad_lsp::client::*;
use birchpad_lsp::fake::{FakeHandle, FakeServer};
use birchpad_lsp::message::{Message, RequestId, ResponseError};
use birchpad_lsp::{CommandLine, lsp_types};
use futures::StreamExt as _;
use futures::channel::mpsc::UnboundedReceiver;
use futures::executor::block_on;
use serde_json::{Value, json};

fn start(server: FakeServer) -> (Client, UnboundedReceiver<ServerEvent>, FakeHandle) {
    let (transport, handle) = server.start();
    let (client, events) = Client::start(transport);
    (client, events, handle)
}

fn next_event(events: &mut UnboundedReceiver<ServerEvent>) -> Option<ServerEvent> {
    block_on(events.next())
}

#[test]
fn requests_get_their_answers() {
    let server = FakeServer::new(json!({}))
        .on("test/echo", |params| Ok(params.clone()))
        .on("test/fail", |_| Err(ResponseError::new(-32000, "it broke")));
    let (client, _events, handle) = start(server);
    assert_eq!(
        block_on(client.request("test/echo", json!({ "x": [1, "ё"] }))),
        Ok(json!({ "x": [1, "ё"] }))
    );
    assert_eq!(
        block_on(client.request("test/fail", Value::Null)),
        Err(RequestError::Server(ResponseError::new(-32000, "it broke")))
    );
    let unknown = block_on(client.request("test/unknown", Value::Null)).unwrap_err();
    assert!(
        matches!(&unknown, RequestError::Server(error) if error.code == ResponseError::METHOD_NOT_FOUND)
    );
    assert!(unknown.to_string().contains("test/unknown"));
    // Several at once are matched by their ids, whatever order they are awaited in.
    let first = client.request("test/echo", json!(1));
    let second = client.request("test/echo", json!(2));
    assert_eq!(block_on(second), Ok(json!(2)));
    assert_eq!(block_on(first), Ok(json!(1)));
    let ids: Vec<_> = handle
        .received()
        .into_iter()
        .filter_map(|message| match message {
            Message::Request { id, .. } => Some(id),
            _ => None,
        })
        .collect();
    assert_eq!(
        ids,
        (1..=5).map(RequestId::Number).collect::<Vec<_>>(),
        "numbered from 1"
    );
}

#[test]
fn typed_requests_and_notifications() {
    let server = FakeServer::new(json!({})).on("textDocument/hover", |_| {
        Ok(json!({ "contents": { "kind": "plaintext", "value": "a hover" } }))
    });
    let (client, _events, handle) = start(server);
    let params: lsp_types::HoverParams = serde_json::from_value(json!({
        "textDocument": { "uri": "file:///a.rs" },
        "position": { "line": 0, "character": 0 },
    }))
    .unwrap();
    let hover = block_on(client.send::<lsp_types::request::HoverRequest>(params))
        .unwrap()
        .unwrap();
    assert_eq!(
        birchpad_lsp::convert::hover_text(&hover.contents),
        "a hover"
    );
    assert!(
        client.notify_with::<lsp_types::notification::DidCloseTextDocument>(
            lsp_types::DidCloseTextDocumentParams {
                text_document: lsp_types::TextDocumentIdentifier::new(
                    "file:///a.rs".parse().unwrap()
                ),
            }
        )
    );
    assert!(handle.wait_for(|handle| !handle.notifications("textDocument/didClose").is_empty()));
    // An answer that is not the request's result.
    let server = FakeServer::new(json!({})).on("textDocument/hover", |_| Ok(json!([1])));
    let (client, _events, _handle) = start(server);
    let params: lsp_types::HoverParams = serde_json::from_value(json!({
        "textDocument": { "uri": "file:///a.rs" },
        "position": { "line": 0, "character": 0 },
    }))
    .unwrap();
    assert!(matches!(
        block_on(client.send::<lsp_types::request::HoverRequest>(params)),
        Err(RequestError::Invalid(_))
    ));
}

#[test]
fn notifications_from_the_server_and_its_exit() {
    let (client, mut events, handle) = start(FakeServer::new(json!({})));
    handle.notify(
        "window/logMessage",
        json!({ "type": 3, "message": "hello" }),
    );
    assert_eq!(
        next_event(&mut events),
        Some(ServerEvent::Notification {
            method: "window/logMessage".into(),
            params: json!({ "type": 3, "message": "hello" })
        })
    );
    // A request waiting when the server exits fails.
    let waiting = client.request("test/never", Value::Null);
    assert!(handle.wait_for(|handle| !handle.requests("test/never").is_empty()));
    handle.close();
    assert!(matches!(
        next_event(&mut events),
        Some(ServerEvent::Exited { .. })
    ));
    assert_eq!(next_event(&mut events), None, "nothing after the exit");
    // The fake answered "test/never" with an error before closing, or not at all.
    assert!(block_on(waiting).is_err());
    assert!(client.has_exited());
    // Afterwards nothing is sent.
    assert_eq!(
        block_on(client.request("test/echo", Value::Null)),
        Err(RequestError::Exited)
    );
    assert!(!client.notify("initialized", json!({})));
    assert!(!RequestError::Exited.to_string().is_empty());
}

#[test]
fn a_server_that_breaks_the_protocol_is_gone() {
    let (client, mut events, handle) = start(FakeServer::new(json!({})));
    // Not a message: skipped. Then a header without a length.
    handle.send_raw(b"Content-Length: 5\r\n\r\nhello");
    handle.send_raw(b"Content-Type: x\r\n\r\n");
    match next_event(&mut events) {
        Some(ServerEvent::Exited { reason }) => {
            assert!(reason.contains("Content-Length"), "{reason}")
        }
        other => panic!("{other:?}"),
    }
    assert!(client.has_exited());
}

#[test]
fn the_servers_requests_are_answered() {
    let (_client, _events, handle) = start(FakeServer::new(json!({})));
    handle.request(
        1,
        "workspace/configuration",
        json!({ "items": [{ "section": "a" }, { "section": "b" }] }),
    );
    handle.request(2, "workspace/applyEdit", json!({ "edit": {} }));
    handle.request(3, "window/workDoneProgress/create", json!({ "token": 1 }));
    handle.request(4, "test/unknown", Value::Null);
    assert!(handle.wait_for(|handle| handle.responses().len() == 4));
    let responses = handle.responses();
    let result = |id: i64| {
        responses
            .iter()
            .find_map(|message| match message {
                Message::Response { id: got, result } if *got == RequestId::Number(id) => {
                    Some(result.clone())
                }
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(result(1), Ok(json!([null, null])));
    assert_eq!(result(2).unwrap()["applied"], false);
    assert_eq!(result(3), Ok(Value::Null));
    assert_eq!(result(4).unwrap_err().code, ResponseError::METHOD_NOT_FOUND);
}

#[test]
fn answers_to_requests_of_servers() {
    assert_eq!(
        answer_server_request("workspace/configuration", &json!({})),
        Ok(json!([]))
    );
    assert_eq!(
        answer_server_request("workspace/configuration", &json!({ "items": "no" })),
        Ok(json!([]))
    );
    for method in [
        "client/registerCapability",
        "client/unregisterCapability",
        "window/showMessageRequest",
        "workspace/workspaceFolders",
        "workspace/diagnostic/refresh",
    ] {
        assert_eq!(
            answer_server_request(method, &Value::Null),
            Ok(Value::Null),
            "{method}"
        );
    }
    assert!(answer_server_request("", &Value::Null).is_err());
}

#[test]
fn stopping_a_server_asks_it_to_shut_down_and_exit() {
    let (client, mut events, handle) = start(FakeServer::new(json!({})));
    client.stop_now();
    assert!(client.has_exited());
    assert!(handle.wait_for(|handle| handle.received().len() == 2));
    let methods: Vec<String> = handle
        .received()
        .into_iter()
        .map(|message| match message {
            Message::Request { method, .. } | Message::Notification { method, .. } => method,
            Message::Response { .. } => "response".into(),
        })
        .collect();
    assert_eq!(methods, ["shutdown", "exit"]);
    // The events end with the exit, once.
    let mut exits = 0;
    while let Some(event) = next_event(&mut events) {
        assert!(matches!(event, ServerEvent::Exited { .. }));
        exits += 1;
    }
    assert!(exits <= 1);
    // Stopping again does nothing more.
    client.stop_now();
    client.stop();
}

#[test]
fn initializes_with_what_birchpad_offers() {
    let capabilities = json!({
        "positionEncoding": "utf-8",
        "textDocumentSync": 1,
        "completionProvider": { "triggerCharacters": [".", "::"] },
        "hoverProvider": true,
        "definitionProvider": {},
        "referencesProvider": true,
        "renameProvider": { "prepareProvider": true },
        "documentFormattingProvider": false,
    });
    let (client, _events, handle) = start(FakeServer::new(capabilities));
    let dir = tempfile::tempdir().unwrap();
    let abilities = block_on(birchpad_lsp::capabilities::initialize(
        &client,
        dir.path(),
        Some(42),
    ))
    .unwrap();
    assert_eq!(abilities.encoding, birchpad_lsp::PositionEncoding::Utf8);
    assert!(abilities.sync && abilities.completion && abilities.hover);
    assert!(abilities.definition && abilities.references && abilities.rename);
    assert!(!abilities.formatting);
    assert_eq!(abilities.trigger_characters, [".", "::"]);
    let params = &handle.requests("initialize")[0];
    assert_eq!(params["processId"], 42);
    assert_eq!(params["clientInfo"]["name"], "Birchpad");
    assert_eq!(
        params["capabilities"]["general"]["positionEncodings"],
        json!(["utf-8", "utf-16"])
    );
    assert_eq!(
        params["capabilities"]["textDocument"]["completion"]["completionItem"]["snippetSupport"],
        false
    );
    let root = params["rootUri"].as_str().unwrap();
    assert!(root.starts_with("file:///"), "{root}");
    assert_eq!(params["workspaceFolders"][0]["uri"], root);
    assert!(handle.wait_for(|handle| handle.notifications("initialized").len() == 1));
}

#[test]
fn an_initialize_answer_without_capabilities_fails() {
    let server = FakeServer::new(json!("not capabilities"));
    let (client, _events, _handle) = start(server);
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        block_on(birchpad_lsp::capabilities::initialize(
            &client,
            dir.path(),
            None
        )),
        Err(RequestError::Invalid(_))
    ));
}

#[test]
fn a_program_that_does_not_exist_cannot_start() {
    let dir = tempfile::tempdir().unwrap();
    let command = CommandLine::parse("birchpad-no-such-language-server --stdio").unwrap();
    assert!(Transport::spawn(&command, dir.path()).is_err());
    // Nor in a folder that does not exist.
    let command = CommandLine {
        program: std::env::current_exe().unwrap().display().to_string(),
        args: vec!["--list".into()],
    };
    assert!(Transport::spawn(&command, &dir.path().join("missing")).is_err());
}

#[test]
fn a_process_that_does_not_speak_the_protocol_is_gone() {
    // The test program lists its tests on its output, which is no message.
    let dir = tempfile::tempdir().unwrap();
    let command = CommandLine {
        program: std::env::current_exe().unwrap().display().to_string(),
        args: vec!["--list".into()],
    };
    let transport = Transport::spawn(&command, dir.path()).unwrap();
    let (client, mut events) = Client::start(transport);
    assert!(matches!(
        next_event(&mut events),
        Some(ServerEvent::Exited { .. })
    ));
    assert!(client.has_exited());
    client.stop_now();
}

#[test]
fn a_process_that_does_not_exit_is_killed() {
    let dir = tempfile::tempdir().unwrap();
    let command = if cfg!(windows) {
        CommandLine::parse("ping -n 60 127.0.0.1").unwrap()
    } else {
        CommandLine::parse("sleep 60").unwrap()
    };
    let transport = Transport::spawn(&command, dir.path()).unwrap();
    let (client, _events) = Client::start(transport);
    let started = Instant::now();
    client.stop_now();
    let took = started.elapsed();
    assert!(took >= STOP_TIMEOUT / 2, "it was given time: {took:?}");
    assert!(took < Duration::from_secs(20), "{took:?}");
    assert!(client.has_exited());
}
