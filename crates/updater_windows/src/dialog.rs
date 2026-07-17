use anyhow::Context as _;
use std::mem::{self, ManuallyDrop};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            BeginPaint, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, COLOR_WINDOW, CreateFontW,
            DEFAULT_CHARSET, EndPaint, FW_NORMAL, GetSysColorBrush, HDC, HFONT, HGDIOBJ, LOGFONTW,
            OUT_TT_ONLY_PRECIS, PAINTSTRUCT, SelectObject, TextOutW,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::{
                ICC_PROGRESS_CLASS, INITCOMMONCONTROLSEX, InitCommonControlsEx, PBM_SETRANGE32,
                PBM_SETSTEP, PROGRESS_CLASS,
            },
            WindowsAndMessaging::{
                CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow,
                GetDesktopWindow, GetWindowRect, HICON, IDC_ARROW, IMAGE_ICON, LR_DEFAULTSIZE,
                LR_SHARED, LoadCursorW, LoadImageW, PostQuitMessage, RegisterClassW,
                SPI_GETICONTITLELOGFONT, SW_SHOW, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
                SendMessageW, ShowWindow, SystemParametersInfoW, WINDOW_EX_STYLE, WM_CLOSE,
                WM_DESTROY, WM_PAINT, WNDCLASSW, WS_CAPTION, WS_CHILD, WS_EX_TOPMOST, WS_POPUP,
                WS_VISIBLE,
            },
        },
    },
    core::{Error as WindowsError, HSTRING, Owned, PCWSTR},
};

use crate::{WM_TERMINATE, updater::JOBS};

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
        let device_context = unsafe { BeginPaint(window, &mut paint) };
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
        if let Err(error) = unsafe { EndPaint(self.window, &self.paint).ok() } {
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
    unsafe { InitCommonControlsEx(&controls) }
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
    if unsafe { RegisterClassW(&window_class) } == 0 {
        return Err(WindowsError::from_win32()).context("failed to register updater window class");
    }

    let mut desktop = RECT::default();
    // SAFETY: `GetDesktopWindow` returns a valid system-owned `HWND`.
    let desktop_window = unsafe { GetDesktopWindow() };
    // SAFETY: `desktop_window` is a valid `HWND` and `desktop` provides writable storage for the
    // returned bounds.
    unsafe { GetWindowRect(desktop_window, &mut desktop) }
        .context("failed to read desktop bounds")?;
    let width = 400;
    let height = 150;
    // SAFETY: `class_name`, `module` and the window title remain valid for the duration of
    // this call.
    let window = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST,
            class_name,
            windows::core::w!("Zaku"),
            WS_POPUP | WS_CAPTION,
            desktop.right / 2 - width / 2,
            desktop.bottom / 2 - height / 2,
            width,
            height,
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
            20,
            50,
            340,
            35,
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
    unsafe { ShowWindow(window, SW_SHOW) };

    Ok(DialogWindow {
        window,
        progress_bar,
    })
}

// SAFETY: Windows invokes this function through the system ABI.
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
                let font_name = HSTRING::from(system_ui_font_name());
                // SAFETY: `font_name` remains valid for the duration of this call.
                let font = unsafe {
                    CreateFontW(
                        24,
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
                    if let Err(error) =
                        unsafe { TextOutW(paint.device_context, 20, 15, text.as_wide()).ok() }
                    {
                        log::error!("Failed to draw updater window text: {error}");
                    }
                } else {
                    log::error!("Failed to select updater window font");
                }
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
