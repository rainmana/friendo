use friendo::provider::{Message, ProviderConfig, request_body, stream};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::mpsc,
};
use tokio_util::sync::CancellationToken;

type Requests = Arc<Mutex<Vec<(String, Value)>>>;

async fn fixture(responses: Vec<String>, fragment_size: usize) -> (String, Requests) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();
    tokio::spawn(async move {
        for body in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let boundary;
            loop {
                let mut buffer = [0u8; 1024];
                let read = socket.read(&mut buffer).await.unwrap();
                assert!(read > 0);
                bytes.extend_from_slice(&buffer[..read]);
                if let Some(index) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
                    boundary = index + 4;
                    break;
                }
            }
            let headers = std::str::from_utf8(&bytes[..boundary]).unwrap();
            let path = headers
                .lines()
                .next()
                .unwrap()
                .split_whitespace()
                .nth(1)
                .unwrap()
                .to_owned();
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse().unwrap())
                })
                .unwrap();
            while bytes.len() - boundary < length {
                let mut buffer = [0u8; 1024];
                let read = socket.read(&mut buffer).await.unwrap();
                assert!(read > 0);
                bytes.extend_from_slice(&buffer[..read]);
            }
            let request: Value =
                serde_json::from_slice(&bytes[boundary..boundary + length]).unwrap();
            captured.lock().unwrap().push((path, request));
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            if socket.write_all(header.as_bytes()).await.is_err() {
                continue;
            }
            for fragment in body.as_bytes().chunks(fragment_size) {
                if socket.write_all(fragment).await.is_err() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        }
    });
    (format!("http://{address}/v1"), requests)
}

fn config(endpoint: String, protocol: &str) -> ProviderConfig {
    ProviderConfig {
        endpoint,
        protocol: protocol.into(),
        model: "fixture-explicit-model".into(),
        ..ProviderConfig::default()
    }
}
fn tools() -> Value {
    json!([{"type":"function","function":{"name":"inspect","description":"Inspect a fixture","parameters":{"type":"object","properties":{"path":{"type":"string"}}}}}])
}
fn messages() -> Vec<Message> {
    vec![Message {
        role: "user".into(),
        content: json!("Inspect the fixture"),
    }]
}
fn data(value: Value) -> String {
    format!("data: {value}\r\n\r\n")
}
fn oa(delta: Value, finish: Value) -> String {
    data(json!({"choices":[{"index":0,"delta":delta,"finish_reason":finish}]}))
}
fn native(value: Value) -> String {
    format!(
        "event: {}\ndata: {}\n\n",
        value["type"].as_str().unwrap(),
        value
    )
}
fn native_start() -> String {
    native(
        json!({"type":"message_start","message":{"role":"assistant","content":[],"usage":{"input_tokens":7,"output_tokens":0}}}),
    )
}
fn native_stop(reason: &str) -> String {
    native(
        json!({"type":"message_delta","delta":{"stop_reason":reason},"usage":{"output_tokens":9}}),
    ) + &native(json!({"type":"message_stop"}))
}

#[tokio::test]
async fn fragmented_openai_tool_call_roundtrip_uses_real_http() {
    let response = oa(
        json!({"role":"assistant","content":"Inspecting \u{2603} ","tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"ins","arguments":"{\"pa"}}]}),
        Value::Null,
    ) + &oa(
        json!({"tool_calls":[{"index":0,"function":{"name":"pect","arguments":"th\":\"fixture\"}"}}]}),
        Value::Null,
    ) + &oa(json!({}), json!("tool_calls"))
        + &data(json!({"choices":[],"usage":{"total_tokens":12}}))
        + "data: [DONE]\r\n\r\n";
    let final_reply = oa(json!({"content":"Complete"}), Value::Null)
        + &oa(json!({}), json!("stop"))
        + "data: [DONE]\n\n";
    let (endpoint, requests) = fixture(vec![response, final_reply], 1).await;
    let config = config(endpoint, "openai");
    let (tx, mut rx) = mpsc::channel(1);
    let reply = stream(&config, &messages(), &tools(), CancellationToken::new(), tx)
        .await
        .unwrap();
    assert_eq!(reply.text, "Inspecting \u{2603} ");
    assert_eq!(rx.recv().await.as_deref(), Some("Inspecting \u{2603} "));
    assert_eq!(reply.tool_calls[0].name, "inspect");
    assert_eq!(reply.tool_calls[0].arguments, json!({"path":"fixture"}));
    assert_eq!(reply.usage.unwrap()["total_tokens"], 12);
    let mut history = messages();
    history.push(Message{role:"assistant".into(),content:json!({"text":reply.text,"tool_calls":reply.tool_calls,"native":reply.continuation})});
    history.push(Message {
        role: "tool".into(),
        content: json!({"tool_call_id":"call_1","content":"fixture output"}),
    });
    let (tx, _) = mpsc::channel(1);
    let second = stream(&config, &history, &tools(), CancellationToken::new(), tx)
        .await
        .unwrap();
    assert_eq!(second.text, "Complete");
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].0, "/v1/chat/completions");
    assert_eq!(
        requests[1].1["messages"][1]["tool_calls"][0]["id"],
        "call_1"
    );
    assert_eq!(
        requests[1].1["messages"][1]["tool_calls"][0]["function"]["arguments"],
        "{\"path\":\"fixture\"}"
    );
    assert_eq!(requests[1].1["messages"][2]["tool_call_id"], "call_1");
    assert!(!requests[1].1.to_string().contains("api_key"));
}

#[tokio::test]
async fn native_thinking_signature_and_tool_result_survive_roundtrip() {
    let mut response = native_start();
    for event in [
        json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Retained reasoning"}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"signed-fixture"}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_1","name":"inspect","input":{}}}),
        json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"path\":"}}),
        json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"\"fixture\"}"}}),
        json!({"type":"content_block_stop","index":1}),
    ] {
        response += &native(event);
    }
    response += &native_stop("tool_use");
    let final_response = native_start()
        + &native(
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
        )
        + &native(
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Verified"}}),
        )
        + &native(json!({"type":"content_block_stop","index":0}))
        + &native_stop("end_turn");
    let (endpoint, requests) = fixture(vec![response, final_response], 3).await;
    let config = config(endpoint, "anthropic");
    let (tx, _) = mpsc::channel(1);
    let reply = stream(&config, &messages(), &tools(), CancellationToken::new(), tx)
        .await
        .unwrap();
    assert_eq!(
        reply.continuation["content"][0]["signature"],
        "signed-fixture"
    );
    assert_eq!(reply.usage.as_ref().unwrap()["input_tokens"], 7);
    assert_eq!(reply.usage.as_ref().unwrap()["output_tokens"], 9);
    let mut history = messages();
    history.push(Message{role:"assistant".into(),content:json!({"text":reply.text,"tool_calls":reply.tool_calls,"native":reply.continuation})});
    history.push(Message {
        role: "tool".into(),
        content: json!({"tool_call_id":"toolu_1","content":"fixture output"}),
    });
    let (tx, _) = mpsc::channel(1);
    let second = stream(&config, &history, &tools(), CancellationToken::new(), tx)
        .await
        .unwrap();
    assert_eq!(second.text, "Verified");
    let requests = requests.lock().unwrap();
    assert_eq!(requests[0].0, "/v1/messages");
    assert_eq!(requests[0].1["tools"][0]["input_schema"]["type"], "object");
    assert_eq!(
        requests[1].1["messages"][1]["content"][0]["thinking"],
        "Retained reasoning"
    );
    assert_eq!(
        requests[1].1["messages"][1]["content"][0]["signature"],
        "signed-fixture"
    );
    assert_eq!(
        requests[1].1["messages"][2]["content"][0]["tool_use_id"],
        "toolu_1"
    );
}

#[tokio::test]
async fn partial_or_malformed_tool_calls_and_truncation_never_return_tools() {
    let call = |arguments: &str| {
        oa(
            json!({"tool_calls":[{"index":0,"id":"call_1","function":{"name":"inspect","arguments":arguments}}]}),
            Value::Null,
        )
    };
    let responses = vec![
        call("{\"path\":"), // EOF before finish
        call("{") + &oa(json!({}), json!("tool_calls")) + "data: [DONE]\n\n",
        call("{}") + &oa(json!({}), json!("length")) + "data: [DONE]\n\n",
        call("{}") + &oa(json!({}), json!("stop")) + "data: [DONE]\n\n",
        call("{}") + &oa(json!({}), json!("tool_calls")), // EOF even after finish
        "data: [DONE]\n\n".into(),
        "data: {malformed}\n\n".into(),
        call("[]") + &oa(json!({}), json!("tool_calls")) + "data: [DONE]\n\n",
        call("{}") + &data(json!({"error":{"message":"private response body"}})),
    ];
    let count = responses.len();
    let (endpoint, _) = fixture(responses, 7).await;
    for _ in 0..count {
        let (tx, _) = mpsc::channel(1);
        let error = stream(
            &config(endpoint.clone(), "openai"),
            &messages(),
            &tools(),
            CancellationToken::new(),
            tx,
        )
        .await
        .unwrap_err();
        assert!(!error.to_string().contains("private response body"));
    }
}

#[tokio::test]
async fn native_incomplete_blocks_and_truncation_are_rejected() {
    let block = native(
        json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_1","name":"inspect","input":{}}}),
    );
    let responses = vec![
        native_start() + &block + &native_stop("tool_use"), // no block stop
        native_start() + &native_stop("max_tokens"),
        native_start() + &native_stop("tool_use"), // no tool block
        native_start() + &native(json!({"type":"content_block_stop","index":0})),
        native_start()
            + &block
            + &native(
                json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{"}}),
            )
            + &native(json!({"type":"content_block_stop","index":0}))
            + &native_stop("tool_use"),
    ];
    let count = responses.len();
    let (endpoint, _) = fixture(responses, 5).await;
    for _ in 0..count {
        let (tx, _) = mpsc::channel(1);
        assert!(
            stream(
                &config(endpoint.clone(), "anthropic"),
                &messages(),
                &tools(),
                CancellationToken::new(),
                tx
            )
            .await
            .is_err()
        );
    }
}

#[tokio::test]
async fn cancellation_interrupts_slow_http_headers_and_body() {
    for send_headers in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            if send_headers {
                socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n").await.unwrap();
            }
            let _ = ready_tx.send(());
            tokio::time::sleep(Duration::from_secs(10)).await;
        });
        let cancel = CancellationToken::new();
        let cancelled = cancel.clone();
        let task = tokio::spawn(async move {
            let (tx, _) = mpsc::channel(1);
            stream(
                &config(endpoint, "openai"),
                &messages(),
                &tools(),
                cancelled,
                tx,
            )
            .await
        });
        ready_rx.await.unwrap();
        cancel.cancel();
        let result = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
        assert!(result.unwrap_err().to_string().contains("cancelled"));
        server.abort();
    }
}

#[tokio::test]
async fn tool_call_remains_unavailable_until_terminal_event_arrives() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let (sent_tx, sent_rx) = tokio::sync::oneshot::channel();
    let prefix = oa(
        json!({"tool_calls":[{"index":0,"id":"call_1","function":{"name":"inspect","arguments":"{}"}}]}),
        Value::Null,
    ) + &oa(json!({}), json!("tool_calls"));
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buffer = [0u8; 4096];
        let read = socket.read(&mut buffer).await.unwrap();
        assert!(read > 0);
        let suffix = "data: [DONE]\n\n";
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n",prefix.len()+suffix.len()).as_bytes()).await.unwrap();
        socket.write_all(prefix.as_bytes()).await.unwrap();
        let _ = sent_tx.send(());
        release_rx.await.unwrap();
        socket.write_all(suffix.as_bytes()).await.unwrap();
    });
    let mut task = tokio::spawn(async move {
        let (tx, _) = mpsc::channel(1);
        stream(
            &config(endpoint, "openai"),
            &messages(),
            &tools(),
            CancellationToken::new(),
            tx,
        )
        .await
    });
    sent_rx.await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(30), &mut task)
            .await
            .is_err()
    );
    release_tx.send(()).unwrap();
    let reply = task.await.unwrap().unwrap();
    assert_eq!(reply.tool_calls[0].id, "call_1");
    server.await.unwrap();
}

// A subprocess supplies the fake credential without mutating the multithreaded
// test process's environment. The fixture checks headers without retaining them.
#[test]
fn credentials_stay_in_protocol_headers_and_errors_do_not_echo_response_bodies() {
    const CHILD: &str = "FRIENDO_PROVIDER_AUTH_FIXTURE_CHILD";
    const KEY: &str = "FRIENDO_PROVIDER_AUTH_FIXTURE_KEY";
    if std::env::var_os(CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "credentials_stay_in_protocol_headers_and_errors_do_not_echo_response_bodies",
            ])
            .env(CHILD, "1")
            .env(KEY, "non-secret-fixture-credential")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        for protocol in ["openai","anthropic"] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}/v1",listener.local_addr().unwrap());
            let native = protocol == "anthropic";
            let server = tokio::spawn(async move {
                let (mut socket,_) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                loop {
                    let mut buffer = [0u8; 4096];
                    let read = socket.read(&mut buffer).await.unwrap();
                    assert!(read > 0); bytes.extend_from_slice(&buffer[..read]);
                    if bytes.windows(4).any(|part|part == b"\r\n\r\n") { break; }
                }
                let request = std::str::from_utf8(&bytes).unwrap().to_ascii_lowercase();
                assert_eq!(request.contains("x-api-key:"),native);
                assert_eq!(request.contains("authorization: bearer "),!native);
                assert_eq!(request.contains("anthropic-version:"),native);
                let secret = std::env::var(KEY).unwrap();
                let response = format!("HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",secret.len(),secret);
                socket.write_all(response.as_bytes()).await.unwrap();
            });
            let mut cfg = config(endpoint,protocol); cfg.api_key_env = Some(KEY.into());
            let body = request_body(&cfg,&messages(),&tools()).unwrap();
            assert!(!body.to_string().contains(&std::env::var(KEY).unwrap()));
            let (tx,_) = mpsc::channel(1);
            let error = stream(&cfg,&messages(),&tools(),CancellationToken::new(),tx).await.unwrap_err().to_string();
            assert!(error.contains("401"));
            assert!(!error.contains(&std::env::var(KEY).unwrap()));
            server.await.unwrap();
        }
    });
}

#[tokio::test]
async fn oversized_event_is_rejected_and_presentation_backpressure_is_disposable() {
    let response = "data: ".to_owned() + &"x".repeat(1024 * 1024) + "\n\n";
    let (endpoint, _) = fixture(vec![response], 8192).await;
    let (tx, _) = mpsc::channel(1);
    assert!(
        stream(
            &config(endpoint, "openai"),
            &messages(),
            &tools(),
            CancellationToken::new(),
            tx
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("size limit")
    );
    let response = (0..20)
        .map(|_| oa(json!({"content":"x"}), Value::Null))
        .collect::<String>()
        + &oa(json!({}), json!("stop"))
        + "data: [DONE]\n\n";
    let (endpoint, _) = fixture(vec![response], 4096).await;
    let (tx, _rx) = mpsc::channel(1); // Held full until after completion.
    let reply = tokio::time::timeout(
        Duration::from_secs(2),
        stream(
            &config(endpoint, "openai"),
            &messages(),
            &json!([]),
            CancellationToken::new(),
            tx,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(reply.text, "x".repeat(20));
}

#[test]
fn configuration_has_explicit_capabilities_and_no_guessed_model() {
    assert!(ProviderConfig::default().validate().is_err());
    let mut cfg = config("http://localhost:8080/v1".into(), "openai");
    cfg.validate().unwrap();
    assert_eq!(cfg.capabilities()["images"], false);
    cfg.options = json!({"unknown_option":"not silently ignored"});
    assert!(cfg.validate().is_err());
    cfg.options = json!({"output_token_field":"max_completion_tokens","reasoning_effort":"low","include_usage":true});
    let body = request_body(&cfg, &messages(), &json!([])).unwrap();
    assert!(body.get("max_tokens").is_none());
    assert_eq!(body["max_completion_tokens"], 4096);
    assert_eq!(body["stream_options"]["include_usage"], true);
    cfg.endpoint = "https://secret:private@example.com/v1?api_key=private".into();
    let error = cfg.validate().unwrap_err().to_string();
    assert!(!error.contains("private"));
    cfg = config("http://localhost:8080/v1".into(), "anthropic");
    cfg.options = json!({"response_format":{"type":"json_object"}});
    assert!(cfg.validate().is_err());
}

#[test]
fn native_parallel_tool_results_are_one_user_turn() {
    let history = vec![
        Message {
            role: "system".into(),
            content: json!("Explicit system text"),
        },
        Message {
            role: "user".into(),
            content: json!("Inspect"),
        },
        Message {
            role: "assistant".into(),
            content: json!({"text":"","tool_calls":[{"id":"one","name":"inspect","arguments":{}},{"id":"two","name":"inspect","arguments":{}}]}),
        },
        Message {
            role: "tool".into(),
            content: json!({"tool_call_id":"one","content":"first"}),
        },
        Message {
            role: "tool".into(),
            content: json!({"tool_call_id":"two","content":"second"}),
        },
    ];
    let body = request_body(
        &config("http://localhost:8080/v1".into(), "anthropic"),
        &history,
        &tools(),
    )
    .unwrap();
    assert_eq!(body["messages"].as_array().unwrap().len(), 3);
    assert_eq!(body["messages"][2]["content"].as_array().unwrap().len(), 2);
    assert_eq!(body["system"][0]["text"], "Explicit system text");
}
