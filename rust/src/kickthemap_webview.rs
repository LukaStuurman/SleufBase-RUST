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
    use raw_window_handle::{HasWindowHandle, RawWindowHandle, Win32WindowHandle};
    use std::{
        io::Read,
        num::NonZeroIsize,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::{Duration, Instant},
    };
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings4;
    use windows_core::Interface;
    use winit::{
        application::ApplicationHandler,
        event::WindowEvent,
        event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
        platform::windows::{EventLoopBuilderExtWindows, WindowAttributesExtWindows},
        window::{Window, WindowId},
    };
    use wry::{
        NewWindowResponse, PageLoadEvent, WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows,
        WebViewExtWindows,
    };
    const MAX_EDITOR_SCRIPT_BYTES: u64 = 256 * 1024;

    struct App {
        // Drop the webview before its native parent and profile context.
        webview: Option<WebView>,
        window: Option<Window>,
        context: WebContext,
        error: Option<String>,
        fixture: bool,
        parent: NonZeroIsize,
        remember: bool,
        detached: bool,
        last_bounds: Option<[i32; 4]>,
        profile: std::path::PathBuf,
        visible: bool,
        last_visibility: Option<bool>,
        last_window_title: Option<String>,
        last_editor_script: Option<String>,
        editor_page_loaded: Arc<AtomicBool>,
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
                    .with_inner_size(if self.detached {
                        winit::dpi::PhysicalSize::new(1000, 720)
                    } else {
                        winit::dpi::PhysicalSize::new(1, 1)
                    })
                    .with_decorations(self.detached)
                    .with_resizable(self.detached)
                    .with_active(false)
                    .with_visible(false);
                // The parent belongs to the SleufBase process that owns
                // this helper and stays alive for its entire lifetime.
                if unsafe {
                    windows_sys::Win32::UI::WindowsAndMessaging::IsWindow(self.parent.get() as _)
                } == 0
                {
                    return Err("Het SleufBase-hoofdvenster is gesloten".into());
                }
                let attributes = if self.detached {
                    // Ownership keeps the browser tied to SleufBase without
                    // turning it into a WS_CHILD inside the egui input window.
                    attributes.with_owner_window(self.parent.get() as _)
                } else {
                    let parent = RawWindowHandle::Win32(Win32WindowHandle::new(self.parent));
                    unsafe { attributes.with_parent_window(Some(parent)) }
                };
                let window = event_loop
                    .create_window(attributes)
                    .map_err(|e| e.to_string())?;
                let editor_script = read_editor_script(&self.profile);
                let editor_page_loaded = self.editor_page_loaded.clone();
                let builder = WebViewBuilder::new_with_web_context(&mut self.context)
                    .with_additional_browser_args(
                        "--remote-debugging-port=0 --remote-debugging-address=127.0.0.1",
                    )
                    .with_devtools(false)
                    // Wry otherwise calls WebView2 MoveFocus during creation,
                    // even when its host window is hidden for startup auth.
                    .with_focused(false)
                    .with_visible(false)
                    .with_general_autofill_enabled(false)
                    .with_on_page_load_handler(move |event, _| {
                        if matches!(event, PageLoadEvent::Finished) {
                            editor_page_loaded.store(true, Ordering::Release);
                        }
                    })
                    .with_new_window_req_handler(|_, _| NewWindowResponse::Deny);
                let builder = if let Some(script) = editor_script.as_ref() {
                    builder.with_initialization_script(script.clone())
                } else {
                    builder
                };
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
                let webview = if self.detached {
                    builder
                        .with_bounds(webview_bounds(window.inner_size()))
                        .build_as_child(&window)
                } else {
                    builder.build(&window)
                }
                .map_err(|e|format!("Ingebouwde browser kon niet starten. Controleer of Microsoft WebView2 Runtime is geïnstalleerd: {e}"))?;
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
                self.last_editor_script = editor_script;
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
            match event {
                WindowEvent::CloseRequested if self.detached => {
                    // Closing this window preserves the page and any unsaved
                    // form edits; the owning app can explicitly open it again.
                    let _ = self.set_visible(false);
                    let _ = std::fs::write(self.profile.join("window-visible"), b"0");
                    let _ = std::fs::write(self.profile.join("window-closed"), b"1");
                    self.last_visibility = std::fs::write(
                        self.profile.join("window-visible-ack"), b"0",
                    )
                    .ok()
                    .map(|_| false);
                }
                WindowEvent::CloseRequested => event_loop.exit(),
                WindowEvent::Focused(true) if self.visible => {
                    if let Some(webview) = self.webview.as_ref() {
                        let _ = webview.focus();
                    }
                }
                WindowEvent::Resized(size) if self.detached => {
                    if let Some(webview) = self.webview.as_ref() {
                        let _ = webview.set_bounds(webview_bounds(size));
                    }
                }
                _ => {}
            }
        }
        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            // Only this app's private profile receives visibility commands.
            // Keeping the authenticated view hidden preserves the download
            // capability without leaving a website window in the jobs workflow.
            if self.profile.join("window-stop").exists()
                || unsafe {
                    windows_sys::Win32::UI::WindowsAndMessaging::IsWindow(self.parent.get() as _)
                } == 0
            {
                event_loop.exit();
                return;
            }
            let bounds = if self.detached {
                None
            } else {
                std::fs::read_to_string(self.profile.join("window-bounds"))
                    .ok()
                    .and_then(|text| parse_bounds(&text))
            };
            if let (Some(window), Some([x, y, w, h])) = (self.window.as_ref(), bounds) {
                if bounds != self.last_bounds {
                    window.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
                    let _ = window
                        .request_inner_size(winit::dpi::PhysicalSize::new(w as u32, h as u32));
                }
            }
            self.last_bounds = bounds;
            if self.detached {
                if let Some(title) = read_control_text(&self.profile.join("window-title"), 1024)
                    .map(|title| title.trim().replace(['\r', '\n', '\0'], " "))
                    .filter(|title| !title.is_empty())
                {
                    if self.last_window_title.as_ref() != Some(&title) {
                        if let Some(window) = self.window.as_ref() {
                            window.set_title(&title);
                            self.last_window_title = Some(title);
                        }
                    }
                }
            }
            let mut command = [0_u8; 2];
            let visible = std::fs::File::open(self.profile.join("window-visible"))
                .and_then(|mut file| file.read(&mut command))
                .ok()
                .and_then(|len| match &command[..len] {
                    b"0" => Some(false),
                    b"1" => Some(true),
                    _ => None,
                });
            let visible = visible.map(|value| {
                value
                    && (self.detached || bounds.is_some())
                    && !(self.detached && self.profile.join("window-closed").exists())
            });
            if self.last_visibility != visible {
                if let Some(visible) = visible {
                    if self.set_visible(visible).is_ok()
                        && std::fs::write(
                            self.profile.join("window-visible-ack"),
                            if visible { b"1" } else { b"0" },
                        )
                        .is_ok()
                    {
                        self.last_visibility = Some(visible);
                    }
                }
            }
            if self.visible && self.profile.join("window-focus").is_file() {
                let _ = std::fs::remove_file(self.profile.join("window-focus"));
                self.focus_visible();
            }
            // Initialization runs for every new document. Reapply the current
            // script after navigation as well, so live configuration changes
            // cannot fall back to the snapshot from WebView creation.
            let page_loaded = self.editor_page_loaded.swap(false, Ordering::AcqRel);
            if let (Some(script), Some(webview)) =
                (read_editor_script(&self.profile), self.webview.as_ref())
            {
                if (page_loaded || self.last_editor_script.as_ref() != Some(&script))
                    && webview.evaluate_script(&script).is_ok()
                {
                    self.last_editor_script = Some(script);
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

    impl App {
        fn set_visible(&mut self, visible: bool) -> Result<(), String> {
            let (Some(window), Some(webview)) = (self.window.as_ref(), self.webview.as_ref()) else {
                return Err("Het browservenster is nog niet gereed".into());
            };
            if self.visible == visible {
                return Ok(());
            }
            if visible {
                webview.set_visible(true).map_err(|error| error.to_string())?;
                self.visible = true;
                window.set_visible(true);
                self.focus_visible();
            } else {
                // Clear the flag first: queued focus notifications must never
                // move focus back into a browser which is being hidden.
                self.visible = false;
                let _ = std::fs::remove_file(self.profile.join("window-focus"));
                if !self.detached {
                    return_embedded_focus_to_parent(window, self.parent);
                }
                let result = webview.set_visible(false);
                window.set_visible(false);
                result.map_err(|error| error.to_string())?;
            }
            Ok(())
        }

        fn focus_visible(&self) {
            if !self.visible {
                return;
            }
            if self.detached {
                if let Some(window) = self.window.as_ref() {
                    window.set_minimized(false);
                    window.focus_window();
                }
            }
            if let Some(webview) = self.webview.as_ref() {
                let _ = webview.focus();
            }
        }
    }

    fn return_embedded_focus_to_parent(window: &Window, parent: NonZeroIsize) {
        use windows_sys::Win32::UI::{
            Input::KeyboardAndMouse::{GetFocus, SetFocus},
            WindowsAndMessaging::{GetForegroundWindow, IsChild},
        };
        let Ok(handle) = window.window_handle() else { return };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else { return };
        // The embedded child and its host share Windows input handling. Only
        // hand focus back from our own child while SleufBase is foreground;
        // a background login finishing must not activate another application.
        unsafe {
            let parent = parent.get() as _;
            let child = handle.hwnd.get() as _;
            let focus = GetFocus();
            if GetForegroundWindow() == parent
                && (focus == child || IsChild(child, focus) != 0)
            {
                SetFocus(parent);
            }
        }
    }

    fn webview_bounds(size: winit::dpi::PhysicalSize<u32>) -> wry::Rect {
        wry::Rect {
            position: wry::dpi::PhysicalPosition::new(0, 0).into(),
            size: wry::dpi::PhysicalSize::new(size.width, size.height).into(),
        }
    }

    fn read_editor_script(control: &Path) -> Option<String> {
        read_control_text(&control.join("page-editor.js"), MAX_EDITOR_SCRIPT_BYTES)
    }

    fn read_control_text(path: &Path, max_bytes: u64) -> Option<String> {
        let file = std::fs::File::open(path).ok()?;
        let mut script = String::new();
        file.take(max_bytes + 1)
            .read_to_string(&mut script)
            .ok()?;
        (!script.is_empty() && script.len() as u64 <= max_bytes).then_some(script)
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
            detached: control.join("window-detached").is_file(),
            last_bounds: None,
            profile: control.to_owned(),
            visible: false,
            last_visibility: None,
            last_window_title: None,
            last_editor_script: None,
            editor_page_loaded: Arc::new(AtomicBool::new(false)),
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
