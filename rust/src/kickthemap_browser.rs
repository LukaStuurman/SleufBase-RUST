//! Interactive KickTheMap browser sign-in using a *new*, temporary SleufBase
//! WebView2 profile. The sign-in page handles its own reCAPTCHA. CDP is
//! limited to the locally launched browser, never attached to existing tabs.
//! Credentials and session cookies are never saved to SleufBase project files.
use serde_json::{Value,json};
use std::{fs, path::Path,process::Command, time::{Duration,Instant}};
#[cfg(test)]
use tungstenite::Message;

#[path="kickthemap_cdp.rs"]
mod cdp;

#[path="kickthemap_capture.rs"]
mod capture;
pub use capture::CaptureHandle;

#[cfg(windows)]
use crate::vllm_runtime::windows_job::Process as BrowserProcess;
#[cfg(not(windows))]
use std::process::Child as BrowserProcess;
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
pub(crate) fn is_kickthemap_origin(url:&reqwest::Url)->bool{
    url.scheme()=="https"&&url.username().is_empty()&&url.password().is_none()&&url.port_or_known_default()==Some(443)&&matches!(url.host_str(),Some("www.my.kickthemap.com"|"my.kickthemap.com"))
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
// Only the cookies the existing in-memory adoption path trusts can be restored.
fn session_cookie_params(payload:&Value)->Vec<Value>{
    let Some(items)=payload.as_array().filter(|items|items.len()<=4096) else{return Vec::new()};
    items.iter().filter_map(|item|{
        // Partitioned cookies require their original top-level context.
        if item.get("partitionKey").is_some(){return None;}
        let cookie=parse_cookie(item)?;
        let mut params=json!({
            "name":cookie.name,"value":cookie.value,"domain":cookie.domain,
            "path":cookie.path,"secure":true,"httpOnly":item["httpOnly"].as_bool().unwrap_or(false),
        });
        if let Some(expires)=item["expires"].as_f64().filter(|n|n.is_finite()&&*n>0.){
            params["expires"]=json!(expires);
        }
        if let Some(site)=item["sameSite"].as_str().filter(|value|matches!(*value,"Strict"|"Lax"|"None")){
            params["sameSite"]=json!(site);
        }
        Some(params)
    }).collect()
}

pub fn parse_browser_cookies(payload:&Value)->Result<Vec<BrowserCookie>,String>{
    let items=payload.get("cookies").and_then(Value::as_array)
        .ok_or("Ingebouwde browser stuurde geen CDP-cookielijst terug")?;
    if items.len()>4096 {return Err("Browser-cookielijst is te groot".into());}
    let cookies=items.iter().filter_map(parse_cookie).collect::<Vec<_>>();
    if cookies.is_empty(){return Err("Nog geen KickTheMap-aanmelding gevonden in de geopende browser".into());}
    Ok(cookies)
}
pub fn validated_ws_url(raw:&str,port:u16)->Result<String,String>{
    let url=reqwest::Url::parse(raw).map_err(|_|"Ongeldige Ingebouwde browser DevTools-verbinding")?;
    if url.scheme()!="ws" || !url.username().is_empty() || url.password().is_some()
        || url.port_or_known_default()!=Some(port)
        || !matches!(url.host_str(),Some("localhost"|"127.0.0.1"|"[::1]"|"::1"))
        || !url.path().starts_with("/devtools/")
        || url.query().is_some() || url.fragment().is_some()
    {return Err("Ingebouwde browser DevTools-verbinding is niet lokaal of onverwacht".into());}
    Ok(raw.to_owned())
}
fn read_port(profile:&Path)->Result<(u16,String),String>{
    let contents=fs::read_to_string(profile.join("EBWebView").join("DevToolsActivePort"))
        .map_err(|e|format!("Ingebouwde browser DevTools-port nog niet beschikbaar: {e}"))?;
    let mut lines=contents.lines();
    let port=lines.next().ok_or("Ingebouwde browser DevTools-port ontbreekt")?.trim()
        .parse::<u16>().map_err(|_|"Ingebouwde browser DevTools-port is ongeldig")?;
    if port==0 {return Err("Ingebouwde browser DevTools-port is ongeldig".into());}
    let path=lines.next().ok_or("Ingebouwde browser DevTools-socket ontbreekt")?.trim();
    if !path.starts_with("/devtools/browser/") || path.len()>512
        || path.bytes().any(|c|c.is_ascii_whitespace())
    {return Err("Ingebouwde browser DevTools-socket heeft een ongeldig formaat".into());}
    Ok((port,format!("ws://127.0.0.1:{port}{path}")))
}
/// Owns the only browser from which a KickTheMap session can be adopted.
pub struct Browser {
    process:BrowserProcess,
    _profile:tempfile::TempDir,
    port:u16,
    websocket:String,
}
impl Browser {
    pub fn launch()->Result<Self,String>{
        #[cfg(windows)] {
            let profile=tempfile::Builder::new().prefix("sleufbase-kickthemap-").tempdir().map_err(|e|e.to_string())?;
            let mut command=Command::new(std::env::current_exe().map_err(|e|e.to_string())?);
            command.arg("--kickthemap-webview").arg(profile.path());
            Self::launch_owned(command,profile)
        }
        #[cfg(not(windows))] {Err("De ingebouwde KickTheMap-browser vereist Windows en WebView2".into())}
    }
    #[cfg(windows)]
    fn launch_owned(command:Command,profile:tempfile::TempDir)->Result<Self,String>{
        let log=fs::File::create(profile.path().join("startup.log")).map_err(|e|e.to_string())?;
        let process=BrowserProcess::spawn(&command,&log).map_err(|e|format!("Inlogvenster starten: {e}"))?;
        let mut owned=Self{process,_profile:profile,port:0,websocket:String::new()};
        let deadline=Instant::now()+Duration::from_secs(45);
        while Instant::now()<deadline {
            if let Ok((port,websocket))=read_port(owned._profile.path()){
                owned.port=port;owned.websocket=websocket;
                // WebView2 exposes its diagnostic port before its first page
                // exists. Wait for the owned webview instead of announcing a
                // socket-only startup as a ready login window.
                if owned.cdp("Target.getTargets",json!({})).ok().and_then(|v|v.get("targetInfos").and_then(Value::as_array).map(|a|a.iter().any(|p|p["type"]=="page"))).unwrap_or(false){return Ok(owned);}
            }
            if let Some(status)=owned.process.try_wait().map_err(|e|e.to_string())? {
                let detail=fs::read_to_string(owned._profile.path().join("startup.log")).unwrap_or_default();
                let detail=detail.chars().take(2000).collect::<String>();
                return Err(if detail.trim().is_empty(){format!("Het ingebouwde inlogvenster is gesloten voordat de verbinding klaar was ({status})")}
                    else{format!("Ingebouwd inlogvenster starten: {}",detail.trim())});
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err("Het ingebouwde inlogvenster reageert niet. Controleer de WebView2 Runtime en bedrijfsinstellingen voor browserdiagnostiek.".into())
    }
    fn command(&self,method:&str,params:Value,browser_target:Option<&str>)->Result<Value,String>{
        let ws=browser_target.unwrap_or(&self.websocket);
        cdp::command(self.port,ws,method,params,&crate::input::NEVER_CANCEL,
            cdp::Limits{duration:Duration::from_secs(15),response_bytes:MAX_CDP_RESPONSE})
    }
    fn cdp(&self,method:&str,params:Value)->Result<Value,String>{
        self.command(method,params,None)
    }
    pub fn is_closed(&mut self)->bool {
        !matches!(self.process.try_wait(),Ok(None))
    }
    pub fn session_cookies(&self)->Result<Value,String>{
        let data=self.cdp("Storage.getCookies",json!({}))?;
        let cookies=session_cookie_params(&data["cookies"]);
        if cookies.is_empty(){return Err("Geen sessiecookies gevonden".into());}
        Ok(Value::Array(cookies))
    }
    pub fn restore_session(&self,cookies:&Value)->Result<(),String>{
        let mut cookies=session_cookie_params(cookies);
        if cookies.is_empty(){return Err("Geen geldige sessiecookies gevonden".into());}
        for cookie in &mut cookies{
            if let Some(host)=cookie["domain"].as_str().filter(|domain|!domain.starts_with('.')){
                // An omitted Domain attribute keeps this cookie host-only.
                let url=format!("https://{host}/");
                cookie.as_object_mut().unwrap().remove("domain");
                cookie["url"]=json!(url);
            }
        }
        self.cdp("Storage.setCookies",json!({"cookies":cookies}))?;
        self.navigate_account("/")
    }
    pub fn reset_signin(&self)->Result<(),String>{
        self.cdp("Storage.clearCookies",json!({}))?;
        self.navigate_account("/signin")
    }
    fn navigate_account(&self,path:&str)->Result<(),String>{
        let ws=self.channel().job_tab(&crate::input::NEVER_CANCEL)?;
        let result=self.command("Page.navigate",json!({"url":format!("{ORIGIN}{path}")}),Some(&ws))?;
        if result.get("errorText").is_some(){return Err("KickTheMap-pagina kon niet worden geopend".into());}
        Ok(())
    }
    /// Submit only the provider's own form, after its verification token is ready.
    pub fn submit_signin(&self, account: &crate::kickthemap_session::Account) -> Result<bool, String> {
        let ws = self.channel().job_tab(&crate::input::NEVER_CANCEL)?;
        let credentials = json!([account.email, account.password]);
        let script = r#"(([email,password])=>{
            if(!['https://www.my.kickthemap.com','https://my.kickthemap.com'].includes(location.origin)
                || !/^\/signin\/?$/.test(location.pathname)) return false;
            const form=document.querySelector('form#signin-form');
            if(!form || form.method!=='post' || form.action!==location.origin+'/login'
                || form.target || !form.elements.namedItem('_token')?.value
                || !form.elements.namedItem('g-recaptcha-response')?.value) return false;
            const user=form.querySelector('input[type=email][name=email]');
            const secret=form.querySelector('input[type=password][name=password]');
            if(!user || !secret) return false;
            user.value=email;
            secret.value=password;
            HTMLFormElement.prototype.requestSubmit.call(form);
            return true;
        })"#;
        let result = self.command("Runtime.evaluate", json!({
            "expression": format!("{script}({credentials})"),
            "returnByValue": true,
        }), Some(&ws)).map_err(|_| "Het KickTheMap-formulier is niet bereikbaar.")?;
        if result.get("exceptionDetails").is_some() {
            return Err("Het KickTheMap-formulier kon niet worden verzonden.".into());
        }
        Ok(result["result"]["value"].as_bool() == Some(true))
    }

    pub fn cookies(&self)->Result<Vec<BrowserCookie>,String>{
        parse_browser_cookies(&self.cdp("Storage.getCookies",json!({}))?)
    }
    pub fn csrf_token(&self)->Result<String,String>{
        let ws=self.channel().job_tab(&crate::input::NEVER_CANCEL)?;
        let result=self.command("Runtime.evaluate",json!({
            "expression":r#"(()=>{if(!['https://www.my.kickthemap.com','https://my.kickthemap.com'].includes(location.origin))throw new Error('Ongeldige website');if(/^\/(signin|login)\/?$/.test(location.pathname))throw new Error('Meld je eerst aan');return document.querySelector('meta[name="csrf-token"]')?.content||'';})()"#,
            "returnByValue":true
        }),Some(&ws))?;
        if result.get("exceptionDetails").is_some(){return Err("Meld je eerst volledig aan in het geopende Browser-venster".into());}
        result["result"]["value"].as_str().filter(|s|!s.is_empty())
            .map(str::to_owned).ok_or("KickTheMap-tabblad heeft nog geen actieve sessie".into())
    }
    pub fn browser_jobs(&self)->Result<Vec<crate::jobs_cache::Job>,String>{
        crate::kickthemap_live::parse_live_jobs(&self.browser_job_payload()?)
    }
    /// Navigate the owned embedded view to the selected KickTheMap job.
    /// Identity comes from a current, authenticated jobs response, never from a
    /// stale cache or a guessed job URL.
    pub fn set_visible(&self,visible:bool)->Result<(),String>{
        fs::write(self._profile.path().join("window-visible"),if visible{b"1"}else{b"0"}).map_err(|_|"Het ingebouwde venster kon niet worden getoond of verborgen".into())
    }
    pub fn open_job(&self,job_id:u64)->Result<(),String>{
        self.set_visible(true)?;
        let payload=self.browser_job_payload()?;
        let url=job_view_url(&payload,job_id)?;
        let target=self.channel().job_tab(&crate::input::NEVER_CANCEL)?;
        let response=self.command("Page.navigate",
            json!({"url":url.as_str()}),Some(&target))?;
        if response["frameId"].as_str().is_none_or(str::is_empty)||response.get("errorText").is_some(){
            return Err("Ingebouwde browser kon het KickTheMap-jobtabblad niet openen".into());
        }
        Ok(())
    }
    pub fn browser_job_payload(&self)->Result<String,String>{
        self.channel().browser_job_payload()
    }
}

/// Build only an original KickTheMap viewJob path from one verified live job.
fn job_view_url(payload:&str,job_id:u64)->Result<reqwest::Url,String>{
    let raw:Value=serde_json::from_str(payload).map_err(|_|"Ongeldige live joblijst".to_owned())?;
    let entries=crate::kickthemap_live::jobs_list(&raw).ok_or("Onbekend live joblijstformaat")?;
    let job=entries.iter().find(|j|crate::kickthemap_live::parse_positive_id(j.get("Id").or_else(||j.get("id")))==Some(job_id)).ok_or("Job niet gevonden in de huidige KickTheMap-sessie")?;
    let(owner,date)=crate::kickthemap_identity::view_parts(job)?;
    let mut url=reqwest::Url::parse(ORIGIN).map_err(|e|e.to_string())?;
    url.path_segments_mut().map_err(|_|"KickTheMap-URL heeft geen pad")?
        .push("viewJob").push(&format!("{owner}_{date}"));
    Ok(url)
}

/// A lightweight capability limited to this process's own Ingebouwde browser debug port.
/// It never owns or reads other Ingebouwde browser profiles or browser instances.
#[derive(Clone)]
pub struct BrowserChannel { port:u16, websocket:String }
impl Browser {
    pub fn channel(&self)->BrowserChannel {
        BrowserChannel {port:self.port,websocket:self.websocket.clone()}
    }
}
impl BrowserChannel {
    pub fn browser_job_payload(&self)->Result<String,String>{
        self.browser_job_payload_with_cancel(&crate::input::NEVER_CANCEL)
    }
    pub(crate) fn browser_job_payload_with_cancel(&self,cancel:&std::sync::atomic::AtomicBool)->Result<String,String>{
        let ws=self.job_tab(cancel)?;
        let script=r#"(async()=>{if(!['https://www.my.kickthemap.com','https://my.kickthemap.com'].includes(location.origin))throw new Error('Ongeldige website');let csrf=document.querySelector('meta[name="csrf-token"]')?.content||'';let r=await fetch('/jobs/get-user-jobs',{method:'POST',credentials:'same-origin',headers:{'Accept':'application/json','Content-Type':'application/json','X-CSRF-TOKEN':csrf},body:'{}'});if(!r.ok)throw new Error('HTTP '+r.status);return await r.text();})()"#;
        let result=cdp::command(self.port,&ws,"Runtime.evaluate",
            json!({"expression":script,"awaitPromise":true,"returnByValue":true}),cancel,
            cdp::Limits{duration:Duration::from_secs(25),response_bytes:MAX_CDP_RESPONSE})?;
        if result.get("exceptionDetails").is_some() {return Err("KickTheMap job-API kon niet in het browservenster worden opgevraagd".into());}
        let content=result["result"]["value"].as_str()
            .ok_or("KickTheMap-browser heeft geen jobgegevens geretourneerd")?;
        if content.len()>MAX_CDP_RESPONSE{return Err("KickTheMap browser-resultaat te groot".into());}
        Ok(content.to_owned())
    }
    pub(crate) fn storage_config_payload(&self,cancel:&std::sync::atomic::AtomicBool)->Result<String,String>{
        let target=self.job_tab(cancel)?;
        let expression=format!("{}()",include_str!("kickthemap_storage_config.js"));
        let response=self.command("Runtime.evaluate",json!({"expression":expression,"awaitPromise":true,"returnByValue":true}),Some(&target),cancel)?;
        if response.get("exceptionDetails").is_some(){return Err("KickTheMap-opslagconfiguratie kon niet binnen de ingelogde sessie worden opgehaald".into());}
        response["result"]["value"].as_str().filter(|s|s.len()<=65536).map(str::to_owned).ok_or("Ongeldig opslagantwoord uit de ingelogde sessie".into())
    }
    fn command(&self,method:&str,params:Value,target:Option<&str>,cancel:&std::sync::atomic::AtomicBool)->Result<Value,String>{
        let ws=target.unwrap_or(&self.websocket);
        cdp::command(self.port,ws,method,params,cancel,cdp::Limits{duration:Duration::from_secs(15),response_bytes:512*1024})
    }
    fn job_tab(&self,cancel:&std::sync::atomic::AtomicBool)->Result<String,String>{
        let targets=self.command("Target.getTargets",json!({}),None,cancel)?;
        let list=targets["targetInfos"].as_array().ok_or("Browser-taboverzicht ontbreekt")?;
        let page=list.iter().find(|p|{
            p["type"]=="page" && p["url"].as_str().is_some_and(|s|
                reqwest::Url::parse(s).ok().is_some_and(|u|
                    is_kickthemap_origin(&u)))
        }).ok_or("Er is geen KickTheMap-tabblad actief in het eigen Browser-venster")?;
        let id=page["targetId"].as_str().filter(|id|!id.is_empty()&&id.len()<=128&&id.bytes().all(|c|c.is_ascii_alphanumeric()||b"-_".contains(&c)))
            .ok_or("Ongeldige Browser-tabidentiteit")?;
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
        let fields=kind.download_fields(job.job_id,filename,export_name);
        let fields=fields.into_iter().map(|(key,value)|(key.into(),Value::String(value))).collect();
        self.signed_download_fields_with_cancel(Value::Object(fields),cancel)
    }
    pub(crate) fn signed_download_fields_with_cancel(&self,fields:Value,cancel:&std::sync::atomic::AtomicBool)->Result<reqwest::Url,String>{
        crate::input::check(cancel)?;
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
    fn open_job_uses_live_verified_identity_not_guessed_urls() {
        let payload=r#"{"data":[{"Id":42,"user_id":"fixture@example.test","project_date":"2026-10-25T12:00:00Z"}]}"#;
        let url=job_view_url(payload,42).unwrap();
        assert_eq!(url.origin().ascii_serialization(),ORIGIN);
        assert!(url.path().starts_with("/viewJob/"));
        assert!(url.path().contains("fixture"));
        assert!(job_view_url(payload,43).is_err());
        assert!(job_view_url(r#"{"data":[{"Id":42,"user_id":12345,"project_date":"2026-10-26"}]}"#,42)
            .unwrap().path().contains("12345_2026-10-26"));
        assert!(job_view_url(r#"{"data":[{"Id":42,"project_date":"now"}]}"#,42).is_err());
        assert!(job_view_url(r#"{"data":[{"Id":42,"user_id":"bob","project_date":"\\n"}]}"#,42).is_ok());
    }
    #[test]
    fn restored_cookies_keep_security_and_expiry_and_exclude_unrelated_domains(){
        let fixture=json!([
            {"name":"session","value":"fixture","domain":".my.kickthemap.com","path":"/","secure":true,
             "expires":2_000_000_000,"httpOnly":true,"sameSite":"Lax","session":false},
            {"name":"other","value":"fixture","domain":"other.kickthemap.com","path":"/","secure":true},
            {"name":"insecure","value":"fixture","domain":"my.kickthemap.com","path":"/","secure":false}
        ]);
        let cookies=session_cookie_params(&fixture);
        assert_eq!(cookies.len(),1);
        assert_eq!(cookies[0]["httpOnly"],true);
        assert_eq!(cookies[0]["sameSite"],"Lax");
        assert_eq!(cookies[0]["expires"].as_f64(),Some(2_000_000_000.));
        assert_eq!(session_cookie_params(&Value::Array(cookies.clone())),cookies);
    }
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

#[cfg(all(test,windows))]
mod embedded_tests {
    use super::*;
    const FIXTURE:&str="kickthemap_browser::embedded_tests::native_webview_fixture";
    #[test]
    fn native_webview_fixture(){
        let Some(profile)=std::env::var_os("SLEUFBASE_WEBVIEW_FIXTURE") else{return};
        if let Err(error)=crate::kickthemap_webview::run_fixture(Path::new(&profile)){
            eprintln!("{error}");std::process::exit(3);
        }
    }
    #[test]
    fn embedded_runtime_opens_local_fixture_and_connects_without_external_edge(){
        let profile=tempfile::Builder::new().prefix("sleufbase-webview-test-").tempdir().unwrap();
        let mut command=Command::new(std::env::current_exe().unwrap());
        command.args(["--exact",FIXTURE,"--nocapture"]).env("SLEUFBASE_WEBVIEW_FIXTURE",profile.path());
        let browser=Browser::launch_owned(command,profile).unwrap();
        let deadline=Instant::now()+Duration::from_secs(5);
        loop {
            let targets=browser.cdp("Target.getTargets",json!({})).unwrap();
            let id=targets["targetInfos"].as_array().and_then(|a|a.iter().find(|p|p["type"]=="page")).and_then(|p|p["targetId"].as_str());
            let Some(id)=id else{assert!(Instant::now()<deadline,"webview page missing");std::thread::sleep(Duration::from_millis(50));continue};
            let ws=format!("ws://127.0.0.1:{}/devtools/page/{id}",browser.port);
            let result=browser.command("Runtime.evaluate",json!({"expression":"document.title","returnByValue":true}),Some(&ws)).unwrap();
            if result["result"]["value"]=="SleufBase local browser fixture"{break;}
            assert!(Instant::now()<deadline,"local fixture did not load");std::thread::sleep(Duration::from_millis(50));
        }
        browser.set_visible(false).unwrap();
        let until=Instant::now()+Duration::from_secs(3);
        while fs::read(browser._profile.path().join("window-visible-ack")).ok().as_deref()!=Some(b"0"){
            assert!(Instant::now()<until,"owned hidden window did not acknowledge visibility command");std::thread::sleep(Duration::from_millis(50));
        }
        // Browser owns a kill-on-close process tree; dropping it closes the
        // hidden fixture and its WebView2 subprocesses before profile removal.
        drop(browser);
    }
}

#[cfg(test)] mod navigation_regressions{
    use super::*;
    #[test]fn prefix_only_jobs_open_like_original_sleufbase(){
        let url=job_view_url(r#"{"data":[{"Id":42,"Prefix":"name_with_underscore@example.test_2026-10-24_13-14-15","created_at":"2026-10-25T08:00:00Z","user_id":123}]}"#,42).unwrap();
        assert_eq!(url.path(),"/viewJob/123_2026-10-24_13-14-15");
        let url=job_view_url(r#"{"jobs":[{"Id":" 42 ","s3_root_dir":"owner@example.test_2026-10-24_13-14-15"}]}"#,42).unwrap();assert!(url.path().ends_with("owner@example.test_2026-10-24_13-14-15"));
    }
}
