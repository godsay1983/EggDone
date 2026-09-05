use super::*;
use s3::{creds::Credentials, region::Region};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Instant,
};

const EMPTY: &str = r#"{"format_version":1,"rules":[]}"#;

pub(crate) struct Reply {
    status: u16,
    headers: String,
    body: Vec<u8>,
    chunked: bool,
    delay: Duration,
}

impl Reply {
    pub(crate) fn new(status: u16, etag: Option<&str>, body: &[u8]) -> Self {
        Self {
            status,
            headers: etag.map(|v| format!("ETag: {v}\r\n")).unwrap_or_default(),
            body: body.to_vec(),
            chunked: false,
            delay: Duration::ZERO,
        }
    }
}

pub(crate) struct WireRequest {
    pub(crate) head: String,
    pub(crate) body: Vec<u8>,
}

pub(crate) struct Server {
    endpoint: String,
    requests: mpsc::Receiver<WireRequest>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Server {
    pub(crate) fn new(replies: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let (sender, requests) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let worker = thread::spawn(move || {
            for reply in replies {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    if stopped.load(Ordering::Relaxed) || Instant::now() >= deadline {
                        return;
                    }
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2))
                        }
                        Err(error) => panic!("{error}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut data = Vec::new();
                let mut buffer = [0; 8192];
                let boundary = loop {
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    data.extend_from_slice(&buffer[..n]);
                    if let Some(index) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                        break index + 4;
                    }
                    assert!(data.len() < 65536);
                };
                let head = String::from_utf8(data[..boundary].to_vec()).unwrap();
                let length = head
                    .lines()
                    .find_map(|line| {
                        line.split_once(':')
                            .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                            .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                assert!(length <= MAX_RULE_BYTES);
                while data.len() < boundary + length {
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    data.extend_from_slice(&buffer[..n]);
                }
                sender
                    .send(WireRequest {
                        head,
                        body: data[boundary..boundary + length].to_vec(),
                    })
                    .unwrap();
                thread::sleep(reply.delay);
                let framing = if reply.chunked {
                    "Transfer-Encoding: chunked\r\n".to_string()
                } else {
                    format!("Content-Length: {}\r\n", reply.body.len())
                };
                let head = format!(
                    "HTTP/1.1 {} Test\r\n{}{}Connection: close\r\n\r\n",
                    reply.status, reply.headers, framing
                );
                if stream.write_all(head.as_bytes()).is_err() {
                    continue;
                }
                if reply.chunked {
                    for chunk in reply.body.chunks(65536) {
                        if write!(stream, "{:x}\r\n", chunk.len()).is_err()
                            || stream.write_all(chunk).is_err()
                            || stream.write_all(b"\r\n").is_err()
                        {
                            break;
                        }
                    }
                    let _ = stream.write_all(b"0\r\n\r\n");
                } else {
                    let _ = stream.write_all(&reply.body);
                }
            }
        });
        Self {
            endpoint,
            requests,
            stop,
            worker: Some(worker),
        }
    }

    pub(crate) fn bucket(&self) -> Box<Bucket> {
        Bucket::new(
            "rules-test",
            Region::Custom {
                region: "test-region".to_string(),
                endpoint: self.endpoint.clone(),
            },
            Credentials::new(Some("test-access"), Some("test-secret"), None, None, None).unwrap(),
        )
        .unwrap()
        .with_path_style()
    }

    fn client(&self) -> RecurrenceTransport {
        RecurrenceTransport::new(
            &self.bucket(),
            "account/todos.json",
            &[
                "account/notes.json".to_string(),
                "account/note-attachments.json".to_string(),
            ],
        )
        .unwrap()
    }

    pub(crate) fn request(&self) -> WireRequest {
        self.requests.recv_timeout(Duration::from_secs(3)).unwrap()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let result = worker.join();
            if !thread::panicking() {
                result.unwrap();
            }
        }
    }
}

fn fixtures() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../docs/fixtures/recurrence-transport-v1.json"
    ))
    .unwrap()
}

fn empty() -> RecurrenceDocument {
    recurrence_protocol::parse_document(EMPTY).unwrap()
}

#[test]
fn recurrence_transport_shared_get_matrix() {
    tauri::async_runtime::block_on(async {
        for case in fixtures()["downloads"].as_array().unwrap() {
            let server = Server::new(vec![Reply::new(
                case["status"].as_u64().unwrap() as u16,
                case["etag"].as_str(),
                case["body"].as_str().unwrap().as_bytes(),
            )]);
            let result = server.client().download().await;
            match case["expected"].as_str().unwrap() {
                "present" => assert_eq!(result.unwrap().document, Some(empty())),
                "absent" => assert!(result.unwrap().document.is_none()),
                _ => assert!(result.is_err(), "{}", case["id"]),
            }
            let wire = server.request();
            assert!(wire
                .head
                .starts_with("GET /rules-test/account/recurrence-rules.json "));
            assert!(wire.head.contains("AWS4-HMAC-SHA256"));
        }
    });
}

#[test]
fn recurrence_transport_shared_put_matrix() {
    tauri::async_runtime::block_on(async {
        for case in fixtures()["uploads"].as_array().unwrap() {
            let server = Server::new(vec![
                Reply::new(404, None, b""),
                Reply::new(
                    case["status"].as_u64().unwrap() as u16,
                    case["etag"].as_str(),
                    b"",
                ),
            ]);
            let client = server.client();
            let remote = client.download().await.unwrap();
            let result = client.upload(&empty(), &remote).await;
            match case["expected"].as_str().unwrap() {
                "uploaded" => assert_eq!(
                    result.unwrap(),
                    RuleUploadOutcome::Uploaded {
                        etag: "\"version-2\"".to_string()
                    }
                ),
                "conflict" => assert_eq!(result.unwrap(), RuleUploadOutcome::Conflict),
                _ => assert!(result.is_err(), "{}", case["id"]),
            }
            server.request();
            let wire = server.request();
            let head = wire.head.to_lowercase();
            assert!(head.starts_with("put /rules-test/account/recurrence-rules.json "));
            assert!(head.contains("if-none-match: *\r\n"));
            assert!(!head.contains("if-match:"));
            assert!(head.contains(";if-none-match;"));
            assert_eq!(
                recurrence_protocol::parse_document(std::str::from_utf8(&wire.body).unwrap())
                    .unwrap(),
                empty()
            );
        }
    });
}

#[test]
fn recurrence_transport_conflict_requires_fresh_download() {
    tauri::async_runtime::block_on(async {
        let server = Server::new(vec![
            Reply::new(200, Some("\"a\""), EMPTY.as_bytes()),
            Reply::new(412, None, b""),
            Reply::new(200, Some("\"b\""), EMPTY.as_bytes()),
            Reply::new(200, Some("\"c\""), b""),
        ]);
        let client = server.client();
        let remote = client.download().await.unwrap();
        assert_eq!(
            client.upload(&empty(), &remote).await.unwrap(),
            RuleUploadOutcome::Conflict
        );
        server.request();
        assert!(server
            .request()
            .head
            .to_lowercase()
            .contains("if-match: \"a\"\r\n"));
        let fresh = client.download().await.unwrap();
        assert_eq!(
            client.upload(&empty(), &fresh).await.unwrap(),
            RuleUploadOutcome::Uploaded {
                etag: "\"c\"".to_string()
            }
        );
        server.request();
        assert!(server
            .request()
            .head
            .to_lowercase()
            .contains("if-match: \"b\"\r\n"));
    });
}

#[test]
fn recurrence_transport_rejects_foreign_scope_and_invalid_document_before_request() {
    tauri::async_runtime::block_on(async {
        let server = Server::new(vec![Reply::new(404, None, b"")]);
        let client = server.client();
        let remote = client.download().await.unwrap();
        assert_eq!(
            server.client().upload(&empty(), &remote).await.unwrap_err(),
            "RECURRENCE_REMOTE_SCOPE_MISMATCH"
        );
        let mut invalid = empty();
        invalid.format_version = 2;
        assert!(client.upload(&invalid, &remote).await.is_err());
        server.request();
        assert!(server.requests.try_recv().is_err());
        assert_eq!(
            client.bucket.request_timeout(),
            Some(Duration::from_secs(25))
        );
        assert!(RecurrenceTransport::new(
            &server.bucket(),
            "account/todos.json",
            &["account/recurrence-rules.json".to_string()]
        )
        .is_err());
        assert!(RecurrenceTransport::new(&server.bucket(), "../todos.json", &[]).is_err());
        let unicode = Server::new(vec![Reply::new(404, None, b"")]);
        let client =
            RecurrenceTransport::new(&unicode.bucket(), "\u{540c}\u{6b65}/%2F/todos.json", &[])
                .unwrap();
        assert!(client.download().await.unwrap().document.is_none());
        assert!(unicode
            .request()
            .head
            .starts_with("GET /rules-test/%E5%90%8C%E6%AD%A5/%252F/recurrence-rules.json "));
    });
}

#[test]
fn recurrence_transport_caps_bytes_with_or_without_content_length() {
    tauri::async_runtime::block_on(async {
        for chunked in [false, true] {
            let mut reply = Reply::new(200, Some("\"v\""), &vec![b' '; MAX_RULE_BYTES + 1]);
            reply.chunked = chunked;
            let server = Server::new(vec![reply]);
            assert_eq!(
                server.client().download().await.err().unwrap(),
                "RECURRENCE_DOCUMENT_TOO_LARGE"
            );
        }
        for body in [vec![0xff], vec![b' '; 1_048_577]] {
            let server = Server::new(vec![Reply::new(200, Some("\"v\""), &body)]);
            assert!(server.client().download().await.is_err());
        }
    });
}

#[test]
fn recurrence_transport_duplicate_etags_and_network_errors_are_not_empty() {
    tauri::async_runtime::block_on(async {
        for value in ["\"v\"\n", "\"v\"\r\n", "\"v\"\r\nInjected: value"] {
            assert!(!valid_etag(value));
        }
        let mut reply = Reply::new(200, Some("\"v\""), EMPTY.as_bytes());
        reply.headers.push_str("ETag: \"other\"\r\n");
        let server = Server::new(vec![reply]);
        assert_eq!(
            server.client().download().await.err().unwrap(),
            "RECURRENCE_ETAG_REQUIRED"
        );
        let mut delayed = Reply::new(200, Some("\"v\""), EMPTY.as_bytes());
        delayed.delay = Duration::from_millis(150);
        let server = Server::new(vec![delayed]);
        let mut client = server.client();
        client.bucket = client
            .bucket
            .with_request_timeout(Duration::from_millis(40))
            .unwrap();
        assert_eq!(
            client.download().await.err().unwrap(),
            "RECURRENCE_TRANSPORT_NETWORK"
        );
        let mut delayed = Reply::new(200, Some("\"written\""), b"");
        delayed.delay = Duration::from_millis(150);
        let server = Server::new(vec![Reply::new(404, None, b""), delayed]);
        let mut client = server.client();
        client.bucket = client
            .bucket
            .with_request_timeout(Duration::from_millis(80))
            .unwrap();
        let remote = client.download().await.unwrap();
        assert_eq!(
            client.upload(&empty(), &remote).await.unwrap_err(),
            "RECURRENCE_TRANSPORT_NETWORK"
        );
    });
}
