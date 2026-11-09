use birchpad_lsp::message::*;
use serde_json::json;

#[test]
fn messages_go_to_json_and_back() {
    let messages = [
        Message::Request {
            id: RequestId::Number(1),
            method: "textDocument/hover".into(),
            params: json!({ "a": 1 }),
        },
        Message::Request {
            id: RequestId::Text("x-1".into()),
            method: "workspace/configuration".into(),
            params: json!({ "items": [] }),
        },
        Message::Response {
            id: RequestId::Number(2),
            result: Ok(json!([1, 2])),
        },
        Message::Response {
            id: RequestId::Number(3),
            result: Err(ResponseError::new(-32601, "no such method")),
        },
        Message::Notification {
            method: "initialized".into(),
            params: json!({}),
        },
    ];
    for message in messages {
        let json = message.to_json();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(Message::from_json(&json), Some(message.clone()));
        assert_eq!(Message::from_bytes(&message.to_bytes()), Some(message));
    }
}

#[test]
fn a_response_may_leave_out_its_null_result() {
    assert_eq!(
        Message::from_json(&json!({ "jsonrpc": "2.0", "id": 4 })),
        Some(Message::Response {
            id: RequestId::Number(4),
            result: Ok(serde_json::Value::Null)
        })
    );
    // An error of null is no error.
    assert_eq!(
        Message::from_json(&json!({ "id": 5, "result": 1, "error": null })),
        Some(Message::Response {
            id: RequestId::Number(5),
            result: Ok(json!(1))
        })
    );
    // An error without its fields still is one.
    assert_eq!(
        Message::from_json(&json!({ "id": 6, "error": {} })),
        Some(Message::Response {
            id: RequestId::Number(6),
            result: Err(ResponseError::new(0, ""))
        })
    );
    // Parameters left out are null.
    assert_eq!(
        Message::from_json(&json!({ "method": "exit" })),
        Some(Message::Notification {
            method: "exit".into(),
            params: serde_json::Value::Null
        })
    );
}

#[test]
fn what_is_no_message_is_none() {
    for value in [
        json!([]),
        json!(1),
        json!({}),
        json!({ "method": 1 }),
        json!({ "id": [1], "result": 1 }),
        json!({ "id": 1.5, "result": 1 }),
        json!({ "id": null, "method": "x" }),
    ] {
        assert_eq!(Message::from_json(&value), None, "{value}");
    }
    assert_eq!(Message::from_bytes(b"not json"), None);
    assert_eq!(Message::from_bytes(b""), None);
}

#[test]
fn errors_say_their_code() {
    let error = ResponseError::new(ResponseError::CONTENT_MODIFIED, "changed");
    assert_eq!(error.to_string(), "changed (-32801)");
    assert_eq!(ResponseError::REQUEST_CANCELLED, -32800);
    assert_eq!(ResponseError::METHOD_NOT_FOUND, -32601);
}
