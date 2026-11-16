use super::*;
use std::{io, net::TcpListener, sync::mpsc, thread};

fn fixture_cookie() -> Value {
    json!({"name":"laravel_session","value":"fixture-session","domain":"www.my.kickthemap.com",
        "path":"/","secure":true,"httpOnly":true,"sameSite":"Lax"})
}

fn targets(url: &str) -> Value {
    json!({"targetInfos":[{"type":"page","targetId":"fixture-page","url":url}]})
}

fn frame(url: &str, loader: &str) -> Value {
    json!({"frameTree":{"frame":{"id":"frame","url":url,"loaderId":loader}}})
}

fn scripted(replies: Vec<(&'static str, Value)>) -> (BrowserChannel, mpsc::Receiver<Value>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        for (method, result) in replies {
            let deadline = Instant::now() + Duration::from_secs(5);
            let stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "missing {method} request");
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            stream.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            let request: Value = serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(request["method"], method);
            tx.send(request).unwrap();
            socket.send(Message::Text(json!({"id":1,"result":result}).to_string().into())).unwrap();
        }
    });
    (BrowserChannel { port, websocket:format!("ws://127.0.0.1:{port}/devtools/browser/fixture") }, rx, worker)
}

#[test]
fn native_cookie_handoff_rejects_wider_scope_and_keeps_secrets_http_only() {
    let cookie = fixture_cookie();
    assert_eq!(native_session_cookie_params(&json!([cookie.clone()])).unwrap(), vec![cookie.clone()]);
    assert!(native_session_cookie_params(&json!([])).is_err());
    for (field, value) in [
        ("domain", json!(".my.kickthemap.com")), ("domain", json!("other.example")),
        ("path", json!("/private")), ("secure", json!(false)), ("httpOnly", json!(false)),
        ("expires", json!(2_000_000_000)), ("partitionKey", json!({"topLevelSite":ORIGIN})),
        ("name", json!("")), ("name", json!("bad;name")), ("value", json!("")),
        ("value", json!("secret\r\nInjected: 1")), ("value", json!("quoted\"value")),
    ] {
        let mut invalid = cookie.clone();
        invalid[field] = value;
        assert!(native_session_cookie_params(&json!([invalid])).is_err(), "{field}");
    }
    let mut conflicting = cookie.clone();
    conflicting["value"] = json!("different-fixture");
    assert!(native_session_cookie_params(&json!([cookie.clone(), conflicting])).is_err());
    assert_eq!(native_session_cookie_params(&json!([cookie.clone(), cookie])).unwrap().len(), 1);
}

#[test]
fn native_account_opens_verified_job_only_after_new_documents_are_ready() {
    let home = format!("{ORIGIN}/");
    let job = format!("{ORIGIN}/viewJob/fixture_2026-11-16");
    let payload = json!({"data":[{"Id":42,"user_id":"fixture","project_date":"2026-11-16"}]}).to_string();
    let (channel, requests, worker) = scripted(vec![
        ("Target.getTargets", targets(&format!("{ORIGIN}/signin"))),
        ("Page.stopLoading", json!({})),
        ("Storage.clearCookies", json!({})),
        ("Storage.setCookies", json!({})),
        ("Page.navigate", json!({"frameId":"frame","loaderId":"home-new"})),
        // The old home can have the right URL while navigation is still pending.
        ("Page.getFrameTree", frame(&home, "home-old")),
        ("Page.getFrameTree", frame(&home, "home-new")),
        ("Runtime.evaluate", json!({"result":{"value":true}})),
        ("Runtime.evaluate", json!({"result":{"value":payload}})),
        ("Page.navigate", json!({"frameId":"frame","loaderId":"job-new"})),
        ("Page.getFrameTree", frame(&home, "home-new")),
        ("Page.getFrameTree", frame(&job, "job-new")),
        ("Runtime.evaluate", json!({"result":{"value":true}})),
    ]);
    let actual = channel.prepare_authenticated_job(&json!([fixture_cookie()]), 42, &AtomicBool::new(false)).unwrap();
    worker.join().unwrap();
    assert_eq!(actual, job);
    let sent: Vec<_> = requests.try_iter().collect();
    let transferred = &sent[3]["params"]["cookies"][0];
    assert!(transferred.get("domain").is_none(), "must be host-only");
    assert!(transferred.get("expires").is_none(), "must be session-only");
    assert_eq!(transferred["url"], home);
    assert_eq!(transferred["secure"], true);
    assert_eq!(transferred["httpOnly"], true);
    assert_eq!(sent[9]["params"]["url"], job);
}

#[test]
fn reopening_the_current_verified_job_preserves_unfinished_geometry() {
    let job = format!("{ORIGIN}/viewJob/fixture_2026-11-16");
    let payload = json!({"data":[{"Id":42,"user_id":"fixture","project_date":"2026-11-16"}]}).to_string();
    let (channel, requests, worker) = scripted(vec![
        ("Target.getTargets", targets(&job)),
        ("Page.getFrameTree", frame(&job, "existing-job")),
        ("Page.getFrameTree", frame(&job, "existing-job")),
        ("Runtime.evaluate", json!({"result":{"value":true}})),
        ("Runtime.evaluate", json!({"result":{"value":payload}})),
        ("Page.getFrameTree", frame(&job, "existing-job")),
        ("Page.getFrameTree", frame(&job, "existing-job")),
        ("Runtime.evaluate", json!({"result":{"value":true}})),
    ]);
    assert_eq!(channel.open_verified_job(42, &AtomicBool::new(false)).unwrap(), job);
    worker.join().unwrap();
    assert!(requests.try_iter().all(|request| request["method"] != "Page.navigate"));
}

#[test]
fn job_not_in_current_account_never_navigates_to_a_cached_or_guessed_job() {
    let home = format!("{ORIGIN}/");
    let (channel, requests, worker) = scripted(vec![
        ("Target.getTargets", targets(&home)),
        ("Page.getFrameTree", frame(&home, "home")),
        ("Page.getFrameTree", frame(&home, "home")),
        ("Runtime.evaluate", json!({"result":{"value":true}})),
        ("Runtime.evaluate", json!({"result":{"value":"{\"data\":[]}"}})),
    ]);
    assert!(channel.open_verified_job(42, &AtomicBool::new(false)).is_err());
    worker.join().unwrap();
    assert!(requests.try_iter().all(|request| request["method"] != "Page.navigate"));
}

#[test]
fn form_channel_rejects_other_origins_routes_and_missing_pages_before_evaluation() {
    for url in ["https://other.example/viewJob/test", "http://www.my.kickthemap.com/viewJob/test",
        "https://www.my.kickthemap.com/", "https://www.my.kickthemap.com/viewJob/",
        "https://www.my.kickthemap.com/viewJob/test/extra", "https://www.my.kickthemap.com/viewJob/test?redirect=x",
        "https://www.my.kickthemap.com/viewJob/test#other"]
    {
        let (channel, requests, worker) = scripted(vec![("Target.getTargets", targets(url))]);
        assert!(channel.form_snapshot(&AtomicBool::new(false)).is_err(), "{url}");
        worker.join().unwrap();
        assert_eq!(requests.try_iter().count(), 1);
    }
    let (channel, _requests, worker) = scripted(vec![("Target.getTargets", json!({"targetInfos":[]}))]);
    assert!(channel.form_snapshot(&AtomicBool::new(false)).is_err());
    worker.join().unwrap();
}

#[test]
fn reload_refuses_a_different_job_in_the_same_browser() {
    let (channel, requests, worker) = scripted(vec![("Target.getTargets", targets(&format!("{ORIGIN}/viewJob/other")))]);
    assert!(channel.reload_job(&format!("{ORIGIN}/viewJob/selected"), &AtomicBool::new(false)).is_err());
    worker.join().unwrap();
    assert_eq!(requests.try_iter().count(), 1);
}

#[test]
fn cancelled_operations_and_closed_browser_stop_without_mutation() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let channel = BrowserChannel { port, websocket:format!("ws://127.0.0.1:{port}/devtools/browser/closed") };
    let cancel = AtomicBool::new(true);
    assert!(Browser::launch_embedded_with_cancel(BrowserDock::new().unwrap(), 0, false, &cancel)
        .err().unwrap().contains("geannuleerd"));
    assert!(channel.prepare_authenticated_job(&json!([fixture_cookie()]), 42, &cancel).is_err());
    assert!(channel.open_verified_job(42, &cancel).is_err());
    assert!(channel.form_snapshot(&cancel).is_err());
    assert!(channel.reload_job(&format!("{ORIGIN}/viewJob/selected"), &cancel).is_err());
    assert!(channel.form_snapshot(&AtomicBool::new(false)).is_err());
}
