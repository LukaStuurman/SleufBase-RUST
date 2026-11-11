//! Process-local WebView2 window owned by SleufBase, never an external browser.
//! Authentication and reCAPTCHA are completed manually by the operator.
use std::path::Path;

pub fn run(control: &Path, profile: &Path, parent: isize, remember: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        native::run(control, profile, parent, remember, false)
    }
    #[cfg(not(windows))]
    {
        let _ = (control, profile, parent, remember);
        Err("De ingebouwde KickTheMap-browser vereist Windows en WebView2".into())
    }
}
#[cfg(all(test, windows))]
pub(crate) fn run_fixture(profile: &Path, parent: isize) -> Result<(), String> {
    native::run(profile, profile, parent, true, true)
}

#[cfg(windows)]
mod native {
    use super::*;
    use raw_window_handle::{RawWindowHandle, Win32WindowHandle};
    use std::{
        num::NonZeroIsize,
        time::{Duration, Instant},
    };
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings4;
    use windows_core::Interface;
    use winit::{
        application::ApplicationHandler,
        event::WindowEvent,
        event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
        platform::windows::EventLoopBuilderExtWindows,
        window::{Window, WindowId},
    };
    use wry::{
        NewWindowResponse, WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows,
        WebViewExtWindows,
    };
    struct App {
        // Drop the webview before its native parent and profile context.
        webview: Option<WebView>,
        window: Option<Window>,
        context: WebContext,
        error: Option<String>,
        fixture: bool,
        parent: NonZeroIsize,
        remember: bool,
        last_bounds: Option<[i32; 4]>,
        profile: std::path::PathBuf,
        visible: bool,
        last_visibility: Option<bool>,
        deadline: Option<Instant>,
    }
    impl ApplicationHandler for App {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_some() || self.error.is_some() {
                return;
            }
            let result = (|| {
                let attributes = Window::default_attributes()
                    .with_title("SleufBase — KickTheMap")
                    .with_inner_size(winit::dpi::PhysicalSize::new(1, 1))
                    .with_decorations(false)
                    .with_visible(false);
                // The parent belongs to the SleufBase process that owns
                // this helper and stays alive for its entire lifetime.
                if unsafe {
                    windows_sys::Win32::UI::WindowsAndMessaging::IsWindow(self.parent.get() as _)
                } == 0
                {
                    return Err("Het SleufBase-hoofdvenster is gesloten".into());
                }
                let parent = RawWindowHandle::Win32(Win32WindowHandle::new(self.parent));
                let attributes = unsafe { attributes.with_parent_window(Some(parent)) };
                let window = event_loop
                    .create_window(attributes)
                    .map_err(|e| e.to_string())?;
                let builder = WebViewBuilder::new_with_web_context(&mut self.context)
                    .with_additional_browser_args(
                        "--remote-debugging-port=0 --remote-debugging-address=127.0.0.1",
                    )
                    .with_devtools(false)
                    .with_general_autofill_enabled(false)
                    .with_new_window_req_handler(|_, _| NewWindowResponse::Deny);
                let builder = if self.fixture {
                    builder.with_html("<!doctype html><title>SleufBase local browser fixture</title><meta name='csrf-token' content='fixture-only'><p>Local fixture; no account or network.</p>")
                } else {
                    builder
                        .with_url("https://www.my.kickthemap.com/signin")
                        .with_navigation_handler(|url| {
                            reqwest::Url::parse(&url).ok().is_some_and(|url| {
                                crate::kickthemap_browser::is_kickthemap_origin(&url)
                            })
                        })
                };
                let webview=builder.build(&window).map_err(|e|format!("Ingebouwde browser kon niet starten. Controleer of Microsoft WebView2 Runtime is geïnstalleerd: {e}"))?;
                if self.remember {
                    unsafe {
                        let settings: ICoreWebView2Settings4 = webview
                            .controller()
                            .CoreWebView2()
                            .and_then(|view| view.Settings())
                            .and_then(|settings| settings.cast())
                            .map_err(|e| format!("WebView2 kan het account niet onthouden: {e}"))?;
                        settings
                            .SetIsPasswordAutosaveEnabled(true)
                            .map_err(|e| e.to_string())?;
                    }
                }
                self.webview = Some(webview);
                self.window = Some(window);
                // The fixture lifetime starts after synchronous WebView creation:
                // runtime initialization on a busy runner can itself take 20s.
                self.deadline = self
                    .fixture
                    .then(|| Instant::now() + Duration::from_secs(60));
                Ok::<_, String>(())
            })();
            if let Err(error) = result {
                self.error = Some(error);
                event_loop.exit();
            }
        }
        fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
            if matches!(event, WindowEvent::CloseRequested) {
                event_loop.exit();
            }
        }
        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            // Only this app's private profile receives visibility commands.
            // Keeping the authenticated view hidden preserves the download
            // capability without leaving a website window in the jobs workflow.
            use std::io::Read;
            if self.profile.join("window-stop").exists() {
                event_loop.exit();
                return;
            }
            let bounds = std::fs::read_to_string(self.profile.join("window-bounds"))
                .ok()
                .and_then(|text| parse_bounds(&text));
            if let (Some(window), Some([x, y, w, h])) = (self.window.as_ref(), bounds) {
                if bounds != self.last_bounds {
                    window.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
                    let _ = window
                        .request_inner_size(winit::dpi::PhysicalSize::new(w as u32, h as u32));
                }
            }
            self.last_bounds = bounds;
            let mut command = [0_u8; 2];
            let visible = std::fs::File::open(self.profile.join("window-visible"))
                .and_then(|mut file| file.read(&mut command))
                .ok()
                .and_then(|len| match &command[..len] {
                    b"0" => Some(false),
                    b"1" => Some(true),
                    _ => None,
                });
            let visible = visible.map(|value| value && bounds.is_some());
            if self.last_visibility != visible {
                if let (Some(visible), Some(window)) = (visible, self.window.as_ref()) {
                    if self.visible != visible {
                        window.set_visible(visible);
                        self.visible = visible;
                    }
                    if std::fs::write(
                        self.profile.join("window-visible-ack"),
                        if visible { b"1" } else { b"0" },
                    )
                    .is_ok()
                    {
                        self.last_visibility = Some(visible);
                    }
                }
            }
            let next = Instant::now() + Duration::from_millis(250);
            if self
                .deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
            {
                event_loop.exit();
            } else {
                event_loop.set_control_flow(ControlFlow::WaitUntil(
                    self.deadline.map_or(next, |deadline| deadline.min(next)),
                ));
            }
        }
    }

    fn parse_bounds(text: &str) -> Option<[i32; 4]> {
        let mut values = text.split_ascii_whitespace().map(str::parse::<i32>);
        let bounds = [
            values.next()?.ok()?,
            values.next()?.ok()?,
            values.next()?.ok()?,
            values.next()?.ok()?,
        ];
        (values.next().is_none() && bounds[2] > 0 && bounds[3] > 0).then_some(bounds)
    }

    pub(super) fn run(
        control: &Path,
        profile: &Path,
        parent: isize,
        remember: bool,
        fixture: bool,
    ) -> Result<(), String> {
        if !profile.is_dir() || !control.is_dir() {
            return Err("Tijdelijke browsermap ontbreekt".into());
        }
        let mut builder = EventLoop::builder();
        builder.with_any_thread(true);
        let event_loop = builder
            .build()
            .map_err(|e| format!("Inlogvenster starten: {e}"))?;
        let mut app = App {
            webview: None,
            window: None,
            context: WebContext::new(Some(profile.to_owned())),
            error: None,
            fixture,
            parent: NonZeroIsize::new(parent).ok_or("SleufBase-hoofdvenster ontbreekt")?,
            remember,
            last_bounds: None,
            profile: control.to_owned(),
            visible: false,
            last_visibility: None,
            deadline: None,
        };
        event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
        if let Some(error) = app.error.take() {
            Err(error)
        } else {
            Ok(())
        }
    }
}
