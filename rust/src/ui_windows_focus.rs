//! Return keyboard focus to the native input window on an actual UI click.
use eframe::egui;
use windows_sys::Win32::UI::{
    Input::KeyboardAndMouse::{GetFocus, SetFocus},
    WindowsAndMessaging::GetForegroundWindow,
};

pub(super) fn repair_on_pointer_press(host: isize, ctx: &egui::Context) {
    if host == 0 || !ctx.input(|input| input.pointer.any_pressed()) {
        return;
    }
    // This runs on eframe's owning UI thread. A click in an already active
    // Win32 parent does not itself restore focus after an embedded WebView had
    // it. Never activate SleufBase while another application is foreground.
    unsafe {
        let host = host as _;
        if GetForegroundWindow() == host && GetFocus() != host {
            SetFocus(host);
            // Let winit deliver WM_SETFOCUS before the next text-edit frame.
            ctx.request_repaint();
        }
    }
}
