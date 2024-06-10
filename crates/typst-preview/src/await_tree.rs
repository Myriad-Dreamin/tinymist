use std::borrow::Cow;

use await_tree::{Config, Registry};
use once_cell::sync::Lazy;
use tokio::sync::Mutex;
pub static REGISTRY: Lazy<Mutex<Registry<Cow<'static, str>>>> =
    Lazy::new(|| Mutex::new(Registry::new(Config::default())));

pub async fn get_await_tree_async() -> String {
    let trace = REGISTRY.lock().await;
    get_await_tree_impl(&trace)
}

pub fn get_await_tree_blocking() -> String {
    let trace = REGISTRY.blocking_lock();
    get_await_tree_impl(&trace)
}

fn get_await_tree_impl(trace: &Registry<Cow<'static, str>>) -> String {
    let mut res = trace.iter().collect::<Vec<_>>();
    res.sort_by_key(|&(k, _)| k);
    res.into_iter()
        .map(|(_, tree)| tree.to_string())
        .collect::<String>()
}
