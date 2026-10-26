//! Bounded one-shot fetch/XHR capture in the exact process-owned Edge tab.
//! Original contract: kickthemap_browser.DownloadCaptureApi and capture JS.
use super::*;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream},
    sync::atomic::AtomicBool,
};
use tungstenite::protocol::WebSocketConfig;
pub struct CaptureHandle {
    channel: BrowserChannel,
    target: String,
    nonce: String,
    job_id: u64,
    expires: Instant,
}
impl BrowserChannel {
    fn capture_command(
        &self,
        method: &str,
        params: Value,
        target: &str,
        cancel: &AtomicBool,
    ) -> Result<Value, String> {
        self.capture_command_budget(method, params, target, cancel, Duration::from_secs(4))
    }
    fn capture_command_budget(
        &self,
        method: &str,
        params: Value,
        target: &str,
        cancel: &AtomicBool,
        budget: Duration,
    ) -> Result<Value, String> {
        crate::input::check(cancel)?;
        validated_ws_url(target, self.port)?;
        let stream = TcpStream::connect_timeout(
            &SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), self.port),
            Duration::from_millis(200),
        )
        .map_err(|e| format!("Edge-captureverbinding: {e}"))?;
        stream
            .set_read_timeout(Some(Duration::from_millis(150)))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_millis(200)))
            .map_err(|e| e.to_string())?;
        let config = WebSocketConfig::default()
            .max_message_size(Some(512 * 1024))
            .max_frame_size(Some(512 * 1024));
        let (mut socket, _) = tungstenite::client::client_with_config(target, stream, Some(config))
            .map_err(|e| format!("Edge-capturehandshake: {e}"))?;
        socket
            .send(Message::Text(
                json!({"id":1,"method":method,"params":params})
                    .to_string()
                    .into(),
            ))
            .map_err(|e| e.to_string())?;
        let deadline = Instant::now() + budget;
        loop {
            crate::input::check(cancel)?;
            if Instant::now() > deadline {
                return Err("Edge-captureantwoord duurde te lang".into());
            }
            let message = match socket.read() {
                Ok(m) => m,
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    continue
                }
                Err(e) => return Err(format!("Edge-captureantwoord: {e}")),
            };
            if let Message::Text(body) = message {
                let value: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
                if value["id"] == 1 {
                    if value.get("error").is_some() {
                        return Err("Edge heeft de captureopdracht geweigerd".into());
                    }
                    return value
                        .get("result")
                        .cloned()
                        .ok_or("Edge-captureantwoord ontbreekt".into());
                }
            }
        }
    }
    /// Arms only the selected job in a tab in this BrowserChannel's own process.
    pub fn arm_download_capture(
        &self,
        job_id: u64,
        cancel: &AtomicBool,
    ) -> Result<CaptureHandle, String> {
        if job_id == 0 {
            return Err("Kies een geldige KickTheMap-job".into());
        }
        let targets =
            self.capture_command("Target.getTargets", json!({}), &self.websocket, cancel)?;
        let info = targets["targetInfos"]
            .as_array()
            .and_then(|list| {
                list.iter().find(|t| {
                    t["type"] == "page"
                        && t["url"].as_str().is_some_and(|s| {
                            reqwest::Url::parse(s).ok().is_some_and(|u| {
                                u.origin() == reqwest::Url::parse(ORIGIN).unwrap().origin()
                            })
                        })
                })
            })
            .ok_or("Open KickTheMap in het eigen Edge-venster")?;
        let id = info["targetId"]
            .as_str()
            .filter(|id| {
                !id.is_empty()
                    && id.len() <= 128
                    && id
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
            })
            .ok_or("Ongeldige Edge-tabidentiteit")?;
        let target = format!("ws://127.0.0.1:{}/devtools/page/{id}", self.port);
        let mut nonce = [0u8; 16];
        getrandom::fill(&mut nonce).map_err(|e| e.to_string())?;
        let nonce = nonce.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let expression = format!(
            "{}({})",
            include_str!("kickthemap_capture.js"),
            json!({"nonce":nonce,"jobId":job_id.to_string()})
        );
        let handle = CaptureHandle {
            channel: self.clone(),
            target,
            nonce,
            job_id,
            expires: Instant::now() + Duration::from_secs(120),
        };
        let result = self.capture_command(
            "Runtime.evaluate",
            json!({"expression":expression,"returnByValue":true,"awaitPromise":true}),
            &handle.target,
            cancel,
        );
        match result {
            Err(error) => {
                let _ = handle.disarm();
                Err(error)
            }
            Ok(result)
                if result.get("exceptionDetails").is_some()
                    || result["result"]["value"] != true =>
            {
                let _ = handle.disarm();
                Err("De handmatige download kon niet worden geactiveerd in deze Edge-tab".into())
            }
            Ok(_) => Ok(handle),
        }
    }
}
impl CaptureHandle {
    pub fn job_id(&self) -> u64 {
        self.job_id
    }
    /// Polling never changes the selected job or tab. Returned JSON contains
    /// request templates and a response object path, never the signed URL.
    pub fn wait(&self, cancel: &AtomicBool) -> Result<String, String> {
        let result = (|| loop {
            crate::input::check(cancel)?;
            if Instant::now() > self.expires {
                return Err(
                    "Geen handmatige GeoTIFF-download vastgelegd binnen twee minuten".into(),
                );
            }
            let expression=format!("(()=>{{const s=window.__sleufbaseNativeDownloadCapture;if(!s||s.nonce!=={})throw Error('capture replaced');return s.captured;}})()",json!(self.nonce));
            let result = self.channel.capture_command(
                "Runtime.evaluate",
                json!({"expression":expression,"returnByValue":true}),
                &self.target,
                cancel,
            )?;
            if result.get("exceptionDetails").is_some() {
                return Err("De vastlegsessie is vervangen of de Edge-tab is vernieuwd".into());
            }
            if let Some(value) = result
                .get("result")
                .and_then(|v| v.get("value"))
                .filter(|v| !v.is_null())
            {
                return validate_capture(value, self.job_id, &self.nonce);
            }
            crate::cadastral::cancel_sleep(cancel, Duration::from_millis(150))?;
        })();
        // Cleanup is itself bounded and performed on the capture worker. An
        // expired/replaced nonce can never remove a newer armed capture.
        let _ = self.disarm();
        result
    }
    pub fn disarm(&self) -> Result<(), String> {
        let expression=format!("(()=>{{const s=window.__sleufbaseNativeDownloadCapture;if(s?.nonce==={})s.cleanup();return true;}})()",json!(self.nonce));
        self.channel
            .capture_command_budget(
                "Runtime.evaluate",
                json!({"expression":expression,"returnByValue":true}),
                &self.target,
                &AtomicBool::new(false),
                Duration::from_millis(200),
            )
            .map(|_| ())
    }
}
fn validate_capture(value: &Value, job_id: u64, nonce: &str) -> Result<String, String> {
    if value["nonce"] != nonce
        || value["job_id"] != job_id.to_string()
        || value["status"] != "captured"
    {
        return Err("Vastgelegde download hoort bij een andere job of vastlegsessie".into());
    }
    let body = serde_json::to_string(value).map_err(|e| e.to_string())?;
    if body.len() > 256 * 1024 {
        return Err("Handmatige downloadregistratie is te groot".into());
    }
    let fields = value["request"]["fields"]
        .as_object()
        .ok_or("Downloadvelden ontbreken")?;
    if !fields
        .values()
        .any(|v| crate::python_values::string(Some(v)).ok().as_deref() == Some(&job_id.to_string()))
    {
        return Err("Gekozen job ontbreekt in de vastgelegde aanvraag".into());
    }
    Ok(body)
}

#[cfg(test)]
mod capture_tests {
    use super::*;
    use std::{
        net::TcpListener,
        sync::{Arc, Mutex},
        thread,
    };
    fn accept_bounded(listener: &TcpListener) -> TcpStream {
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "mock connection deadline");
                    thread::sleep(Duration::from_millis(5));
                }
                Err(e) => panic!("{e}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
    }
    fn server(
        replies: Vec<Value>,
    ) -> (
        BrowserChannel,
        Arc<Mutex<Vec<Value>>>,
        thread::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let messages = seen.clone();
        let worker = thread::spawn(move || {
            for reply in replies {
                let stream = accept_bounded(&listener);
                let mut socket = tungstenite::accept(stream).unwrap();
                let body = socket.read().unwrap().into_text().unwrap();
                messages
                    .lock()
                    .unwrap()
                    .push(serde_json::from_str(&body).unwrap());
                let _ = socket.send(Message::Text(
                    json!({"id":1,"result":reply}).to_string().into(),
                ));
            }
        });
        (
            BrowserChannel {
                port,
                websocket: format!("ws://127.0.0.1:{port}/devtools/browser/fake"),
            },
            seen,
            worker,
        )
    }
    fn handle(channel: BrowserChannel) -> CaptureHandle {
        CaptureHandle {
            target: format!("ws://127.0.0.1:{}/devtools/page/fake", channel.port),
            channel,
            nonce: "expected".into(),
            job_id: 42,
            expires: Instant::now() + Duration::from_secs(2),
        }
    }
    fn payload() -> Value {
        json!({"nonce":"expected","job_id":"42","status":"captured","request":{"fields":{"jobId":"42"}}})
    }
    #[test]
    fn websocket_capture_requires_exact_nonce_job_status_and_cleans_up() {
        for mutation in [
            None,
            Some(("nonce", "foreign")),
            Some(("job_id", "43")),
            Some(("status", "pending")),
        ] {
            let mut value = payload();
            if let Some((key, value_text)) = mutation {
                value[key] = json!(value_text);
            }
            let (channel, seen, worker) = server(vec![
                json!({"result":{"value":value}}),
                json!({"result":{"value":true}}),
            ]);
            let result = handle(channel).wait(&AtomicBool::new(false));
            assert_eq!(result.is_ok(), mutation.is_none());
            worker.join().unwrap();
            let seen = seen.lock().unwrap();
            assert_eq!(seen.len(), 2);
            let cleanup = seen[1]["params"]["expression"].as_str().unwrap();
            assert!(cleanup.contains("expected") && cleanup.contains("s.cleanup()"));
        }
    }
    #[test]
    fn cancelled_or_expired_capture_still_disarms_exact_nonce() {
        for cancelled in [true, false] {
            let (channel, seen, worker) = server(vec![json!({"result":{"value":true}})]);
            let mut h = handle(channel);
            if !cancelled {
                h.expires = Instant::now() - Duration::from_millis(1);
            }
            assert!(h.wait(&AtomicBool::new(cancelled)).is_err());
            worker.join().unwrap();
            assert_eq!(seen.lock().unwrap().len(), 1);
        }
    }
    #[test]
    fn arming_exception_disarms_the_exact_attempted_nonce() {
        let (channel, seen, worker) = server(vec![
            json!({"targetInfos":[{"type":"page","url":ORIGIN,"targetId":"fake"}]}),
            json!({"exceptionDetails":{"text":"fake failure"}}),
            json!({"result":{"value":true}}),
        ]);
        assert!(channel
            .arm_download_capture(42, &AtomicBool::new(false))
            .is_err());
        worker.join().unwrap();
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 3);
        let arm = seen[1]["params"]["expression"].as_str().unwrap();
        let cleanup = seen[2]["params"]["expression"].as_str().unwrap();
        let nonce = arm
            .rsplit_once("\"nonce\":\"")
            .unwrap()
            .1
            .split('"')
            .next()
            .unwrap();
        assert_eq!(nonce.len(), 32);
        assert!(
            cleanup.contains(nonce)
                && cleanup.contains("s?.nonce===")
                && cleanup.contains("s.cleanup()")
        );
    }
    #[test]
    fn foreign_websocket_target_is_rejected_before_connect() {
        let channel = BrowserChannel {
            port: 9,
            websocket: String::new(),
        };
        for target in [
            "ws://example.com:9/devtools/page/fake",
            "ws://127.0.0.1:10/devtools/page/fake",
        ] {
            assert!(channel
                .capture_command(
                    "Runtime.evaluate",
                    json!({}),
                    target,
                    &AtomicBool::new(false)
                )
                .is_err());
        }
    }
    #[test]
    fn websocket_rejects_oversized_frame_and_enforces_response_deadline() {
        let (channel, _, worker) = server(vec![json!({"padding":"x".repeat(600*1024)})]);
        assert!(channel
            .capture_command(
                "Runtime.evaluate",
                json!({}),
                &channel.websocket,
                &AtomicBool::new(false)
            )
            .is_err());
        worker.join().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let worker = thread::spawn(move || {
            let stream = accept_bounded(&listener);
            let mut socket = tungstenite::accept(stream).unwrap();
            let _ = socket.read();
            thread::sleep(Duration::from_millis(350));
        });
        let channel = BrowserChannel {
            port,
            websocket: format!("ws://127.0.0.1:{port}/devtools/browser/fake"),
        };
        let start = Instant::now();
        let error = channel
            .capture_command_budget(
                "Runtime.evaluate",
                json!({}),
                &channel.websocket,
                &AtomicBool::new(false),
                Duration::from_millis(50),
            )
            .unwrap_err();
        assert!(error.contains("te lang"), "{error}");
        assert!(start.elapsed() < Duration::from_secs(2));
        worker.join().unwrap();
    }
}
