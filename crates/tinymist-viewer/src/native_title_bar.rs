//! Native title-bar hit testing for decorationless windows.

#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicBool, Ordering};

use winit::window::WindowId;

#[cfg(any(test, target_os = "windows"))]
use vello::kurbo::{Point, Size};

#[cfg(any(test, target_os = "windows"))]
use crate::title_bar::{TITLE_BAR_HEIGHT, TitleBarButton, button_at_pos};

/// Installs native title-bar behavior for the given winit window.
#[cfg(target_os = "windows")]
pub fn install_for_window_id(window_id: WindowId) {
    windows::install_for_window_id(window_id);
}

/// Installs native title-bar behavior for the given winit window.
#[cfg(not(target_os = "windows"))]
pub fn install_for_window_id(_window_id: WindowId) {}

/// Consumes a native maximize-button activation that has already been handled by Windows.
#[cfg(target_os = "windows")]
pub(crate) fn take_native_maximize_activation() -> bool {
    NATIVE_MAXIMIZE_ACTIVATION.swap(false, Ordering::AcqRel)
}

/// Consumes a native maximize-button activation that has already been handled by Windows.
#[cfg(not(target_os = "windows"))]
pub(crate) fn take_native_maximize_activation() -> bool {
    false
}

#[cfg(target_os = "windows")]
static NATIVE_MAXIMIZE_ACTIVATION: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(any(test, target_os = "windows"))]
enum NativeTitleBarHit {
    Client,
    Caption,
    MaximizeButton,
    Resize(ResizeEdge),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(any(test, target_os = "windows"))]
enum ResizeEdge {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[cfg(any(test, target_os = "windows"))]
fn native_title_bar_hit_test(
    logical_size: Size,
    logical_pos: Point,
    primary_button_down: bool,
    resize_border: Size,
) -> Option<NativeTitleBarHit> {
    if logical_pos.x < 0.0
        || logical_pos.x >= logical_size.width
        || logical_pos.y < 0.0
        || logical_pos.y >= logical_size.height
    {
        return None;
    }

    if let Some(edge) = resize_edge_at_pos(logical_size, logical_pos, resize_border)
        && !primary_button_down
    {
        return Some(NativeTitleBarHit::Resize(edge));
    }

    if logical_pos.y >= TITLE_BAR_HEIGHT {
        return None;
    }

    match button_at_pos(logical_size, logical_pos) {
        Some(TitleBarButton::Maximize) => Some(NativeTitleBarHit::MaximizeButton),
        Some(_) => Some(NativeTitleBarHit::Client),
        None => Some(NativeTitleBarHit::Caption),
    }
}

#[cfg(any(test, target_os = "windows"))]
fn resize_edge_at_pos(size: Size, pos: Point, border: Size) -> Option<ResizeEdge> {
    if border.width <= 0.0 || border.height <= 0.0 {
        return None;
    }

    let border_x = border.width.min(size.width * 0.5);
    let border_y = border.height.min(size.height * 0.5);
    let left = pos.x < border_x;
    let right = pos.x >= size.width - border_x;
    let top = pos.y < border_y;
    let bottom = pos.y >= size.height - border_y;

    match (left, right, top, bottom) {
        (true, _, true, _) => Some(ResizeEdge::TopLeft),
        (_, true, true, _) => Some(ResizeEdge::TopRight),
        (true, _, _, true) => Some(ResizeEdge::BottomLeft),
        (_, true, _, true) => Some(ResizeEdge::BottomRight),
        (true, _, _, _) => Some(ResizeEdge::Left),
        (_, true, _, _) => Some(ResizeEdge::Right),
        (_, _, true, _) => Some(ResizeEdge::Top),
        (_, _, _, true) => Some(ResizeEdge::Bottom),
        _ => None,
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use std::collections::HashSet;
    use std::mem;
    use std::sync::atomic::Ordering;
    use std::sync::{Mutex, OnceLock};

    use super::{
        NATIVE_MAXIMIZE_ACTIVATION, NativeTitleBarHit, ResizeEdge, native_title_bar_hit_test,
    };
    use vello::kurbo::{Point, Size};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
    use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyState, TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, TrackMouseEvent, VK_LBUTTON,
    };
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClientRect, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTCLIENT, HTLEFT,
        HTMAXBUTTON, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, IsZoomed, SM_CXPADDEDBORDER,
        SM_CXSIZEFRAME, SM_CYSIZEFRAME, SW_MAXIMIZE, SW_RESTORE, SendMessageW, ShowWindow,
        WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCDESTROY, WM_NCHITTEST, WM_NCLBUTTONDOWN,
        WM_NCLBUTTONUP, WM_NCMOUSELEAVE, WM_NCMOUSEMOVE,
    };
    use winit::window::WindowId;

    const SUBCLASS_ID: usize = 0x544d_5654;
    const BASE_DPI: f64 = 96.0;
    const FALLBACK_RESIZE_BORDER_WIDTH: f64 = 8.0;
    const WM_MOUSELEAVE: u32 = 0x02A3;

    pub(super) fn install_for_window_id(window_id: WindowId) {
        let Ok(hwnd_value) = usize::try_from(u64::from(window_id)) else {
            return;
        };
        if hwnd_value == 0 {
            return;
        }

        let Ok(mut installed) = installed_windows().lock() else {
            return;
        };
        if !installed.insert(hwnd_value) {
            return;
        }
        drop(installed);

        let hwnd = hwnd_value as HWND;
        // SAFETY: `window_id` comes from winit for a live Windows window on the event loop thread.
        let installed =
            unsafe { SetWindowSubclass(hwnd, Some(title_bar_subclass_proc), SUBCLASS_ID, 0) != 0 };
        if !installed && let Ok(mut installed_windows) = installed_windows().lock() {
            installed_windows.remove(&hwnd_value);
        }
    }

    unsafe extern "system" fn title_bar_subclass_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _subclass_id: usize,
        _ref_data: usize,
    ) -> LRESULT {
        if message == WM_NCDESTROY {
            remove_subclass(hwnd);
            // SAFETY: Forwarding the original window message to the next subclass/default proc.
            return unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
        }

        if message != WM_NCHITTEST {
            if message == WM_NCMOUSEMOVE && wparam == HTMAXBUTTON as WPARAM {
                set_maximize_button_hovered(hwnd, true);
                // SAFETY: Forwarding keeps Windows 11 snap-layout hover behavior active.
                let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
                bridge_maximize_mouse_move(hwnd, lparam);
                return result;
            }

            if message == WM_NCMOUSELEAVE {
                set_maximize_button_hovered(hwnd, false);
                bridge_maximize_mouse_leave(hwnd);
                // SAFETY: Forwarding messages we do not handle to the next subclass/default proc.
                return unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
            }

            if message == WM_MOUSELEAVE && is_maximize_button_hovered(hwnd) {
                return 0;
            }

            if message == WM_NCLBUTTONDOWN && wparam == HTMAXBUTTON as WPARAM {
                set_maximize_button_hovered(hwnd, true);
                set_maximize_button_pressed(hwnd, true);
                bridge_maximize_mouse_move(hwnd, lparam);
                send_client_mouse_message(hwnd, WM_LBUTTONDOWN, lparam);
                return 0;
            }

            if message == WM_NCLBUTTONUP && take_maximize_button_pressed(hwnd) {
                bridge_maximize_mouse_move(hwnd, lparam);
                let released_on_maximize =
                    hit_test(hwnd, lparam, false) == Some(NativeTitleBarHit::MaximizeButton);
                set_maximize_button_hovered(hwnd, released_on_maximize);
                send_client_mouse_up(hwnd, lparam, released_on_maximize);
                if released_on_maximize {
                    toggle_window_maximized(hwnd);
                }
                return 0;
            }

            // SAFETY: Forwarding messages we do not handle to the next subclass/default proc.
            return unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
        }

        // SAFETY: Forwarding first preserves winit/DefWindowProc resize-border hit tests.
        let default_hit = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
        if default_hit != HTCLIENT as LRESULT {
            return default_hit;
        }

        match hit_test(hwnd, lparam, is_primary_button_down()) {
            Some(NativeTitleBarHit::Caption) => HTCAPTION as LRESULT,
            Some(NativeTitleBarHit::MaximizeButton) => HTMAXBUTTON as LRESULT,
            Some(NativeTitleBarHit::Resize(edge)) => resize_edge_to_hit(edge) as LRESULT,
            Some(NativeTitleBarHit::Client) | None => default_hit,
        }
    }

    fn hit_test(
        hwnd: HWND,
        lparam: LPARAM,
        primary_button_down: bool,
    ) -> Option<NativeTitleBarHit> {
        let scale_factor = scale_factor_for_window(hwnd);
        let client_size = client_size(hwnd)?;
        let client_pos = client_position_from_lparam(hwnd, lparam)?;
        let resize_border = resize_border_size(hwnd, scale_factor);
        let logical_size = Size::new(
            f64::from(client_size.0) / scale_factor,
            f64::from(client_size.1) / scale_factor,
        );
        let logical_pos = Point::new(
            f64::from(client_pos.x) / scale_factor,
            f64::from(client_pos.y) / scale_factor,
        );

        native_title_bar_hit_test(
            logical_size,
            logical_pos,
            primary_button_down,
            resize_border,
        )
    }

    fn installed_windows() -> &'static Mutex<HashSet<usize>> {
        static INSTALLED: OnceLock<Mutex<HashSet<usize>>> = OnceLock::new();
        INSTALLED.get_or_init(|| Mutex::new(HashSet::new()))
    }

    fn pressed_maximize_windows() -> &'static Mutex<HashSet<usize>> {
        static PRESSED: OnceLock<Mutex<HashSet<usize>>> = OnceLock::new();
        PRESSED.get_or_init(|| Mutex::new(HashSet::new()))
    }

    fn hovered_maximize_windows() -> &'static Mutex<HashSet<usize>> {
        static HOVERED: OnceLock<Mutex<HashSet<usize>>> = OnceLock::new();
        HOVERED.get_or_init(|| Mutex::new(HashSet::new()))
    }

    fn remove_subclass(hwnd: HWND) {
        if let Ok(mut installed) = installed_windows().lock() {
            installed.remove(&(hwnd as usize));
        }
        if let Ok(mut pressed) = pressed_maximize_windows().lock() {
            pressed.remove(&(hwnd as usize));
        }
        if let Ok(mut hovered) = hovered_maximize_windows().lock() {
            hovered.remove(&(hwnd as usize));
        }

        // SAFETY: The subclass was installed by `install_for_window_id` with this proc and id.
        unsafe {
            RemoveWindowSubclass(hwnd, Some(title_bar_subclass_proc), SUBCLASS_ID);
        }
    }

    fn set_maximize_button_pressed(hwnd: HWND, pressed: bool) {
        let Ok(mut pressed_windows) = pressed_maximize_windows().lock() else {
            return;
        };

        if pressed {
            pressed_windows.insert(hwnd as usize);
        } else {
            pressed_windows.remove(&(hwnd as usize));
        }
    }

    fn take_maximize_button_pressed(hwnd: HWND) -> bool {
        let Ok(mut pressed_windows) = pressed_maximize_windows().lock() else {
            return false;
        };
        pressed_windows.remove(&(hwnd as usize))
    }

    fn set_maximize_button_hovered(hwnd: HWND, hovered: bool) {
        let Ok(mut hovered_windows) = hovered_maximize_windows().lock() else {
            return;
        };

        if hovered {
            hovered_windows.insert(hwnd as usize);
        } else {
            hovered_windows.remove(&(hwnd as usize));
        }
    }

    fn is_maximize_button_hovered(hwnd: HWND) -> bool {
        let Ok(hovered_windows) = hovered_maximize_windows().lock() else {
            return false;
        };
        hovered_windows.contains(&(hwnd as usize))
    }

    fn scale_factor_for_window(hwnd: HWND) -> f64 {
        // SAFETY: `hwnd` is the window currently being processed by the subclass callback.
        let dpi = unsafe { GetDpiForWindow(hwnd) };
        if dpi == 0 {
            1.0
        } else {
            f64::from(dpi) / BASE_DPI
        }
    }

    fn bridge_maximize_mouse_move(hwnd: HWND, screen_lparam: LPARAM) {
        send_client_mouse_message(hwnd, WM_MOUSEMOVE, screen_lparam);
        track_non_client_mouse_leave(hwnd);
    }

    fn bridge_maximize_mouse_leave(hwnd: HWND) {
        // SAFETY: Sending a client leave lets winit/Masonry clear the custom button hover state.
        unsafe {
            SendMessageW(hwnd, WM_MOUSELEAVE, 0, 0);
        }
    }

    fn send_client_mouse_message(hwnd: HWND, message: u32, screen_lparam: LPARAM) {
        let Some(client_lparam) = client_mouse_lparam_from_screen_lparam(hwnd, screen_lparam)
        else {
            return;
        };

        // SAFETY: `hwnd` is the current window; the sent message is processed by winit's
        // regular client-area mouse path, keeping the Xilem title-bar widget visuals in sync.
        unsafe {
            SendMessageW(hwnd, message, 0, client_lparam);
        }
    }

    fn send_client_mouse_up(hwnd: HWND, screen_lparam: LPARAM, suppress_activation: bool) {
        if suppress_activation {
            NATIVE_MAXIMIZE_ACTIVATION.store(true, Ordering::Release);
        }

        send_client_mouse_message(hwnd, WM_LBUTTONUP, screen_lparam);

        if suppress_activation {
            NATIVE_MAXIMIZE_ACTIVATION.store(false, Ordering::Release);
        }
    }

    fn toggle_window_maximized(hwnd: HWND) {
        // SAFETY: `hwnd` is the window currently being processed by the subclass callback.
        let is_maximized = unsafe { IsZoomed(hwnd) } != 0;
        let command = if is_maximized {
            SW_RESTORE
        } else {
            SW_MAXIMIZE
        };
        // SAFETY: `hwnd` is the window currently being processed by the subclass callback.
        unsafe {
            ShowWindow(hwnd, command);
        }
    }

    fn track_non_client_mouse_leave(hwnd: HWND) {
        let mut event = TRACKMOUSEEVENT {
            cbSize: mem::size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE | TME_NONCLIENT,
            hwndTrack: hwnd,
            dwHoverTime: 0,
        };
        // SAFETY: `event` is fully initialized and refers to the current window.
        unsafe {
            TrackMouseEvent(&mut event);
        }
    }

    fn client_mouse_lparam_from_screen_lparam(hwnd: HWND, screen_lparam: LPARAM) -> Option<LPARAM> {
        let point = client_position_from_lparam(hwnd, screen_lparam)?;
        Some(pack_signed_words(point.x, point.y))
    }

    fn resize_border_size(hwnd: HWND, scale_factor: f64) -> Size {
        // SAFETY: `hwnd` is the window currently being processed by the subclass callback.
        if unsafe { IsZoomed(hwnd) } != 0 {
            return Size::ZERO;
        }

        // SAFETY: `hwnd` is the window currently being processed by the subclass callback.
        let dpi = unsafe { GetDpiForWindow(hwnd) };
        let dpi = if dpi == 0 { BASE_DPI as u32 } else { dpi };

        // SAFETY: GetSystemMetricsForDpi reads process-global system metrics for this DPI.
        let frame_x = unsafe { GetSystemMetricsForDpi(SM_CXSIZEFRAME, dpi) };
        // SAFETY: GetSystemMetricsForDpi reads process-global system metrics for this DPI.
        let frame_y = unsafe { GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi) };
        // SAFETY: GetSystemMetricsForDpi reads process-global system metrics for this DPI.
        let padded = unsafe { GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi) };

        let fallback = (FALLBACK_RESIZE_BORDER_WIDTH * scale_factor).round() as i32;
        let width = (frame_x + padded).max(fallback).max(0);
        let height = (frame_y + padded).max(fallback).max(0);

        Size::new(
            f64::from(width) / scale_factor,
            f64::from(height) / scale_factor,
        )
    }

    fn resize_edge_to_hit(edge: ResizeEdge) -> u32 {
        match edge {
            ResizeEdge::Left => HTLEFT,
            ResizeEdge::Right => HTRIGHT,
            ResizeEdge::Top => HTTOP,
            ResizeEdge::Bottom => HTBOTTOM,
            ResizeEdge::TopLeft => HTTOPLEFT,
            ResizeEdge::TopRight => HTTOPRIGHT,
            ResizeEdge::BottomLeft => HTBOTTOMLEFT,
            ResizeEdge::BottomRight => HTBOTTOMRIGHT,
        }
    }

    fn client_size(hwnd: HWND) -> Option<(i32, i32)> {
        let mut rect = RECT::default();
        // SAFETY: `rect` is a valid out pointer and `hwnd` is owned by the current callback.
        let ok = unsafe { GetClientRect(hwnd, &mut rect) } != 0;
        if !ok {
            return None;
        }

        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        (width > 0 && height > 0).then_some((width, height))
    }

    fn client_position_from_lparam(hwnd: HWND, lparam: LPARAM) -> Option<POINT> {
        let mut point = POINT {
            x: signed_low_word(lparam),
            y: signed_high_word(lparam),
        };
        // SAFETY: `point` is a valid in/out pointer and `hwnd` is owned by the current callback.
        let ok = unsafe { ScreenToClient(hwnd, &mut point) } != 0;
        ok.then_some(point)
    }

    fn signed_low_word(value: LPARAM) -> i32 {
        (value as u32 as u16 as i16).into()
    }

    fn signed_high_word(value: LPARAM) -> i32 {
        ((value as u32 >> 16) as u16 as i16).into()
    }

    fn pack_signed_words(low: i32, high: i32) -> LPARAM {
        let low = u32::from(low as i16 as u16);
        let high = u32::from(high as i16 as u16) << 16;
        (low | high) as LPARAM
    }

    fn is_primary_button_down() -> bool {
        // SAFETY: GetKeyState reads the calling thread's keyboard state and has no pointer input.
        unsafe { GetKeyState(i32::from(VK_LBUTTON)) < 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_title_bar_hit_test_promotes_empty_title_area_to_caption() {
        assert_eq!(
            native_title_bar_hit_test(
                Size::new(800.0, 600.0),
                Point::new(200.0, 20.0),
                false,
                Size::new(8.0, 8.0),
            ),
            Some(NativeTitleBarHit::Caption),
        );
    }

    #[test]
    fn native_title_bar_hit_test_promotes_maximize_button_to_native_hit() {
        assert_eq!(
            native_title_bar_hit_test(
                Size::new(800.0, 600.0),
                Point::new(730.0, 20.0),
                false,
                Size::new(8.0, 8.0),
            ),
            Some(NativeTitleBarHit::MaximizeButton),
        );
    }

    #[test]
    fn native_title_bar_hit_test_keeps_pressed_maximize_as_native_hit() {
        assert_eq!(
            native_title_bar_hit_test(
                Size::new(800.0, 600.0),
                Point::new(730.0, 20.0),
                true,
                Size::new(8.0, 8.0),
            ),
            Some(NativeTitleBarHit::MaximizeButton),
        );
    }

    #[test]
    fn native_title_bar_hit_test_keeps_other_buttons_in_client_area() {
        assert_eq!(
            native_title_bar_hit_test(
                Size::new(800.0, 600.0),
                Point::new(775.0, 20.0),
                false,
                Size::new(8.0, 8.0),
            ),
            Some(NativeTitleBarHit::Client),
        );
    }

    #[test]
    fn native_title_bar_hit_test_ignores_body_area() {
        assert_eq!(
            native_title_bar_hit_test(
                Size::new(800.0, 600.0),
                Point::new(200.0, 60.0),
                false,
                Size::new(8.0, 8.0),
            ),
            None,
        );
    }

    #[test]
    fn native_title_bar_hit_test_promotes_window_edges_to_resize_hits() {
        let size = Size::new(800.0, 600.0);
        let border = Size::new(8.0, 8.0);

        assert_eq!(
            native_title_bar_hit_test(size, Point::new(2.0, 2.0), false, border),
            Some(NativeTitleBarHit::Resize(ResizeEdge::TopLeft)),
        );
        assert_eq!(
            native_title_bar_hit_test(size, Point::new(797.0, 2.0), false, border),
            Some(NativeTitleBarHit::Resize(ResizeEdge::TopRight)),
        );
        assert_eq!(
            native_title_bar_hit_test(size, Point::new(2.0, 597.0), false, border),
            Some(NativeTitleBarHit::Resize(ResizeEdge::BottomLeft)),
        );
        assert_eq!(
            native_title_bar_hit_test(size, Point::new(797.0, 597.0), false, border),
            Some(NativeTitleBarHit::Resize(ResizeEdge::BottomRight)),
        );
        assert_eq!(
            native_title_bar_hit_test(size, Point::new(2.0, 200.0), false, border),
            Some(NativeTitleBarHit::Resize(ResizeEdge::Left)),
        );
        assert_eq!(
            native_title_bar_hit_test(size, Point::new(797.0, 200.0), false, border),
            Some(NativeTitleBarHit::Resize(ResizeEdge::Right)),
        );
        assert_eq!(
            native_title_bar_hit_test(size, Point::new(200.0, 2.0), false, border),
            Some(NativeTitleBarHit::Resize(ResizeEdge::Top)),
        );
        assert_eq!(
            native_title_bar_hit_test(size, Point::new(200.0, 597.0), false, border),
            Some(NativeTitleBarHit::Resize(ResizeEdge::Bottom)),
        );
    }
}
