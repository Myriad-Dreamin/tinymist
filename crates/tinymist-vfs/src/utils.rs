#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#[allow(unused_macros)]
macro_rules! console_log {
    ($($arg:tt)*) => {
        #[cfg(feature = "web")]
        web_sys::console::info_1(&format!(
            $($arg)*
        ).into());
    }
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
#[allow(unused_macros)]
macro_rules! console_log {
    ($($arg:tt)*) => {
        eprintln!(
            $($arg)*
        );
    }
}

#[allow(unused_imports)]
pub(crate) use console_log;
