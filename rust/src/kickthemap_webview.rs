//! Process-local WebView2 window owned by SleufBase, never an external browser.
//! Authentication and reCAPTCHA are completed manually by the operator.
use std::path::Path;

pub fn run(profile: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        native::run(profile, false)
    }
    #[cfg(not(windows))]
    {
        let _ = profile;
        Err("De ingebouwde KickTheMap-browser vereist Windows en WebView2".into())
    }
}
#[cfg(all(test, windows))]
pub(crate) fn run_fixture(profile: &Path) -> Result<(), String> {
    native::run(profile, true)
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::time::{Duration, Instant};
    use winit::{
        application::ApplicationHandler,
        event::WindowEvent,
        event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
        platform::windows::EventLoopBuilderExtWindows,
        window::{Window, WindowId},
    };
    use wry::{WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows};
    struct App {
        // Drop the webview before its native parent and profile context.
        webview: Option<WebView>,
        window: Option<Window>,
        context: WebContext,
        error: Option<String>,
        fixture: bool,
        deadline: Option<Instant>,
    }
    impl ApplicationHandler for App {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_some() || self.error.is_some() {
                return;
            }
            let result = (|| {
                let window = event_loop
                    .create_window(
                        Window::default_attributes()
                            .with_title("SleufBase — Aanmelden bij KickTheMap")
                            .with_inner_size(winit::dpi::LogicalSize::new(1100., 800.))
                            .with_min_inner_size(winit::dpi::LogicalSize::new(640., 480.))
                            .with_visible(!self.fixture),
                    )
                    .map_err(|e| e.to_string())?;
                let builder = WebViewBuilder::new_with_web_context(&mut self.context)
                    .with_additional_browser_args(
                        "--remote-debugging-port=0 --remote-debugging-address=127.0.0.1",
                    )
                    .with_devtools(false);
                let builder = if self.fixture {
                    builder.with_html("<!doctype html><title>SleufBase local browser fixture</title><meta name='csrf-token' content='fixture-only'><p>Local fixture; no account or network.</p>")
                } else {
                    builder.with_url("https://www.my.kickthemap.com/signin")
                };
                let webview=builder.build(&window).map_err(|e|format!("Ingebouwde browser kon niet starten. Controleer of Microsoft WebView2 Runtime is geïnstalleerd: {e}"))?;
                self.webview = Some(webview);
                self.window = Some(window);
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
            if let Some(deadline) = self.deadline {
                if Instant::now() >= deadline {
                    event_loop.exit();
                } else {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
                }
            } else {
                event_loop.set_control_flow(ControlFlow::Wait);
            }
        }
    }
    pub(super) fn run(profile: &Path, fixture: bool) -> Result<(), String> {
        if !profile.is_dir() {
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
            deadline: fixture.then(|| Instant::now() + Duration::from_secs(20)),
        };
        event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
        if let Some(error) = app.error.take() {
            Err(error)
        } else {
            Ok(())
        }
    }
}
