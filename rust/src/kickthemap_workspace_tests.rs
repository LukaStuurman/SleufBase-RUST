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
            // Accepted Windows sockets can inherit the listener's nonblocking
            // mode. The scripted WebSocket peer uses blocking reads with a timeout.
            stream.set_nonblocking(false).unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            stream.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut endpoint = String::new();
            let mut socket = tungstenite::accept_hdr(stream,
                |request: &tungstenite::handshake::server::Request, response| {
                    endpoint = request.uri().path().to_owned();
                    Ok(response)
                }).unwrap();
            let mut request: Value = serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(request["method"], method);
            request["fixture_endpoint"] = json!(endpoint);
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
        ("Network.clearBrowserCookies", json!({})),
        ("Network.setCookies", json!({})),
        ("Network.getCookies", json!({"cookies":[fixture_cookie()]})),
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
        ("Target.getTargets", targets(&job)),
        ("Page.getFrameTree", frame(&job, "job-new")),
        ("Page.getFrameTree", frame(&job, "job-new")),
        // Capture cannot be injected while this document is still loading.
        ("Runtime.evaluate", json!({"result":{"value":false}})),
        ("Page.getFrameTree", frame(&job, "job-new")),
        ("Runtime.evaluate", json!({"result":{"value":true}})),
        ("Runtime.evaluate", json!({"result":{"value":true}})),
        ("Runtime.evaluate", json!({"result":{"value":true}})),
    ]);
    let actual = channel.prepare_authenticated_job(&json!([fixture_cookie()]), 42, &AtomicBool::new(false)).unwrap();
    let capture = channel.arm_job_download_capture(42, &actual, &AtomicBool::new(false)).unwrap();
    assert_eq!(capture.job_id(), 42);
    capture.disarm().unwrap();
    worker.join().unwrap();
    assert_eq!(actual, job);
    let sent: Vec<_> = requests.try_iter().collect();
    let transferred = &sent[3]["params"]["cookies"][0];
    assert!(transferred.get("domain").is_none(), "must be host-only");
    assert!(transferred.get("expires").is_none(), "must be session-only");
    assert_eq!(transferred["url"], home);
    assert_eq!(transferred["secure"], true);
    assert_eq!(transferred["httpOnly"], true);
    assert_eq!(sent[4]["params"]["urls"], json!([home]));
    for request in &sent[1..5] {
        assert_eq!(request["fixture_endpoint"], "/devtools/page/fixture-page",
            "native cookies must reach the same WebView that opens the job");
    }
    assert_eq!(sent[10]["params"]["url"], job);
    for request in &sent[..20] {
        assert!(!request["params"]["expression"].as_str().unwrap_or("")
            .contains("const key='__sleufbaseNativeDownloadCapture'"));
    }
    let arm = sent[20]["params"]["expression"].as_str().unwrap();
    assert!(arm.contains(&format!("location.href!=={}", json!(job))));
    assert!(arm.contains("document.readyState==='loading'"));
    assert!(arm.contains("const key='__sleufbaseNativeDownloadCapture'"));
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
        ("Target.getTargets", targets(&job)),
        ("Page.getFrameTree", frame(&job, "existing-job")),
        ("Page.getFrameTree", frame(&job, "existing-job")),
        ("Runtime.evaluate", json!({"result":{"value":true}})),
        ("Runtime.evaluate", json!({"result":{"value":true}})),
        ("Runtime.evaluate", json!({"result":{"value":true}})),
    ]);
    assert_eq!(channel.open_verified_job_with_native_session(&json!([fixture_cookie()]), 42, &AtomicBool::new(false)).unwrap(), job);
    let capture = channel.arm_job_download_capture(42, &job, &AtomicBool::new(false)).unwrap();
    capture.disarm().unwrap();
    worker.join().unwrap();
    assert!(requests.try_iter().all(|request| request["method"] != "Page.navigate"
        && !request["method"].as_str().unwrap().contains("Cookies")));
}

#[test]
fn native_handoff_refuses_missing_changed_or_widened_page_cookies_before_navigation() {
    let mut wrong_value = fixture_cookie();
    wrong_value["value"] = json!("fixture-other-session");
    let mut wider_domain = fixture_cookie();
    wider_domain["domain"] = json!(".my.kickthemap.com");
    let mut readable = fixture_cookie();
    readable["httpOnly"] = json!(false);
    let mut persistent = fixture_cookie();
    persistent["expires"] = json!(2_000_000_000);
    let mut partitioned = fixture_cookie();
    partitioned["partitionKey"] = json!({"topLevelSite":ORIGIN});
    for received in [json!([]), json!([wrong_value]), json!([wider_domain]),
        json!([readable]), json!([persistent]), json!([partitioned]),
        json!([fixture_cookie(), fixture_cookie()])]
    {
        let (channel, requests, worker) = scripted(vec![
            ("Target.getTargets", targets(&format!("{ORIGIN}/signin"))),
            ("Page.stopLoading", json!({})),
            ("Network.clearBrowserCookies", json!({})),
            ("Network.setCookies", json!({})),
            ("Network.getCookies", json!({"cookies":received})),
        ]);
        let error = channel.prepare_authenticated_job(&json!([fixture_cookie()]), 42,
            &AtomicBool::new(false)).unwrap_err();
        worker.join().unwrap();
        assert!(error.contains("overgedragen"));
        assert!(!error.contains("verlopen") && !error.contains("fixture-session")
            && !error.contains("laravel_session"));
        let sent: Vec<_> = requests.try_iter().collect();
        assert_eq!(sent.len(), 5, "a failed cookie transfer must never navigate or read jobs");
        assert!(sent[2..].iter().all(|request| request["fixture_endpoint"] == "/devtools/page/fixture-page"));
    }
}

#[test]
fn login_route_is_not_expired_until_its_current_document_is_ready() {
    let home = format!("{ORIGIN}/");
    let signin = format!("{ORIGIN}/signin");
    let (channel, requests, worker) = scripted(vec![
        ("Page.getFrameTree", frame(&signin, "home-new")),
        // The current JS document is still loading or already changed again.
        ("Runtime.evaluate", json!({"result":{"value":false}})),
        ("Page.getFrameTree", frame(&home, "home-new")),
        ("Runtime.evaluate", json!({"result":{"value":true}})),
    ]);
    let load = PageLoad { url:reqwest::Url::parse(&home).unwrap(), frame_id:"frame".into(),
        loader_id:Some("home-new".into()), require_csrf:true };
    let target = format!("ws://127.0.0.1:{}/devtools/page/fixture-page", channel.port);
    channel.wait_page(&target, &load, &AtomicBool::new(false), Instant::now() + Duration::from_secs(2)).unwrap();
    worker.join().unwrap();
    let sent: Vec<_> = requests.try_iter().collect();
    let guard = sent[1]["params"]["expression"].as_str().unwrap();
    assert!(guard.contains(&format!("location.href==={}", json!(signin))));
    assert!(guard.contains("document.readyState!=='loading'"));
}

#[test]
fn signed_out_job_window_recovers_from_current_native_session_and_verifies_the_job() {
    let signin = format!("{ORIGIN}/signin");
    let home = format!("{ORIGIN}/");
    let job = format!("{ORIGIN}/viewJob/fixture_2026-11-16");
    let payload = json!({"data":[{"Id":42,"user_id":"fixture","project_date":"2026-11-16"}]}).to_string();
    let (channel, requests, worker) = scripted(vec![
        ("Target.getTargets", targets(&signin)),
        ("Page.getFrameTree", frame(&signin, "signed-out")),
        ("Page.getFrameTree", frame(&signin, "signed-out")),
        ("Runtime.evaluate", json!({"result":{"value":true}})),
        ("Target.getTargets", targets(&signin)),
        ("Page.stopLoading", json!({})),
        ("Network.clearBrowserCookies", json!({})),
        ("Network.setCookies", json!({})),
        ("Network.getCookies", json!({"cookies":[fixture_cookie()]})),
        ("Page.navigate", json!({"frameId":"frame","loaderId":"home-new"})),
        ("Page.getFrameTree", frame(&home, "home-new")),
        ("Runtime.evaluate", json!({"result":{"value":true}})),
        ("Runtime.evaluate", json!({"result":{"value":payload}})),
        ("Page.navigate", json!({"frameId":"frame","loaderId":"job-new"})),
        ("Page.getFrameTree", frame(&job, "job-new")),
        ("Runtime.evaluate", json!({"result":{"value":true}})),
    ]);
    assert_eq!(channel.open_verified_job_with_native_session(&json!([fixture_cookie()]), 42,
        &AtomicBool::new(false)).unwrap(), job);
    worker.join().unwrap();
    let sent: Vec<_> = requests.try_iter().collect();
    assert_eq!(sent.iter().filter(|request| request["method"] == "Network.setCookies").count(), 1);
    assert_eq!(sent[9]["params"]["url"], home);
    assert_eq!(sent[13]["params"]["url"], job);
    assert!(sent[12]["params"]["expression"].as_str().unwrap().contains("/jobs/get-user-jobs"));
}

#[test]
fn native_session_recovery_stops_after_one_rejected_handoff() {
    let signin = format!("{ORIGIN}/signin");
    let (channel, requests, worker) = scripted(vec![
        ("Target.getTargets", targets(&signin)),
        ("Page.getFrameTree", frame(&signin, "signed-out")),
        ("Page.getFrameTree", frame(&signin, "signed-out")),
        ("Runtime.evaluate", json!({"result":{"value":true}})),
        ("Target.getTargets", targets(&signin)),
        ("Page.stopLoading", json!({})),
        ("Network.clearBrowserCookies", json!({})),
        ("Network.setCookies", json!({})),
        ("Network.getCookies", json!({"cookies":[fixture_cookie()]})),
        ("Page.navigate", json!({"frameId":"frame","loaderId":"rejected-session"})),
        ("Page.getFrameTree", frame(&signin, "rejected-session")),
        ("Runtime.evaluate", json!({"result":{"value":true}})),
    ]);
    let error = channel.open_verified_job_with_native_session(&json!([fixture_cookie()]), 42,
        &AtomicBool::new(false)).unwrap_err();
    worker.join().unwrap();
    assert_eq!(error, BROWSER_SIGNIN_REQUIRED);
    assert!(!error.contains("sessie is verlopen"));
    let sent: Vec<_> = requests.try_iter().collect();
    assert_eq!(sent.iter().filter(|request| request["method"] == "Network.setCookies").count(), 1);
    assert_eq!(sent.iter().filter(|request| request["method"] == "Page.navigate").count(), 1);
}

#[test]
fn job_capture_rejects_invalid_expected_routes_before_contacting_the_browser() {
    let channel = BrowserChannel { port:9, websocket:"ws://127.0.0.1:9/devtools/browser/unused".into() };
    for url in ["https://other.example/viewJob/test", "http://www.my.kickthemap.com/viewJob/test",
        "https://www.my.kickthemap.com/", "https://www.my.kickthemap.com/signin",
        "https://www.my.kickthemap.com/viewJob/", "https://www.my.kickthemap.com/viewJob/test/extra",
        "https://www.my.kickthemap.com/viewJob/test?redirect=x", "https://www.my.kickthemap.com/viewJob/test#other"]
    {
        assert_eq!(channel.arm_job_download_capture(42, url, &AtomicBool::new(false)).err().as_deref(),
            Some("Ongeldige geselecteerde KickTheMap-joblocatie"), "{url}");
    }
}

#[test]
fn job_capture_requires_the_exact_owned_job_page_and_frame() {
    let expected = format!("{ORIGIN}/viewJob/selected");
    for url in [format!("{ORIGIN}/"), format!("{ORIGIN}/viewJob/other"),
        "https://other.example/viewJob/selected".into()]
    {
        let (channel, requests, worker) = scripted(vec![("Target.getTargets", targets(&url))]);
        assert!(channel.arm_job_download_capture(42, &expected, &AtomicBool::new(false)).is_err());
        worker.join().unwrap();
        assert_eq!(requests.try_iter().count(), 1, "must not inject in {url}");
    }
    let (channel, requests, worker) = scripted(vec![
        ("Target.getTargets", targets(&expected)),
        ("Page.getFrameTree", frame(&format!("{ORIGIN}/viewJob/other"), "changed-document")),
    ]);
    assert!(channel.arm_job_download_capture(42, &expected, &AtomicBool::new(false)).is_err());
    worker.join().unwrap();
    assert!(requests.try_iter().all(|request| request["method"] != "Runtime.evaluate"));
}

#[test]
fn job_capture_rejects_a_failed_final_document_guard_and_cleans_up() {
    let job = format!("{ORIGIN}/viewJob/selected");
    let (channel, requests, worker) = scripted(vec![
        ("Target.getTargets", targets(&job)),
        ("Page.getFrameTree", frame(&job, "existing-job")),
        ("Page.getFrameTree", frame(&job, "existing-job")),
        ("Runtime.evaluate", json!({"result":{"value":true}})),
        // Navigation/loading can begin after the ready reply but before arming.
        ("Runtime.evaluate", json!({"result":{"value":false}})),
        ("Runtime.evaluate", json!({"result":{"value":true}})),
    ]);
    assert!(channel.arm_job_download_capture(42, &job, &AtomicBool::new(false)).is_err());
    worker.join().unwrap();
    let sent: Vec<_> = requests.try_iter().collect();
    let arm = sent[4]["params"]["expression"].as_str().unwrap();
    let guard_end = arm.find("return false;").unwrap();
    assert!(arm[..guard_end].contains(&format!("location.href!=={}", json!(job))));
    assert!(arm[..guard_end].contains("document.readyState==='loading'"));
    assert!(arm.find("const key='__sleufbaseNativeDownloadCapture'").unwrap() > guard_end);
    assert!(sent[5]["params"]["expression"].as_str().unwrap().contains("s.cleanup()"));
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
    assert!(channel.open_verified_job_with_native_session(&json!([fixture_cookie()]), 42, &AtomicBool::new(false)).is_err());
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
    assert!(channel.open_verified_job_with_native_session(&json!([fixture_cookie()]), 42, &cancel).is_err());
    assert!(channel.arm_job_download_capture(42, &format!("{ORIGIN}/viewJob/selected"), &cancel)
        .err().unwrap().contains("geannuleerd"));
    assert!(channel.form_snapshot(&cancel).is_err());
    assert!(channel.reload_job(&format!("{ORIGIN}/viewJob/selected"), &cancel).is_err());
    assert!(channel.form_snapshot(&AtomicBool::new(false)).is_err());
}
