//! A tiny localization library for Rust.

use std::{
    borrow::Cow,
    collections::HashMap,
    sync::{atomic::AtomicPtr, OnceLock},
};

use rayon::{iter::ParallelIterator, str::ParallelString};

static ALL_TRANSLATIONS: OnceLock<HashMap<String, HashMap<String, String>>> = OnceLock::new();

/// Replaces the current translations with the given translations.
pub fn replace_translations(translations: HashMap<String, HashMap<String, String>>) {
    let new_translations = ALL_TRANSLATIONS.set(translations);

    if let Err(new_translations) = new_translations {
        eprintln!("cannot replacing translations: {}", new_translations.len());
    }
}

static CURRENT_TRANSLATIONS: AtomicPtr<HashMap<String, String>> =
    AtomicPtr::new(std::ptr::null_mut());

/// Replaces the current translations with the given translations.
pub fn set_locale(locale: &str) -> Option<()> {
    let translations = ALL_TRANSLATIONS.get()?;
    let translations = translations.get(locale)?;

    CURRENT_TRANSLATIONS.swap(
        translations as *const _ as *mut _,
        std::sync::atomic::Ordering::SeqCst,
    );

    Some(())
}

/// Loads a TOML string into a map of translations.
pub fn load_toml(input: &str) -> HashMap<String, HashMap<String, String>> {
    let mut translates = parse_toml(input, false);
    for vs in translates.values_mut() {
        for v in vs.values_mut() {
            let parsed = serde_json::from_str::<String>(v).unwrap();
            *v = parsed;
        }
    }

    translates
}

/// Parses a TOML string into a map of translations.
pub fn parse_toml(input: &str, tr: bool) -> HashMap<String, HashMap<String, String>> {
    let lines = input
        .par_split('\n')
        .map(|line| line.trim())
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .collect::<Vec<_>>();

    let mut translations = HashMap::new();
    let mut key = String::new();

    for line in lines {
        if line.starts_with('[') {
            key = line[1..line.len() - 1].to_string();
        } else {
            let equal_index = line.find('=').unwrap();
            let lang = line[..equal_index].trim().to_string();
            let value = line[equal_index + 1..].trim().to_string();

            if tr {
                translations
                    .entry(key.clone())
                    .or_insert_with(HashMap::new)
                    .insert(lang, value);
            } else {
                translations
                    .entry(lang)
                    .or_insert_with(HashMap::new)
                    .insert(key.clone(), value);
            }
        }
    }

    translations
}

/// Translates a string to the current language.
fn find_message(key: &'static str, message: &'static str) -> &'static str {
    let translations = CURRENT_TRANSLATIONS.load(std::sync::atomic::Ordering::Relaxed);

    if translations.is_null() {
        return message;
    }

    // Safety: The pointer is valid.
    let translations = unsafe { &*translations };

    translations.get(key).map(String::as_str).unwrap_or(message)
}

/// Translates a string to the current language.
pub fn t_without_args(key: &'static str, message: &'static str) -> Cow<'static, str> {
    Cow::Borrowed(find_message(key, message))
}

/// An argument for a translation.
pub enum Arg<'a> {
    /// A string argument.
    Str(&'a str),
    /// An integer argument.
    Int(i64),
    /// A float argument.
    Float(f64),
}

/// Translates a string to the current language.
pub fn t_with_args(key: &'static str, message: &'static str, args: &[&Arg]) -> Cow<'static, str> {
    let message = find_message(key, message);
    let mut result = String::new();
    let mut arg_index = 0;

    for c in message.chars() {
        if c == '{' {
            let mut arg_index_str = String::new();

            let chars = message.chars().skip(arg_index + 1);

            for c in chars {
                if c == '}' {
                    break;
                }

                arg_index_str.push(c);
            }

            arg_index = arg_index_str.parse::<usize>().unwrap();
            let arg = args[arg_index];

            match arg {
                Arg::Str(s) => result.push_str(s),
                Arg::Int(i) => result.push_str(&i.to_string()),
                Arg::Float(f) => result.push_str(&f.to_string()),
            }

            arg_index += arg_index_str.len() + 2;
        } else {
            result.push(c);
        }
    }

    Cow::Owned(result)
}

/// Translates a string to the current language.
#[macro_export]
macro_rules! t {
    ($key:expr, $message:expr) => {
        $crate::t_without_args($key, $message)
    };
    ($key:expr, $message:expr, $($args:expr),*) => {
        $crate::t_with_args($key, $message, &[$($args),*])
    };
}
