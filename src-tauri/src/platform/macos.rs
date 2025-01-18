use cocoa::{
    appkit::{NSView, NSWindow, NSWindowButton},
    base::id,
    foundation::NSRect,
};
use objc::{
    msg_send,
    runtime::{Object, Sel},
    sel, sel_impl,
};
use rand::{distr::Alphanumeric, Rng};
use std::ffi;
use tauri::{Listener, Runtime, WebviewWindow, WindowEvent};

const WINDOW_THEME_CHANGED: &str = "tauri://theme-changed";
const TRAFFIC_LIGHTS_PADDING_X: f64 = 14.0;
const TRAFFIC_LIGHTS_PADDING_Y: f64 = 16.0;

struct UnsafeNSWindowHandle(*mut ffi::c_void);

unsafe impl Send for UnsafeNSWindowHandle {}
unsafe impl Sync for UnsafeNSWindowHandle {}

pub fn initialize<R: Runtime>(webview_window: &WebviewWindow<R>) {
    position_traffic_lights(UnsafeNSWindowHandle(
        webview_window
            .ns_window()
            .expect("Failed to get NSWindow handle"),
    ));

    let window_instance = webview_window.clone();
    let theme_changed_event = webview_window.listen(WINDOW_THEME_CHANGED, move |_| {
        position_traffic_lights(UnsafeNSWindowHandle(
            window_instance
                .ns_window()
                .expect("Failed to get NSWindow handle"),
        ));
    });

    let window_instance = webview_window.clone();
    webview_window.on_window_event(move |event| {
        if let WindowEvent::Destroyed = event {
            window_instance.unlisten(theme_changed_event);
        }
    });

    initialize_window_delegate(webview_window);
}

fn position_traffic_lights(unsafe_ns_window_handle: UnsafeNSWindowHandle) {
    let ns_window = unsafe_ns_window_handle.0 as cocoa::base::id;

    unsafe {
        let close = ns_window.standardWindowButton_(NSWindowButton::NSWindowCloseButton);
        let miniaturize =
            ns_window.standardWindowButton_(NSWindowButton::NSWindowMiniaturizeButton);
        let zoom = ns_window.standardWindowButton_(NSWindowButton::NSWindowZoomButton);
        let title_bar_container_view = close.superview().superview();
        let close_rect: NSRect = objc::msg_send![close, frame];
        let button_height = close_rect.size.height;
        let title_bar_frame_height = button_height + TRAFFIC_LIGHTS_PADDING_Y;
        let mut title_bar_rect = NSView::frame(title_bar_container_view);
        title_bar_rect.size.height = title_bar_frame_height;
        title_bar_rect.origin.y = NSView::frame(ns_window).size.height - title_bar_frame_height;

        let _: () = objc::msg_send![title_bar_container_view, setFrame: title_bar_rect];
        let space_between = NSView::frame(miniaturize).origin.x - NSView::frame(close).origin.x;

        for (index, button) in [close, miniaturize, zoom].into_iter().enumerate() {
            let mut rect: NSRect = NSView::frame(button);
            rect.origin.x = TRAFFIC_LIGHTS_PADDING_X + (index as f64 * space_between);
            button.setFrameOrigin(rect.origin);
        }
    }
}

fn hide_traffic_lights(unsafe_ns_window_handle: UnsafeNSWindowHandle) {
    let ns_window = unsafe_ns_window_handle.0 as cocoa::base::id;

    unsafe {
        let close = ns_window.standardWindowButton_(NSWindowButton::NSWindowCloseButton);
        let miniaturize =
            ns_window.standardWindowButton_(NSWindowButton::NSWindowMiniaturizeButton);
        let zoom = ns_window.standardWindowButton_(NSWindowButton::NSWindowZoomButton);

        for button in [close, miniaturize, zoom] {
            let _: () = objc::msg_send![button, setHidden: true];
        }
    }
}

fn show_traffic_lights(unsafe_ns_window_handle: UnsafeNSWindowHandle) {
    let ns_window = unsafe_ns_window_handle.0 as cocoa::base::id;

    unsafe {
        let close = ns_window.standardWindowButton_(NSWindowButton::NSWindowCloseButton);
        let miniaturize =
            ns_window.standardWindowButton_(NSWindowButton::NSWindowMiniaturizeButton);
        let zoom = ns_window.standardWindowButton_(NSWindowButton::NSWindowZoomButton);

        for button in [close, miniaturize, zoom] {
            let _: () = objc::msg_send![button, setHidden: false];
        }
    }
}

fn initialize_window_delegate<R: Runtime>(webview_window: &WebviewWindow<R>) {
    unsafe {
        extern "C" fn on_window_did_resize<R: Runtime>(
            this: &Object,
            _cmd: Sel,
            _notification: cocoa::base::id,
        ) {
            with_window_state(this, |state: &mut WindowState| {
                position_traffic_lights(UnsafeNSWindowHandle(state.ns_window as *mut ffi::c_void));
            });
        }

        extern "C" fn on_window_did_exit_full_screen<R: Runtime>(
            this: &Object,
            _cmd: Sel,
            _notification: cocoa::base::id,
        ) {
            with_window_state(&*this, |state: &mut WindowState| {
                position_traffic_lights(UnsafeNSWindowHandle(state.ns_window as *mut ffi::c_void));
                show_traffic_lights(UnsafeNSWindowHandle(state.ns_window as *mut ffi::c_void));
            });
        }

        extern "C" fn on_window_will_exit_full_screen<R: Runtime>(
            this: &Object,
            _cmd: Sel,
            _notification: cocoa::base::id,
        ) {
            with_window_state(&*this, |state: &mut WindowState| {
                hide_traffic_lights(UnsafeNSWindowHandle(state.ns_window as *mut ffi::c_void));
            });
        }

        let ns_window =
            webview_window.ns_window().expect("Failed to get NSWindow") as cocoa::base::id;
        let current_delegate: cocoa::base::id = ns_window.delegate();
        let window_label = webview_window.label().to_string();
        let app_state = WindowState { ns_window };
        let app_box = Box::into_raw(Box::new(app_state)) as *mut ffi::c_void;
        let random_str: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(20)
            .map(char::from)
            .collect();
        let delegate_name = format!("windowDelegate_{}_{}", window_label, random_str);

        ns_window.setDelegate_(cocoa::delegate!(&delegate_name, {
            window: cocoa::base::id = ns_window,
            app_box: *mut ffi::c_void = app_box,
            toolbar: cocoa::base::id = cocoa::base::nil,
            super_delegate: cocoa::base::id = current_delegate,
            (windowDidResize:) => on_window_did_resize::<R> as extern fn(&Object, Sel, cocoa::base::id),
            (windowDidExitFullScreen:) => on_window_did_exit_full_screen::<R> as extern fn(&Object, Sel, cocoa::base::id),
            (windowWillExitFullScreen:) => on_window_will_exit_full_screen::<R> as extern fn(&Object, Sel, cocoa::base::id)
        }));
    }
}

#[derive(Debug)]
struct WindowState {
    ns_window: cocoa::base::id,
}

fn with_window_state<F: FnOnce(&mut WindowState)>(this: &Object, func: F) {
    let ptr = unsafe {
        let x: *mut ffi::c_void = *this.get_ivar("app_box");
        &mut *(x as *mut WindowState)
    };

    func(ptr);
}
