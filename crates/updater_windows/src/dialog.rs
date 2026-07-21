use anyhow::Context as _;
use std::mem::{self, ManuallyDrop};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            BeginPaint, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, COLOR_WINDOW, CreateFontW,
            DEFAULT_CHARSET, EndPaint, FW_NORMAL, GetSysColorBrush, HDC, HFONT, HGDIOBJ, LOGFONTW,
            InvalidateRect, OUT_TT_ONLY_PRECIS, PAINTSTRUCT, SelectObject, TextOutW,
        },
        System::{LibraryLoader::GetModuleHandleW, WindowsProgramming::MulDiv},
        UI::{
            Controls::{
                ICC_PROGRESS_CLASS, INITCOMMONCONTROLSEX, InitCommonControlsEx, PBM_SETRANGE32,
                PBM_SETSTEP, PROGRESS_CLASS,
            },
            HiDpi::{GetDpiForSystem, GetDpiForWindow},
            WindowsAndMessaging::{
                CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow,
                FindWindowExW, GetDesktopWindow, GetWindowRect, HICON, IDC_ARROW, IMAGE_ICON,
                LR_DEFAULTSIZE, LR_SHARED, LoadCursorW, LoadImageW, PostQuitMessage,
                RegisterClassW, SPI_GETICONTITLELOGFONT, SWP_NOACTIVATE, SWP_NOZORDER, SW_SHOW,
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SendMessageW, SetWindowPos, ShowWindow,
                SystemParametersInfoW, USER_DEFAULT_SCREEN_DPI, WINDOW_EX_STYLE, WM_CLOSE,
                WM_DESTROY, WM_DPICHANGED, WM_PAINT, WNDCLASSW, WS_CAPTION, WS_CHILD,
                WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
            },
        },
    },
    core::{Error as WindowsError, HSTRING, Owned, PCWSTR},
};

use crate::{WM_TERMINATE, updater::JOBS};

const DIALOG_LAYOUT: DialogLayout = DialogLayout {
    window_width: 400,
    window_height: 130,
    text_x: 20,
    text_y: 15,
    font_height: 24,
    progress_bar_x: 20,
    progress_bar_y: 50,
    progress_bar_width: 340,
    progress_bar_height: 15,
};

struct DialogLayout {
    window_width: i32,
    window_height: i32,
    text_x: i32,
    text_y: i32,
    font_height: i32,
    progress_bar_x: i32,
    progress_bar_y: i32,
    progress_bar_width: i32,
    progress_bar_height: i32,
}

impl DialogLayout {
    fn scaled(&self, dpi: u32) -> Self {
        let dpi = i32::try_from(dpi).expect("dpi should fit in i32");
        let default_dpi =
            i32::try_from(USER_DEFAULT_SCREEN_DPI).expect("default dpi should fit in i32");
        let scale = |value| {
            // SAFETY: `default_dpi` is the nonzero Windows default screen DPI.
            unsafe { MulDiv(value, dpi, default_dpi) }
        };

        Self {
            window_width: scale(self.window_width),
            window_height: scale(self.window_height),
            text_x: scale(self.text_x),
            text_y: scale(self.text_y),
            font_height: scale(self.font_height),
            progress_bar_x: scale(self.progress_bar_x),
            progress_bar_y: scale(self.progress_bar_y),
            progress_bar_width: scale(self.progress_bar_width),
            progress_bar_height: scale(self.progress_bar_height),
        }
    }
}

pub(crate) struct DialogWindow {
    pub(crate) window: HWND,
    pub(crate) progress_bar: HWND,
}

struct PaintSession {
    window: HWND,
    paint: PAINTSTRUCT,
    device_context: HDC,
}

impl PaintSession {
    fn new(window: HWND) -> Self {
        let mut paint = PAINTSTRUCT::default();
        // SAFETY: `window` is a valid `HWND` and `paint` provides writable storage for
        // `BeginPaint`.
        let device_context = unsafe { BeginPaint(window, &raw mut paint) };
        Self {
            window,
            paint,
            device_context,
        }
    }
}

impl Drop for PaintSession {
    fn drop(&mut self) {
        // SAFETY: `self.window` is a valid `HWND` and `self.paint` belongs to the same paint
        // session.
        if let Err(error) = unsafe { EndPaint(self.window, &raw const self.paint).ok() } {
            log::error!("Failed to end painting updater window: {error}");
        }
    }
}

struct FontSelection {
    device_context: HDC,
    previous_font: HGDIOBJ,
    font: ManuallyDrop<Owned<HFONT>>,
}

impl FontSelection {
    fn new(device_context: HDC, font: HFONT) -> Option<Self> {
        // SAFETY: `font` is a valid and uniquely owned `HFONT` returned by `CreateFontW`.
        let font = unsafe { Owned::new(font) };
        // SAFETY: `device_context` remains valid for the duration of this call.
        let previous_font = unsafe { SelectObject(device_context, (*font).into()) };
        if previous_font.is_invalid() {
            return None;
        }

        Some(Self {
            device_context,
            previous_font,
            font: ManuallyDrop::new(font),
        })
    }
}

impl Drop for FontSelection {
    fn drop(&mut self) {
        // SAFETY: `self.device_context` and the font previously selected into it remain valid for
        // the duration of this call.
        if unsafe { SelectObject(self.device_context, self.previous_font) }.is_invalid() {
            log::error!("Failed to restore updater window font");
            return;
        }

        // SAFETY: `self.font` remains initialized and uniquely owned after the previous font was
        // restored to `self.device_context`.
        unsafe { ManuallyDrop::drop(&mut self.font) };
    }
}

pub(crate) fn create_dialog_window() -> anyhow::Result<DialogWindow> {
    let controls = INITCOMMONCONTROLSEX {
        dwSize: u32::try_from(mem::size_of::<INITCOMMONCONTROLSEX>())
            .context("common controls structure size should fit in u32")?,
        dwICC: ICC_PROGRESS_CLASS,
    };
    // SAFETY: `controls` remains valid for the duration of this call and its fields satisfy the
    // requirements of `InitCommonControlsEx`.
    unsafe { InitCommonControlsEx(&raw const controls) }
        .ok()
        .context("failed to initialize Windows common controls")?;

    let class_name = windows::core::w!("Zaku-Updater-Dialog-Class");
    // SAFETY: `GetModuleHandleW` accepts `None` to select the current executable module.
    let module =
        unsafe { GetModuleHandleW(None) }.context("failed to get updater module handle")?;
    // `LoadImageW` interprets pointer value 1 as integer resource ID 1.
    let icon_resource = PCWSTR::from_raw(std::ptr::without_provenance(1));
    // SAFETY: `module` remains valid for the duration of this call and `icon_resource` encodes
    // resource ID 1 as required by `LoadImageW`.
    let icon = unsafe {
        LoadImageW(
            Some(module.into()),
            icon_resource,
            IMAGE_ICON,
            0,
            0,
            LR_DEFAULTSIZE | LR_SHARED,
        )
    }
    .context("failed to load updater icon")?;
    // SAFETY: `LoadCursorW` accepts `None` when loading a predefined cursor.
    let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }.context("failed to load arrow cursor")?;
    let window_class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        lpszClassName: class_name,
        style: CS_HREDRAW | CS_VREDRAW,
        hInstance: module.into(),
        hIcon: HICON(icon.0),
        hCursor: cursor,
        // SAFETY: `COLOR_WINDOW` identifies a system-owned brush.
        hbrBackground: unsafe { GetSysColorBrush(COLOR_WINDOW) },
        ..Default::default()
    };
    // SAFETY: `window_class` and the resources it references remain valid for the duration of
    // this call.
    if unsafe { RegisterClassW(&raw const window_class) } == 0 {
        return Err(WindowsError::from_thread())
            .context("failed to register updater window class");
    }

    let mut desktop = RECT::default();
    // SAFETY: `GetDesktopWindow` returns a valid system-owned `HWND`.
    let desktop_window = unsafe { GetDesktopWindow() };
    // SAFETY: `desktop_window` is a valid `HWND` and `desktop` provides writable storage for the
    // returned bounds.
    unsafe { GetWindowRect(desktop_window, &raw mut desktop) }
        .context("failed to read desktop bounds")?;
    // SAFETY: `GetDpiForSystem` has no preconditions.
    let layout = DIALOG_LAYOUT.scaled(unsafe { GetDpiForSystem() });
    let window_x = desktop.left + (desktop.right - desktop.left - layout.window_width) / 2;
    let window_y = desktop.top + (desktop.bottom - desktop.top - layout.window_height) / 2;
    // SAFETY: `class_name`, `module` and the window title remain valid for the duration of
    // this call.
    let window = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST,
            class_name,
            windows::core::w!("Zaku"),
            WS_POPUP | WS_CAPTION,
            window_x,
            window_y,
            layout.window_width,
            layout.window_height,
            None,
            None,
            Some(module.into()),
            None,
        )
    }
    .context("failed to create updater window")?;

    // SAFETY: `window` is a valid `HWND` and `PROGRESS_CLASS` identifies the class registered above
    // with `InitCommonControlsEx`.
    let progress_bar = match unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PROGRESS_CLASS,
            None,
            WS_CHILD | WS_VISIBLE,
            layout.progress_bar_x,
            layout.progress_bar_y,
            layout.progress_bar_width,
            layout.progress_bar_height,
            Some(window),
            None,
            None,
            None,
        )
    } {
        Ok(progress_bar) => progress_bar,
        Err(error) => {
            // SAFETY: `window` is a valid `HWND` and belongs to the current thread.
            if let Err(destroy_error) = unsafe { DestroyWindow(window) } {
                log::error!("Failed to destroy updater window: {destroy_error}");
            }
            return Err(error).context("failed to create updater progress bar");
        }
    };
    let progress_max = isize::try_from(JOBS.len()).expect("job count should fit in isize");
    // SAFETY: `progress_bar` is a valid `HWND` and the message parameters satisfy `PBM_SETRANGE32`.
    unsafe {
        SendMessageW(
            progress_bar,
            PBM_SETRANGE32,
            Some(WPARAM(0)),
            Some(LPARAM(progress_max)),
        )
    };
    // SAFETY: `progress_bar` is a valid `HWND` and the message parameters satisfy `PBM_SETSTEP`.
    unsafe { SendMessageW(progress_bar, PBM_SETSTEP, Some(WPARAM(1)), None) };
    // SAFETY: `window` is a valid `HWND`.
    unsafe { ShowWindow(window, SW_SHOW) }.as_bool();

    Ok(DialogWindow {
        window,
        progress_bar,
    })
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_PAINT => {
            let paint = PaintSession::new(window);
            if paint.device_context.is_invalid() {
                log::error!("Failed to begin painting updater window");
            } else {
                // SAFETY: `window` is a valid `HWND`.
                let layout = DIALOG_LAYOUT.scaled(unsafe { GetDpiForWindow(window) });
                let font_name = HSTRING::from(system_ui_font_name());
                // SAFETY: `font_name` remains valid for the duration of this call.
                let font = unsafe {
                    CreateFontW(
                        layout.font_height,
                        0,
                        0,
                        0,
                        FW_NORMAL.0.cast_signed(),
                        0,
                        0,
                        0,
                        DEFAULT_CHARSET,
                        OUT_TT_ONLY_PRECIS,
                        CLIP_DEFAULT_PRECIS,
                        CLEARTYPE_QUALITY,
                        0,
                        &font_name,
                    )
                };
                if font.is_invalid() {
                    log::error!("Failed to create updater window font");
                } else if let Some(_font_selection) = FontSelection::new(paint.device_context, font)
                {
                    let text = HSTRING::from("Updating Zaku...");
                    // SAFETY: `paint.device_context` is a valid `HDC`.
                    let result = unsafe {
                        TextOutW(
                            paint.device_context,
                            layout.text_x,
                            layout.text_y,
                            &text,
                        )
                        .ok()
                    };
                    if let Err(error) = result {
                        log::error!("Failed to draw updater window text: {error}");
                    }
                } else {
                    log::error!("Failed to select updater window font");
                }
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            // SAFETY: Windows provides `lparam` as a valid `RECT` pointer for this call.
            let suggested_bounds = unsafe {
                &*std::ptr::with_exposed_provenance::<RECT>(
                    usize::try_from(lparam.0)
                        .expect("suggested bounds address should fit in usize"),
                )
            };
            // SAFETY: `window` is a valid `HWND`.
            if let Err(error) = unsafe {
                SetWindowPos(
                    window,
                    None,
                    suggested_bounds.left,
                    suggested_bounds.top,
                    suggested_bounds.right - suggested_bounds.left,
                    suggested_bounds.bottom - suggested_bounds.top,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                )
            } {
                log::error!("Failed to resize updater window for DPI change: {error}");
            }

            // SAFETY: `window` is a valid `HWND` and `PROGRESS_CLASS` is a valid class name.
            match unsafe { FindWindowExW(Some(window), None, PROGRESS_CLASS, PCWSTR::null()) } {
                Ok(progress_bar) => {
                    // SAFETY: `window` is a valid `HWND`.
                    let layout = DIALOG_LAYOUT.scaled(unsafe { GetDpiForWindow(window) });
                    // SAFETY: `progress_bar` is a valid child `HWND`.
                    if let Err(error) = unsafe {
                        SetWindowPos(
                            progress_bar,
                            None,
                            layout.progress_bar_x,
                            layout.progress_bar_y,
                            layout.progress_bar_width,
                            layout.progress_bar_height,
                            SWP_NOACTIVATE | SWP_NOZORDER,
                        )
                    } {
                        log::error!("Failed to resize updater progress bar for DPI change: {error}");
                    }
                }
                Err(error) => {
                    log::error!("Failed to find updater progress bar after DPI change: {error}");
                }
            }

            // SAFETY: `window` is a valid `HWND`.
            if let Err(error) = unsafe { InvalidateRect(Some(window), None, true).ok() } {
                log::error!("Failed to redraw updater window after DPI change: {error}");
            }
            LRESULT(0)
        }
        WM_TERMINATE => {
            // SAFETY: `window` is a valid `HWND` and belongs to the current thread.
            if let Err(error) = unsafe { DestroyWindow(window) } {
                log::error!("Failed to destroy updater window: {error}");
                // SAFETY: The updater message loop runs on the current thread.
                unsafe { PostQuitMessage(1) };
            }
            LRESULT(0)
        }
        WM_CLOSE => LRESULT(0),
        WM_DESTROY => {
            // SAFETY: The updater message loop runs on the current thread.
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => {
            // SAFETY: Windows provides the arguments required by `DefWindowProcW`.
            unsafe { DefWindowProcW(window, message, wparam, lparam) }
        }
    }
}

fn system_ui_font_name() -> String {
    let mut font = LOGFONTW::default();
    let size =
        u32::try_from(mem::size_of::<LOGFONTW>()).expect("font structure size should fit in u32");
    // SAFETY: `font` provides writable storage and `size` satisfies `SPI_GETICONTITLELOGFONT`.
    if let Err(error) = unsafe {
        SystemParametersInfoW(
            SPI_GETICONTITLELOGFONT,
            size,
            Some(std::ptr::from_mut(&mut font).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    {
        log::warn!("Failed to read system UI font: {error}");
        return "MS Shell Dlg".to_string();
    }

    String::from_utf16_lossy(&font.lfFaceName)
        .trim_matches(char::from(0))
        .to_string()
}
