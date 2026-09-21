#[cfg(target_os = "windows")]
pub mod platform {
    use std::ffi::c_void;

    #[link(name = "dwmapi")]
    #[link(name = "user32")]
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThreadId() -> u32;
        fn EnumThreadWindows(
            dwThreadId: u32,
            lpfn: unsafe extern "system" fn(hwnd: *mut c_void, l_param: isize) -> i32,
            l_param: isize,
        ) -> i32;
        fn DwmSetWindowAttribute(
            hwnd: *mut c_void,
            dw_attribute: u32,
            pv_attribute: *const c_void,
            cb_attribute: u32,
        ) -> i32;
    }

    unsafe extern "system" fn enum_window_callback(hwnd: *mut c_void, _lparam: isize) -> i32 {
        // 1. DWMWA_USE_IMMERSIVE_DARK_MODE (attribute 20)
        let dark_mode: i32 = 1;
        DwmSetWindowAttribute(hwnd, 20, &dark_mode as *const _ as *const c_void, 4);

        // 2. DWMWA_CAPTION_COLOR (attribute 35) -> COLORREF 0x002E2723 (BGR for #23272E)
        let caption_color: u32 = 0x002E2723;
        DwmSetWindowAttribute(hwnd, 35, &caption_color as *const _ as *const c_void, 4);

        // 3. DWMWA_TEXT_COLOR (attribute 36) -> COLORREF 0x00BFB2AB (BGR for #ABB2BF)
        let text_color: u32 = 0x00BFB2AB;
        DwmSetWindowAttribute(hwnd, 36, &text_color as *const _ as *const c_void, 4);

        // 4. DWMWA_BORDER_COLOR (attribute 34) -> COLORREF 0x001F1A18 (BGR for #181A1F)
        let border_color: u32 = 0x001F1A18;
        DwmSetWindowAttribute(hwnd, 34, &border_color as *const _ as *const c_void, 4);

        1 // Continue enumeration
    }

    pub fn apply_dark_title_bar() {
        unsafe {
            let thread_id = GetCurrentThreadId();
            EnumThreadWindows(thread_id, enum_window_callback, 0);
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub mod platform {
    pub fn apply_dark_title_bar() {}
}

pub fn apply_dark_title_bar() {
    platform::apply_dark_title_bar();
}
