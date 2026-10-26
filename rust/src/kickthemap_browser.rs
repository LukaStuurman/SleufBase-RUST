//! Interactive KickTheMap browser sign-in using a *new*, temporary Microsoft
//! Edge profile. The operator handles reCAPTCHA in the real browser. CDP is
//! limited to the locally launched browser, never attached to existing tabs.
//! Credentials and session cookies are never saved to SleufBase project files.
use serde_json::{Value,json};
use std::{fs, path::{Path,PathBuf},process::{Child,Command,Stdio}, time::{Duration,Instant}};
#[cfg(test)]
use tungstenite::Message;

#[path="kickthemap_cdp.rs"]
mod cdp;

#[path="kickthemap_capture.rs"]
mod capture;
pub use capture::CaptureHandle;

const ORIGIN:&str="https://www.my.kickthemap.com";
const MAX_CDP_RESPONSE:usize=24*1024*1024;
#[derive(Clone,Debug)]
pub struct BrowserCookie {
    pub name:String,
    pub value:String,
    pub domain:String,
    pub path:String,
    pub secure:bool,
}
fn allowed_domain(raw:&str)->bool {
    let domain=raw.trim_start_matches('.').to_ascii_lowercase();
    domain=="my.kickthemap.com" || domain=="www.my.kickthemap.com"
}
fn parse_cookie(value:&Value)->Option<BrowserCookie>{
    let name=value["name"].as_str()?.to_owned();
    let content=value["value"].as_str()?.to_owned();
    let domain=value["domain"].as_str()?.to_owned();
    let path=value["path"].as_str()?.to_owned();
    let secure=value["secure"].as_bool()?;
    if !allowed_domain(&domain) || !secure || name.is_empty() || name.len()>256
        || !name.bytes().all(|c|c.is_ascii_alphanumeric()||b"!#$%&'*+-.^_`|~".contains(&c))
        || content.len()>8192 || content.chars().any(|c|matches!(c,'\r'|'\n'|';'))
        || !path.starts_with('/') || path.len()>1024 || path.chars().any(|c|matches!(c,'\r'|'\n'|';'))
    {return None;}
    Some(BrowserCookie{name,value:content,domain,path,secure})
}
pub fn parse_browser_cookies(payload:&Value)->Result<Vec<BrowserCookie>,String>{
    let items=payload.get("cookies").and_then(Value::as_array)
        .ok_or("Edge stuurde geen CDP-cookielijst terug")?;
    if items.len()>4096 {return Err("Browser-cookielijst is te groot".into());}
    let cookies=items.iter().filter_map(parse_cookie).collect::<Vec<_>>();
    if cookies.is_empty(){return Err("Nog geen KickTheMap-aanmelding gevonden in de geopende browser".into());}
    Ok(cookies)
}
pub fn validated_ws_url(raw:&str,port:u16)->Result<String,String>{
    let url=reqwest::Url::parse(raw).map_err(|_|"Ongeldige Edge DevTools-verbinding")?;
    if url.scheme()!="ws" || !url.username().is_empty() || url.password().is_some()
        || url.port_or_known_default()!=Some(port)
        || !matches!(url.host_str(),Some("localhost"|"127.0.0.1"|"[::1]"|"::1"))
        || !url.path().starts_with("/devtools/")
        || url.query().is_some() || url.fragment().is_some()
    {return Err("Edge DevTools-verbinding is niet lokaal of onverwacht".into());}
    Ok(raw.to_owned())
}
fn find_edge()->Result<PathBuf,String>{
    #[cfg(target_os="windows")] {
        let mut locations=Vec::new();
        for name in ["PROGRAMFILES(X86)","PROGRAMFILES","LOCALAPPDATA"] {
            if let Some(dir)=std::env::var_os(name) {
                locations.push(PathBuf::from(dir).join("Microsoft/Edge/Application/msedge.exe"));
            }
        }
        if let Some(path)=locations.into_iter().find(|p|p.is_file()){return Ok(path);}
        Err("Microsoft Edge is niet gevonden; installeer Edge met WebView2-ondersteuning".into())
    }
    #[cfg(not(target_os="windows"))] {
        Err("KickTheMap browseraanmelding wordt alleen op Windows met Microsoft Edge ondersteund".into())
    }
}
fn read_port(profile:&Path)->Result<(u16,String),String>{
    let contents=fs::read_to_string(profile.join("DevToolsActivePort"))
        .map_err(|e|format!("Edge DevTools-port nog niet beschikbaar: {e}"))?;
    let mut lines=contents.lines();
    let port=lines.next().ok_or("Edge DevTools-port ontbreekt")?.trim()
        .parse::<u16>().map_err(|_|"Edge DevTools-port is ongeldig")?;
    if port==0 {return Err("Edge DevTools-port is ongeldig".into());}
    let path=lines.next().ok_or("Edge DevTools-socket ontbreekt")?.trim();
    if !path.starts_with("/devtools/browser/") || path.len()>512
        || path.bytes().any(|c|c.is_ascii_whitespace())
    {return Err("Edge DevTools-socket heeft een ongeldig formaat".into());}
    Ok((port,format!("ws://127.0.0.1:{port}{path}")))
}
/// Owns the only browser from which a KickTheMap session can be adopted.
pub struct Browser {
    process:Child,
    _profile:tempfile::TempDir,
    port:u16,
    websocket:String,
}
impl Browser {
    pub fn launch()->Result<Self,String>{
        let edge=find_edge()?;
        let profile=tempfile::Builder::new().prefix("sleufbase-kickthemap-")
            .tempdir().map_err(|e|format!("Tijdelijk Edge-profiel: {e}"))?;
        let mut command=Command::new(edge);
        command.arg("--remote-debugging-port=0")
            .arg("--remote-debugging-address=127.0.0.1")
            .arg(format!("--user-data-dir={}",profile.path().display()))
            .arg("--no-first-run").arg("--no-default-browser-check")
            .arg("--new-window").arg(format!("{ORIGIN}/signin"))
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        let process=command.spawn().map_err(|e|format!("Edge starten mislukt: {e}"))?;
        let mut owned=Self{process,_profile:profile,port:0,websocket:String::new()};
        let deadline=Instant::now()+Duration::from_secs(25);
        while Instant::now()<deadline {
            if let Ok((port,websocket))=read_port(owned._profile.path()){
                owned.port=port;
                owned.websocket=websocket;
                return Ok(owned);
            }
            if owned.process.try_wait().map_err(|e|e.to_string())?.is_some() {
                return Err("Edge is gestopt voordat het browservenster klaar was".into());
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        Err("Edge-browser start niet met lokale DevTools-verbinding. Controleer bedrijfsbeleid voor remote debugging".into())
    }
    fn command(&self,method:&str,params:Value,browser_target:Option<&str>)->Result<Value,String>{
        let ws=browser_target.unwrap_or(&self.websocket);
        cdp::command(self.port,ws,method,params,&crate::input::NEVER_CANCEL,
            cdp::Limits{duration:Duration::from_secs(15),response_bytes:MAX_CDP_RESPONSE})
    }
    fn cdp(&self,method:&str,params:Value)->Result<Value,String>{
        self.command(method,params,None)
    }
    pub fn cookies(&self)->Result<Vec<BrowserCookie>,String>{
        parse_browser_cookies(&self.cdp("Storage.getCookies",json!({}))?)
    }
    pub fn browser_jobs(&self)->Result<Vec<crate::jobs_cache::Job>,String>{
        crate::kickthemap_live::parse_live_jobs(&self.browser_job_payload()?)
    }
    /// Open the selected job in a new tab of *our own* KickTheMap Edge profile.
    /// Identity comes from a current, authenticated jobs response, never from a
    /// stale cache or a guessed job URL.
    pub fn open_job(&self,job_id:u64)->Result<(),String>{
        let payload=self.browser_job_payload()?;
        let url=job_view_url(&payload,job_id)?;
        let response=self.command("Target.createTarget",
            json!({"url":url.as_str()}),None)?;
        if response["targetId"].as_str().is_none_or(str::is_empty){
            return Err("Edge kon het KickTheMap-jobtabblad niet openen".into());
        }
        Ok(())
    }
    pub fn browser_job_payload(&self)->Result<String,String>{
        let ws=self.channel().job_tab(&crate::input::NEVER_CANCEL)?;
        let script=r#"(async()=>{let csrf=document.querySelector('meta[name="csrf-token"]')?.content||'';let r=await fetch('/jobs/get-user-jobs',{method:'POST',credentials:'same-origin',headers:{'Accept':'application/json','Content-Type':'application/json','X-CSRF-TOKEN':csrf},body:'{}'});if(!r.ok)throw new Error('HTTP '+r.status);return await r.text();})()"#;
        let result=self.command("Runtime.evaluate",
            json!({"expression":script,"awaitPromise":true,"returnByValue":true}),Some(&ws))?;
        if result.get("exceptionDetails").is_some() {return Err("KickTheMap job-API kon niet in het browservenster worden opgevraagd".into());}
        let content=result["result"]["value"].as_str()
            .ok_or("KickTheMap-browser heeft geen jobgegevens geretourneerd")?;
        if content.len()>MAX_CDP_RESPONSE{return Err("KickTheMap browser-resultaat te groot".into());}
        Ok(content.to_owned())
    }
}

/// Build only an original KickTheMap viewJob path from one verified live job.
fn job_view_url(payload:&str,job_id:u64)->Result<reqwest::Url,String>{
    let raw:Value=serde_json::from_str(payload).map_err(|_|"Ongeldige live joblijst".to_owned())?;
    let arrays=[
        raw.as_array(),
        raw.get("data").and_then(Value::as_array),
        raw.get("jobs").and_then(Value::as_array),
        raw.get("projects").and_then(Value::as_array),
        raw.get("data").and_then(|v|v.get("jobs")).and_then(Value::as_array),
        raw.get("data").and_then(|v|v.get("projects")).and_then(Value::as_array),
        raw.get("data").and_then(|v|v.get("data")).and_then(Value::as_array),
    ];
    let job=arrays.into_iter().flatten().flatten().find(|j|{
        j.get("Id").or_else(||j.get("id")).and_then(|v|{
            v.as_u64().or_else(||v.as_str().and_then(|s|s.parse::<u64>().ok()))
        })==Some(job_id)
    }).ok_or("Job niet gevonden in de huidige KickTheMap-sessie")?;
    let text=|key:&str|job.get(key).and_then(Value::as_str).unwrap_or("").trim();
    let owner= if !text("user_id").is_empty(){text("user_id")}
        else if !text("project_mail").is_empty(){text("project_mail")}
        else {text("email")};
    let date= if !text("project_date").is_empty(){text("project_date")}
        else{text("created_at")};
    if owner.is_empty()||date.is_empty()||owner.len()>254||date.len()>100
       ||owner.chars().any(char::is_control)||date.chars().any(char::is_control){
        return Err("Job heeft geen betrouwbare browseridentiteit".into());
    }
    let mut url=reqwest::Url::parse(ORIGIN).map_err(|e|e.to_string())?;
    url.path_segments_mut().map_err(|_|"KickTheMap-URL heeft geen pad")?
        .push("viewJob").push(&format!("{owner}_{date}"));
    Ok(url)
}

/// A lightweight capability limited to this process's own Edge debug port.
/// It never owns or reads other Edge profiles or browser instances.
#[derive(Clone)]
pub struct BrowserChannel { port:u16, websocket:String }
impl Browser {
    pub fn channel(&self)->BrowserChannel {
        BrowserChannel {port:self.port,websocket:self.websocket.clone()}
    }
}
impl BrowserChannel {
    fn command(&self,method:&str,params:Value,target:Option<&str>,cancel:&std::sync::atomic::AtomicBool)->Result<Value,String>{
        let ws=target.unwrap_or(&self.websocket);
        cdp::command(self.port,ws,method,params,cancel,cdp::Limits{duration:Duration::from_secs(15),response_bytes:512*1024})
    }
    fn job_tab(&self,cancel:&std::sync::atomic::AtomicBool)->Result<String,String>{
        let targets=self.command("Target.getTargets",json!({}),None,cancel)?;
        let list=targets["targetInfos"].as_array().ok_or("Edge-taboverzicht ontbreekt")?;
        let page=list.iter().find(|p|{
            p["type"]=="page" && p["url"].as_str().is_some_and(|s|
                reqwest::Url::parse(s).ok().is_some_and(|u|
                    u.origin().ascii_serialization()==ORIGIN))
        }).ok_or("Er is geen KickTheMap-tabblad actief in het eigen Edge-venster")?;
        let id=page["targetId"].as_str().filter(|id|!id.is_empty()&&id.len()<=128&&id.bytes().all(|c|c.is_ascii_alphanumeric()||b"-_".contains(&c)))
            .ok_or("Ongeldige Edge-tabidentiteit")?;
        validated_ws_url(&format!("ws://127.0.0.1:{}/devtools/page/{id}",self.port),self.port)
    }
    /// Fallback to the same-origin browser session for a *signed download URL*.
    /// The resulting URL is still validated and downloaded by the independent
    /// bounded Rust HTTP client, never by blindly accepting a remote redirect.
    pub fn signed_download_url(&self,job:&crate::jobs_cache::Job,
        kind:crate::kickthemap_live::RemoteFileKind)
        ->Result<reqwest::Url,String>
    {
        self.signed_download_url_with_cancel(job,kind,&crate::input::NEVER_CANCEL)
    }
    pub fn signed_download_url_with_cancel(&self,job:&crate::jobs_cache::Job,
        kind:crate::kickthemap_live::RemoteFileKind,cancel:&std::sync::atomic::AtomicBool)->Result<reqwest::Url,String>
    {
        crate::input::check(cancel)?;
        if job.title.len()>32768||job.prefix.len()>32768{return Err("KickTheMap-jobnaam of prefix is te groot".into());}
        use crate::kickthemap_live::RemoteFileKind;
        let filename=match kind {
            RemoteFileKind::GeoTiff if !job.prefix.trim().is_empty()=>
                format!("{}.tiff",job.prefix.trim()),
            RemoteFileKind::GeoTiff=>
                return Err("KickTheMap-job heeft geen GeoTIFF bestands-prefix".into()),
            RemoteFileKind::JobFeatures=>"jobFeatures.json".into(),
        };
        let stem=job.title.trim().chars().map(|c|
            if c.is_ascii_alphanumeric()||"._-".contains(c) {c} else {'_'}
        ).collect::<String>();
        let export_name=match kind {
            RemoteFileKind::GeoTiff=>format!("{stem}.tiff"),
            RemoteFileKind::JobFeatures=>format!("{stem}_jobFeatures.json"),
        };
        let fields=json!({
            "projectId":job.job_id.to_string(),
            "folder":"cloud",
            "fileName":filename,
            "exportName":export_name
        });
        let expression=format!("{}({})",include_str!("kickthemap_signed_url.js"),fields);
        let target=self.job_tab(cancel)?;
        let response=self.command("Runtime.evaluate",
            json!({"expression":expression,"awaitPromise":true,"returnByValue":true}),
            Some(&target),cancel)?;
        if response.get("exceptionDetails").is_some() {
            return Err("Ook de ingelogde KickTheMap-browser kon geen downloadlink genereren".into());
        }
        let signed=response["result"]["value"].as_str()
            .ok_or("KickTheMap-browser leverde geen geldige downloadlink")?;
        crate::kickthemap_live::validate_signed_url(signed)
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        let _=self.process.kill();
        let _=self.process.wait();
        // A private temporary profile is dropped after the owned browser.
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_private_origin_cookies_are_adopted() {
        let cookie=json!({"cookies":[
            {"name":"laravel_session","value":"ok_abc","domain":".my.kickthemap.com","path":"/","secure":true},
            {"name":"stolen","value":"x","domain":".google.com","path":"/","secure":true},
            {"name":"bad","value":"x\r\nInjected: 1","domain":"my.kickthemap.com","path":"/","secure":true},
            {"name":"not-secure","value":"x","domain":"www.my.kickthemap.com","path":"/","secure":false}
        ]});
        let accepted=parse_browser_cookies(&cookie).unwrap();
        assert_eq!(accepted.len(),1);
        assert_eq!(accepted[0].name,"laravel_session");
        assert!(parse_browser_cookies(&json!({"cookies":[]})).is_err());
    }
    #[test]
    fn cdp_sockets_reject_nonlocal_and_crossport_urls() {
        assert!(validated_ws_url("ws://127.0.0.1:17293/devtools/browser/abc",17293).is_ok());
        assert!(validated_ws_url("ws://localhost:17293/devtools/page/abc",17293).is_ok());
        for candidate in [
            "ws://attacker.com:17293/devtools/page/x",
            "ws://localhost:17294/devtools/page/x",
            "wss://localhost:17293/devtools/page/x",
            "ws://user@localhost:17293/devtools/page/x",
            "ws://localhost:17293/private",
        ] {assert!(validated_ws_url(candidate,17293).is_err(),"{candidate}");}
    }
}
