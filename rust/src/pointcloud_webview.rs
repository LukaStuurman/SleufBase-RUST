//! An owned WebView2 window for the private local Potree editor.
//! It runs in a helper process so it cannot interfere with egui's event loop.

pub fn run(url: &str, parent: isize) -> Result<(), String> {
    #[cfg(windows)]
    { native::run(url, parent) }
    #[cfg(not(windows))]
    {
        let _ = (url, parent);
        Err("De ingebouwde puntenwolkeditor vereist Windows en WebView2".into())
    }
}

#[cfg(windows)]
mod native {
    use std::{
        num::NonZeroIsize,
        sync::{atomic::{AtomicBool, Ordering}, Arc},
        time::{Duration, Instant},
    };
    use winit::{
        application::ApplicationHandler,
        event::WindowEvent,
        event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
        platform::windows::{EventLoopBuilderExtWindows, WindowAttributesExtWindows},
        window::{Window, WindowId},
    };
    use wry::{NewWindowResponse, WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows};

    struct App {
        // The browser must be released before the HWND and profile disappear.
        webview: Option<WebView>,
        window: Option<Window>,
        context: WebContext,
        _profile: tempfile::TempDir,
        url: String,
        parent: NonZeroIsize,
        close_ready: Arc<AtomicBool>,
        loaded: Arc<AtomicBool>,
        error: Option<String>,
    }

    impl ApplicationHandler for App {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_some() || self.error.is_some() { return; }
            let result = (|| {
                if !parent_alive(self.parent) { return Err("Het SleufBase-hoofdvenster is gesloten".to_owned()); }
                let window = event_loop.create_window(Window::default_attributes()
                    .with_title("SleufBase — Puntenwolkeditor")
                    .with_inner_size(winit::dpi::PhysicalSize::new(1280, 840))
                    .with_min_inner_size(winit::dpi::PhysicalSize::new(760, 540))
                    .with_resizable(true)
                    .with_owner_window(self.parent.get() as _))
                    .map_err(|e| e.to_string())?;
                let allowed = self.url.clone();
                let close_ready = Arc::clone(&self.close_ready);
                let loaded = Arc::clone(&self.loaded);
                let webview = WebViewBuilder::new_with_web_context(&mut self.context)
                    .with_url(&self.url)
                    .with_devtools(false)
                    .with_general_autofill_enabled(false)
                    .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
                    .with_navigation_handler(move |url| {
                        // Only this session's exact editor document may navigate.
                        // Workers/assets are subresources and retain normal access.
                        url == allowed || url == format!("{allowed}index.html")
                    })
                    .with_on_page_load_handler(move |event, _| {
                        if matches!(event, wry::PageLoadEvent::Finished) {
                            loaded.store(true, Ordering::Release);
                        }
                    })
                    .with_ipc_handler(move |request| {
                        if request.body() == "close" {
                            close_ready.store(true, Ordering::Release);
                        }
                    })
                    .with_bounds(bounds(window.inner_size()))
                    .build_as_child(&window)
                    .map_err(|e| format!("Puntenwolkeditor kon niet starten. Controleer de Microsoft WebView2 Runtime: {e}"))?;
                self.webview = Some(webview);
                self.window = Some(window);
                Ok::<_, String>(())
            })();
            if let Err(error) = result {
                self.error = Some(error);
                event_loop.exit();
            }
        }

        fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
            if self.window.as_ref().is_none_or(|window| window.id() != window_id) { return; }
            match event {
                WindowEvent::CloseRequested => {
                    if !self.loaded.load(Ordering::Acquire) {
                        // An editor document which never loaded cannot contain edits.
                        event_loop.exit();
                    } else if let Some(webview) = &self.webview {
                        // Do not destroy the window until the page confirms its
                        // pending save. On network/disk errors it stays open.
                        let _ = webview.evaluate_script(
                            "if (typeof window.sleufbaseBeforeClose === 'function') { window.sleufbaseBeforeClose(); } else { window.ipc.postMessage('close'); }"
                        );
                    }
                }
                WindowEvent::Resized(size) => {
                    if let Some(webview) = &self.webview { let _ = webview.set_bounds(bounds(size)); }
                }
                WindowEvent::Focused(true) => {
                    if let Some(webview) = &self.webview { let _ = webview.focus(); }
                }
                _ => {}
            }
        }

        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            if self.close_ready.load(Ordering::Acquire) || !parent_alive(self.parent) {
                event_loop.exit();
                return;
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(150)));
        }
    }

    fn parent_alive(parent: NonZeroIsize) -> bool {
        unsafe { windows_sys::Win32::UI::WindowsAndMessaging::IsWindow(parent.get() as _) != 0 }
    }

    fn bounds(size: winit::dpi::PhysicalSize<u32>) -> wry::Rect {
        wry::Rect {
            position: wry::dpi::PhysicalPosition::new(0, 0).into(),
            size: wry::dpi::PhysicalSize::new(size.width, size.height).into(),
        }
    }

    pub(super) fn run(url: &str, parent: isize) -> Result<(), String> {
        let parsed = reqwest::Url::parse(url).map_err(|_| "Ongeldig editoradres")?;
        let token = parsed.path().trim_matches('/');
        if parsed.scheme() != "http" || parsed.host_str() != Some("127.0.0.1")
            || parsed.port().is_none() || !parsed.username().is_empty() || parsed.password().is_some()
            || parsed.query().is_some() || parsed.fragment().is_some()
            || !parsed.path().ends_with('/') || token.len() != 48
            || !token.bytes().all(|b| b.is_ascii_hexdigit())
        { return Err("Ongeldig lokaal puntenwolkadres".into()); }
        let parent = NonZeroIsize::new(parent).ok_or("SleufBase-hoofdvenster ontbreekt")?;
        if !parent_alive(parent) { return Err("Het SleufBase-hoofdvenster is gesloten".into()); }
        let profile = tempfile::Builder::new().prefix("sleufbase-pointcloud-").tempdir()
            .map_err(|e| format!("Editorprofiel maken: {e}"))?;
        let context = WebContext::new(Some(profile.path().to_path_buf()));
        let mut builder = EventLoop::builder();
        builder.with_any_thread(true);
        let event_loop = builder.build().map_err(|e| format!("Editorvenster starten: {e}"))?;
        let mut app = App {
            webview: None,
            window: None,
            context,
            _profile: profile,
            url: url.to_owned(),
            parent,
            close_ready: Arc::new(AtomicBool::new(false)),
            loaded: Arc::new(AtomicBool::new(false)),
            error: None,
        };
        event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
        app.error.map_or(Ok(()), Err)
    }
}
