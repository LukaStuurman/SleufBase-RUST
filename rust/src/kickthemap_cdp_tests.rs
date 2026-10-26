use super::*;
use std::{
    net::TcpListener,
    sync::{atomic::Ordering, Arc},
};

fn accept(listener: &TcpListener) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                return stream;
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "missing test connection");
                thread::sleep(Duration::from_millis(5));
            }
            Err(e) => panic!("{e}"),
        }
    }
}
fn server(
    work: impl FnOnce(TcpListener) + Send + 'static,
) -> (u16, String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    (
        port,
        format!("ws://127.0.0.1:{port}/devtools/browser/fake"),
        thread::spawn(move || work(listener)),
    )
}
fn limits(ms: u64, bytes: usize) -> Limits {
    Limits {
        duration: Duration::from_millis(ms),
        response_bytes: bytes,
    }
}

#[test]
fn matching_reply_ignores_unrelated_events_and_preserves_error_failures() {
    for response in [
        json!({"id":1,"result":{"value":"ok"}}),
        json!({"id":1,"error":{"message":"failure"}}),
    ] {
        let failed = response.get("error").is_some();
        let (port, url, worker) = server(move |l| {
            let mut socket = tungstenite::accept(accept(&l)).unwrap();
            let request: Value =
                serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(request["method"], "Runtime.evaluate");
            socket
                .send(Message::Text(
                    json!({"method":"Page.event"}).to_string().into(),
                ))
                .unwrap();
            socket
                .send(Message::Text(response.to_string().into()))
                .unwrap();
        });
        let result = command(
            port,
            &url,
            "Runtime.evaluate",
            json!({}),
            &AtomicBool::new(false),
            limits(1000, 1024),
        );
        if failed {
            assert!(result.is_err());
        } else {
            assert_eq!(result.unwrap()["value"], "ok");
        }
        worker.join().unwrap();
    }
}
#[test]
fn event_flood_cannot_extend_absolute_deadline() {
    let (port, url, worker) = server(|l| {
        let mut socket = tungstenite::accept(accept(&l)).unwrap();
        socket.read().unwrap();
        for _ in 0..200 {
            if socket
                .send(Message::Text(
                    json!({"id":2,"result":{}}).to_string().into(),
                ))
                .is_err()
            {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
    });
    let start = Instant::now();
    let error = command(
        port,
        &url,
        "Runtime.evaluate",
        json!({}),
        &AtomicBool::new(false),
        limits(100, 1024),
    )
    .unwrap_err();
    assert!(error.contains("te lang"), "{error}");
    assert!(start.elapsed() < Duration::from_secs(1));
    worker.join().unwrap();
}
#[test]
fn cancellation_interrupts_stalled_response_and_handshake() {
    for handshake in [false, true] {
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let (port, url, worker) = server(move |l| {
            let stream = accept(&l);
            if handshake {
                thread::sleep(Duration::from_millis(50));
                flag.store(true, Ordering::Relaxed);
                thread::sleep(Duration::from_millis(150));
                drop(stream);
            } else {
                let mut socket = tungstenite::accept(stream).unwrap();
                socket.read().unwrap();
                flag.store(true, Ordering::Relaxed);
                thread::sleep(Duration::from_millis(150));
            }
        });
        let start = Instant::now();
        let error = command(
            port,
            &url,
            "Runtime.evaluate",
            json!({}),
            &cancel,
            limits(2000, 1024),
        )
        .unwrap_err();
        assert!(error.contains("geannuleerd"), "{error}");
        assert!(start.elapsed() < Duration::from_secs(1));
        worker.join().unwrap();
    }
}
#[test]
fn partial_handshake_obeys_the_same_deadline() {
    let (port, url, worker) = server(|l| {
        let mut stream = accept(&l);
        stream
            .write_all(b"HTTP/1.1 101 Switching Protocols\r\n")
            .unwrap();
        thread::sleep(Duration::from_millis(350));
    });
    let start = Instant::now();
    let error = command(
        port,
        &url,
        "Runtime.evaluate",
        json!({}),
        &AtomicBool::new(false),
        limits(100, 1024),
    )
    .unwrap_err();
    assert!(error.contains("te lang"), "{error}");
    assert!(start.elapsed() < Duration::from_secs(1));
    worker.join().unwrap();
}
#[test]
fn announced_and_fragmented_oversize_frames_fail_before_payload_completion() {
    for fragmented in [false, true] {
        let (port, url, worker) = server(move |l| {
            let mut socket = tungstenite::accept(accept(&l)).unwrap();
            socket.read().unwrap();
            if fragmented {
                socket.get_mut().write_all(&[0x01, 96]).unwrap();
                socket.get_mut().write_all(&[b' '; 96]).unwrap();
                let _ = socket.get_mut().write_all(&[0x80, 96]);
                let _ = socket.get_mut().write_all(&[b' '; 96]);
            } else {
                let mut header = vec![0x81, 127];
                header.extend_from_slice(&(1024u64 * 1024).to_be_bytes());
                socket.get_mut().write_all(&header).unwrap();
            }
            thread::sleep(Duration::from_millis(350));
        });
        let error = command(
            port,
            &url,
            "Runtime.evaluate",
            json!({}),
            &AtomicBool::new(false),
            limits(2000, 128),
        )
        .unwrap_err();
        assert!(error.to_lowercase().contains("limit"), "{error}");
        worker.join().unwrap();
    }
}
#[test]
fn preflight_rejects_cancelled_and_oversize_requests_before_connect() {
    let url = "ws://127.0.0.1:9/devtools/browser/fake";
    assert!(command(
        9,
        url,
        "Runtime.evaluate",
        json!({}),
        &AtomicBool::new(true),
        limits(100, 1024)
    )
    .unwrap_err()
    .contains("geannuleerd"));
    assert!(command(
        9,
        url,
        "Runtime.evaluate",
        json!({"text":"x".repeat(REQUEST_LIMIT)}),
        &AtomicBool::new(false),
        limits(1000, 1024)
    )
    .unwrap_err()
    .contains("te groot"));
}
#[test]
fn large_command_is_sent_once_under_backpressure() {
    let length = REQUEST_LIMIT - 128;
    let (port, url, worker) = server(move |l| {
        let mut socket = tungstenite::accept(accept(&l)).unwrap();
        // Delay consuming a near-limit request so the nonblocking writer must
        // retain and flush its existing frame instead of resending it.
        thread::sleep(Duration::from_millis(80));
        let request: Value =
            serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(request["params"]["text"].as_str().unwrap().len(), length);
        socket
            .send(Message::Text(
                json!({"id":1,"result":{"sent":1}}).to_string().into(),
            ))
            .unwrap();
        assert!(socket.read().is_err(), "command was sent twice");
    });
    let result = command(
        port,
        &url,
        "Runtime.evaluate",
        json!({"text":"x".repeat(length)}),
        &AtomicBool::new(false),
        limits(3000, 1024),
    )
    .unwrap();
    assert_eq!(result["sent"], 1);
    worker.join().unwrap();
}
fn job() -> crate::jobs_cache::Job {
    crate::jobs_cache::Job {
        job_id: 42,
        title: "PS 42".into(),
        prefix: "file42".into(),
        address: String::new(),
        municipality: String::new(),
        coordinates: String::new(),
        client_date: String::new(),
        status: 1,
        downloadable: true,
    }
}
#[test]
fn actual_signed_url_fallback_selects_owned_target_validates_url_and_obeys_cancel() {
    for (cancel_reply, signed) in [
        (false, "https://s3.amazonaws.com/example.tiff"),
        (false, "https://foreign.example/example.tiff"),
        (true, "https://s3.amazonaws.com/example.tiff"),
    ] {
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let (port, url, worker) = server(move |l| {
            for index in 0..2 {
                let mut socket = tungstenite::accept_hdr(
                    accept(&l),
                    |request: &tungstenite::handshake::server::Request,
                     response: tungstenite::handshake::server::Response| {
                        assert_eq!(
                            request.uri().path(),
                            if index == 0 {
                                "/devtools/browser/fake"
                            } else {
                                "/devtools/page/owned"
                            }
                        );
                        Ok(response)
                    },
                )
                .unwrap();
                let request: Value =
                    serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
                let result = if index == 0 {
                    assert_eq!(request["method"], "Target.getTargets");
                    json!({"targetInfos":[{"type":"page","url":"https://www.my.kickthemap.com:8443/","targetId":"wrong-port"},{"type":"page","url":"https://other.example/","targetId":"foreign"},{"type":"page","url":super::super::ORIGIN,"targetId":"owned"}]})
                } else {
                    assert_eq!(request["method"], "Runtime.evaluate");
                    let expression = request["params"]["expression"].as_str().unwrap();
                    assert!(expression.contains("file42.tiff") && expression.contains("42"));
                    if cancel_reply {
                        flag.store(true, Ordering::Relaxed);
                    }
                    json!({"result":{"value":signed}})
                };
                let _ = socket.send(Message::Text(
                    json!({"id":1,"result":result}).to_string().into(),
                ));
            }
        });
        let channel = super::super::BrowserChannel {
            port,
            websocket: url,
        };
        let result = channel.signed_download_url_with_cancel(
            &job(),
            crate::kickthemap_live::RemoteFileKind::GeoTiff,
            &cancel,
        );
        if cancel_reply {
            assert!(result.unwrap_err().contains("geannuleerd"));
        } else if signed.contains("foreign") {
            assert!(result.unwrap_err().contains("domein"));
        } else {
            assert_eq!(result.unwrap().host_str(), Some("s3.amazonaws.com"));
        }
        worker.join().unwrap();
    }
}
