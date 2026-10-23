// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The few things that are the operating system's own: the notification-area icon with its notices, the keyboard hook that lets the in-game menu
//! take keys from a game, and where a game's window is. Everything is Windows-only; elsewhere these do nothing, so the rest builds and tests.

/// What the tray icon asks of the launcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    Open,
    Library,
    Store,
    Settings,
    CheckUpdates,
    ReportProblem,
    Exit,
}

/// A game window's client area in screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClientRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[cfg(windows)]
pub use imp::*;
#[cfg(not(windows))]
pub use other::*;

#[cfg(not(windows))]
mod other {
    use super::{ClientRect, TrayEvent};
    use std::sync::mpsc::Sender;

    pub struct Tray;
    impl Tray {
        pub fn start(_events: Sender<TrayEvent>) -> Option<Self> {
            None
        }
        pub fn notify(&self, _title: &str, _text: &str) {}
    }
    pub struct KeyboardHook;
    impl KeyboardHook {
        pub fn install(_handler: Box<dyn Fn(u32, bool) -> bool + Send + Sync>) -> Option<Self> {
            None
        }
    }
    pub fn game_client_rect(_pid: u32) -> Option<ClientRect> {
        None
    }
    pub fn is_foreground(_pid: u32) -> bool {
        false
    }
}

#[cfg(windows)]
mod imp {
    use super::{ClientRect, TrayEvent};
    use std::ptr::{null, null_mut};
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
    use std::sync::mpsc::Sender;
    use std::sync::{Arc, Mutex, RwLock};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        ClientToScreen, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, FillRect, GetDC, GetDeviceCaps, GetStockObject, ReleaseDC,
        RoundRect, SelectObject, SetBkMode, SetTextColor, DT_LEFT, DT_SINGLELINE, DT_VCENTER, LOGPIXELSY, NULL_PEN,
    };
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, MEASUREITEMSTRUCT};
    use windows_sys::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn copy_into(dst: &mut [u16], s: &str) {
        let n = dst.len().saturating_sub(1);
        for (i, c) in s.encode_utf16().take(n).enumerate() {
            dst[i] = c;
        }
    }

    // ---- the foreground window and the game's window ---------------------------------------------------------------------

    pub fn is_foreground(pid: u32) -> bool {
        unsafe {
            let fg = GetForegroundWindow();
            if fg.is_null() {
                return false;
            }
            let mut p = 0u32;
            GetWindowThreadProcessId(fg, &mut p);
            p == pid
        }
    }

    struct Search {
        pid: u32,
        best: HWND,
        area: i64,
    }

    unsafe extern "system" fn each_window(hwnd: HWND, lparam: LPARAM) -> i32 {
        let s = &mut *(lparam as *mut Search);
        let mut p = 0u32;
        GetWindowThreadProcessId(hwnd, &mut p);
        if p != s.pid || IsWindowVisible(hwnd) == 0 || !GetWindow(hwnd, GW_OWNER).is_null() {
            return 1;
        }
        let mut r: RECT = std::mem::zeroed();
        if GetClientRect(hwnd, &mut r) != 0 {
            let area = (r.right - r.left) as i64 * (r.bottom - r.top) as i64;
            if area > s.area {
                s.area = area;
                s.best = hwnd;
            }
        }
        1
    }

    /// Where the game's main window draws (its client area, in screen pixels); `None` when it has no window yet.
    pub fn game_client_rect(pid: u32) -> Option<ClientRect> {
        unsafe {
            let mut s = Search {
                pid,
                best: null_mut(),
                area: 0,
            };
            EnumWindows(Some(each_window), &mut s as *mut Search as LPARAM);
            if s.best.is_null() || s.area <= 0 {
                return None;
            }
            let mut r: RECT = std::mem::zeroed();
            GetClientRect(s.best, &mut r);
            let mut origin = POINT { x: 0, y: 0 };
            ClientToScreen(s.best, &mut origin);
            Some(ClientRect {
                x: origin.x,
                y: origin.y,
                w: r.right - r.left,
                h: r.bottom - r.top,
            })
        }
    }

    // ---- the keyboard hook ------------------------------------------------------------------------------------------------

    type Handler = Arc<dyn Fn(u32, bool) -> bool + Send + Sync>;
    static HANDLER: RwLock<Option<Handler>> = RwLock::new(None);

    pub struct KeyboardHook {
        thread: u32,
    }

    unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let k = &*(lparam as *const KBDLLHOOKSTRUCT);
            let down = wparam == WM_KEYDOWN as usize || wparam == WM_SYSKEYDOWN as usize;
            let up = wparam == WM_KEYUP as usize || wparam == WM_SYSKEYUP as usize;
            if down || up {
                let handler = HANDLER.read().ok().and_then(|h| h.clone());
                if let Some(h) = handler {
                    if h(k.vkCode, down) {
                        return 1; // the key goes no further
                    }
                }
            }
        }
        CallNextHookEx(null_mut(), code, wparam, lparam)
    }

    impl KeyboardHook {
        /// Installs a low-level keyboard hook on a thread of its own. The handler says whether it took the key (then the game never sees it); it runs
        /// inside the hook, so it must be quick.
        pub fn install(handler: Box<dyn Fn(u32, bool) -> bool + Send + Sync>) -> Option<Self> {
            *HANDLER.write().ok()? = Some(Arc::from(handler));
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || unsafe {
                let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), GetModuleHandleW(null()), 0);
                let _ = tx.send(if hook.is_null() { 0 } else { GetCurrentThreadId() });
                if hook.is_null() {
                    return;
                }
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                UnhookWindowsHookEx(hook);
            });
            match rx.recv() {
                Ok(t) if t != 0 => Some(Self { thread: t }),
                _ => None,
            }
        }
    }

    impl Drop for KeyboardHook {
        fn drop(&mut self) {
            unsafe {
                PostThreadMessageW(self.thread, WM_QUIT, 0, 0);
            }
            if let Ok(mut h) = HANDLER.write() {
                *h = None;
            }
        }
    }

    // ---- the notification-area icon ---------------------------------------------------------------------------------------

    const CALLBACK: u32 = WM_APP + 1;
    const CMD_OPEN: usize = 1;
    const CMD_EXIT: usize = 2;
    const CMD_LIBRARY: usize = 3;
    const CMD_STORE: usize = 4;
    const CMD_SETTINGS: usize = 5;
    const CMD_UPDATES: usize = 6;
    const CMD_REPORT: usize = 7;

    /// How a menu row is drawn: the menu is owner-drawn so it wears Cairn's colours, not the system's.
    #[derive(Clone, Copy, PartialEq)]
    enum Row {
        Item,
        Danger,
        Line,
    }

    /// The tray menu, top to bottom: (command, text, how it is drawn).
    const MENU: [(usize, &str, Row); 9] = [
        (CMD_OPEN, "Open Cairn", Row::Item),
        (CMD_LIBRARY, "Library", Row::Item),
        (CMD_STORE, "Store", Row::Item),
        (CMD_SETTINGS, "Settings", Row::Item),
        (0, "", Row::Line),
        (CMD_UPDATES, "Check for updates", Row::Item),
        (CMD_REPORT, "Report a problem", Row::Item),
        (0, "", Row::Line),
        (CMD_EXIT, "Exit", Row::Danger),
    ];

    const ODS_SELECTED: u32 = 1;
    const MENU_WIDTH: i32 = 184;

    /// A colour as the GDI wants it (blue, green, red).
    const fn rgb(r: u8, g: u8, b: u8) -> u32 {
        (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
    }
    const MENU_BG: u32 = rgb(0x1D, 0x21, 0x25);
    const MENU_HOVER: u32 = rgb(0x2B, 0x30, 0x36);
    const MENU_TEXT: u32 = rgb(0xFB, 0xFB, 0xFA);
    const MENU_DANGER: u32 = rgb(0xF8, 0x71, 0x71);
    const MENU_LINE: u32 = rgb(0x34, 0x39, 0x3F);

    unsafe fn dpi_scale() -> f32 {
        let dc = GetDC(null_mut());
        let dpi = GetDeviceCaps(dc, LOGPIXELSY as i32);
        ReleaseDC(null_mut(), dc);
        if dpi > 0 {
            dpi as f32 / 96.0
        } else {
            1.0
        }
    }

    unsafe fn measure_row(m: &mut MEASUREITEMSTRUCT) {
        let scale = dpi_scale();
        let height = match MENU.get(m.itemData).map(|r| r.2) {
            Some(Row::Line) => 9,
            _ => 30,
        };
        m.itemWidth = (MENU_WIDTH as f32 * scale) as u32;
        m.itemHeight = (height as f32 * scale) as u32;
    }

    unsafe fn draw_row(d: &DRAWITEMSTRUCT) {
        let Some(&(_, text, row)) = MENU.get(d.itemData) else { return };
        let scale = dpi_scale();
        let px = |v: f32| (v * scale).round() as i32;
        let hot = d.itemState & ODS_SELECTED != 0 && matches!(row, Row::Item | Row::Danger);
        let back = CreateSolidBrush(MENU_BG);
        FillRect(d.hDC, &d.rcItem, back);
        DeleteObject(back);
        if row == Row::Line {
            let line = CreateSolidBrush(MENU_LINE);
            let y = (d.rcItem.top + d.rcItem.bottom) / 2;
            let r = RECT {
                left: d.rcItem.left + px(10.0),
                top: y,
                right: d.rcItem.right - px(10.0),
                bottom: y + 1.max(px(1.0)),
            };
            FillRect(d.hDC, &r, line);
            DeleteObject(line);
            return;
        }
        if hot {
            // a soft rounded highlight, inset from the edges
            let hover = CreateSolidBrush(MENU_HOVER);
            let old_brush = SelectObject(d.hDC, hover);
            let old_pen = SelectObject(d.hDC, GetStockObject(NULL_PEN));
            RoundRect(
                d.hDC,
                d.rcItem.left + px(4.0),
                d.rcItem.top + px(1.0),
                d.rcItem.right - px(4.0),
                d.rcItem.bottom - px(1.0),
                px(8.0),
                px(8.0),
            );
            SelectObject(d.hDC, old_pen);
            SelectObject(d.hDC, old_brush);
            DeleteObject(hover);
        }
        let font = CreateFontW(-px(14.0), 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, wide("Segoe UI").as_ptr());
        let old = SelectObject(d.hDC, font);
        SetBkMode(d.hDC, 1);
        SetTextColor(
            d.hDC,
            match row {
                Row::Danger => MENU_DANGER,
                _ => MENU_TEXT,
            },
        );
        let mut r = RECT {
            left: d.rcItem.left + px(16.0),
            ..d.rcItem
        };
        let w = wide(text);
        DrawTextW(d.hDC, w.as_ptr(), -1, &mut r, DT_SINGLELINE | DT_VCENTER | DT_LEFT);
        SelectObject(d.hDC, old);
        DeleteObject(font);
    }

    unsafe fn tray_menu() -> HMENU {
        let menu = CreatePopupMenu();
        for (i, (cmd, _, _)) in MENU.iter().enumerate() {
            let flags = MF_OWNERDRAW | if *cmd == 0 { MF_GRAYED } else { 0 };
            AppendMenuW(menu, flags, *cmd, i as *const u16);
        }
        let back = CreateSolidBrush(MENU_BG);
        let info = MENUINFO {
            cbSize: std::mem::size_of::<MENUINFO>() as u32,
            fMask: MIM_BACKGROUND,
            dwStyle: 0,
            cyMax: 0,
            hbrBack: back,
            dwContextHelpID: 0,
            dwMenuData: 0,
        };
        SetMenuInfo(menu, &info);
        menu
    }
    static TRAY_EVENTS: Mutex<Option<Sender<TrayEvent>>> = Mutex::new(None);
    static TRAY_WINDOW: AtomicUsize = AtomicUsize::new(0);
    static TRAY_THREAD: AtomicU32 = AtomicU32::new(0);

    fn send(e: TrayEvent) {
        if let Ok(g) = TRAY_EVENTS.lock() {
            if let Some(tx) = g.as_ref() {
                let _ = tx.send(e);
            }
        }
    }

    unsafe extern "system" fn tray_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if msg == CALLBACK {
            match (lparam & 0xFFFF) as u32 {
                WM_LBUTTONUP => send(TrayEvent::Open),
                WM_RBUTTONUP => {
                    let menu = tray_menu();
                    let mut at = POINT { x: 0, y: 0 };
                    GetCursorPos(&mut at);
                    SetForegroundWindow(hwnd);
                    let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON, at.x, at.y, 0, hwnd, null());
                    DestroyMenu(menu);
                    match cmd as usize {
                        CMD_OPEN => send(TrayEvent::Open),
                        CMD_LIBRARY => send(TrayEvent::Library),
                        CMD_STORE => send(TrayEvent::Store),
                        CMD_SETTINGS => send(TrayEvent::Settings),
                        CMD_UPDATES => send(TrayEvent::CheckUpdates),
                        CMD_REPORT => send(TrayEvent::ReportProblem),
                        CMD_EXIT => send(TrayEvent::Exit),
                        _ => {}
                    }
                }
                NIN_BALLOONUSERCLICK => send(TrayEvent::Open),
                _ => {}
            }
            return 0;
        }
        match msg {
            WM_MEASUREITEM => {
                measure_row(&mut *(lparam as *mut MEASUREITEMSTRUCT));
                return 1;
            }
            WM_DRAWITEM => {
                draw_row(&*(lparam as *const DRAWITEMSTRUCT));
                return 1;
            }
            _ => {}
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    const NIN_BALLOONUSERCLICK: u32 = 0x0405;

    fn data(hwnd: HWND) -> NOTIFYICONDATAW {
        let mut d: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        d.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        d.hWnd = hwnd;
        d.uID = 1;
        d
    }

    pub struct Tray;

    impl Tray {
        /// Adds the icon from a thread of its own; `None` when there is no tray (a restricted shell): the launcher works without it.
        pub fn start(events: Sender<TrayEvent>) -> Option<Self> {
            *TRAY_EVENTS.lock().ok()? = Some(events);
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || unsafe {
                let hinst = GetModuleHandleW(null());
                let class = wide("CairnTrayWindow");
                let wc = WNDCLASSW {
                    style: 0,
                    lpfnWndProc: Some(tray_proc),
                    cbClsExtra: 0,
                    cbWndExtra: 0,
                    hInstance: hinst,
                    hIcon: null_mut(),
                    hCursor: null_mut(),
                    hbrBackground: null_mut(),
                    lpszMenuName: null(),
                    lpszClassName: class.as_ptr(),
                };
                RegisterClassW(&wc);
                let hwnd = CreateWindowExW(
                    0,
                    class.as_ptr(),
                    wide("Cairn").as_ptr(),
                    0,
                    0,
                    0,
                    0,
                    0,
                    HWND_MESSAGE,
                    null_mut(),
                    hinst,
                    null(),
                );
                if hwnd.is_null() {
                    let _ = tx.send(false);
                    return;
                }
                // the executable's own icon (resource 1) when it has one, else the system's
                let mut icon = LoadIconW(hinst, std::ptr::without_provenance::<u16>(1));
                if icon.is_null() {
                    icon = LoadIconW(null_mut(), IDI_APPLICATION);
                }
                let mut d = data(hwnd);
                d.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
                d.uCallbackMessage = CALLBACK;
                d.hIcon = icon;
                copy_into(&mut d.szTip, "Cairn Client");
                let ok = Shell_NotifyIconW(NIM_ADD, &d) != 0;
                TRAY_WINDOW.store(hwnd as usize, Ordering::SeqCst);
                TRAY_THREAD.store(GetCurrentThreadId(), Ordering::SeqCst);
                let _ = tx.send(ok);
                if !ok {
                    return;
                }
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                Shell_NotifyIconW(NIM_DELETE, &d);
                DestroyWindow(hwnd);
            });
            rx.recv().ok().filter(|ok| *ok).map(|_| Self)
        }

        /// A balloon from the icon (the caller decides when the window is not in front).
        pub fn notify(&self, title: &str, text: &str) {
            let hwnd = TRAY_WINDOW.load(Ordering::SeqCst) as HWND;
            if hwnd.is_null() {
                return;
            }
            let mut d = data(hwnd);
            d.uFlags = NIF_INFO;
            d.dwInfoFlags = NIIF_INFO;
            copy_into(&mut d.szInfoTitle, title);
            copy_into(&mut d.szInfo, text);
            unsafe {
                Shell_NotifyIconW(NIM_MODIFY, &d);
            }
        }
    }

    impl Drop for Tray {
        fn drop(&mut self) {
            let t = TRAY_THREAD.load(Ordering::SeqCst);
            if t != 0 {
                unsafe {
                    PostThreadMessageW(t, WM_QUIT, 0, 0);
                }
            }
        }
    }
}
