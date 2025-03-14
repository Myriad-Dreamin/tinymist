//! Tinymist's localization library.

use core::panic;
use std::{
    borrow::Cow,
    sync::{OnceLock, RwLock},
};

use rayon::{
    iter::{IntoParallelRefMutIterator, ParallelIterator},
    str::ParallelString,
};
use rustc_hash::FxHashMap;

/// A map of translations.
pub type TranslationMap = FxHashMap<String, String>;
/// A set of translation maps.
pub type TranslationMapSet = FxHashMap<String, TranslationMap>;

static ALL_TRANSLATIONS: OnceLock<TranslationMapSet> = OnceLock::new();
static LOCALE_TRANSLATIONS: RwLock<Option<&'static TranslationMap>> = RwLock::new(Option::None);

/// Sets the current translations. It can only be called once.
pub fn set_translations(translations: TranslationMapSet) {
    let new_translations = ALL_TRANSLATIONS.set(translations);

    if let Err(new_translations) = new_translations {
        eprintln!("cannot set translations: len = {}", new_translations.len());
    }
}

/// Sets the current locale.
pub fn set_locale(locale: &str) -> Option<()> {
    let translations = ALL_TRANSLATIONS.get()?;
    let translations = translations.get(locale)?;

    *LOCALE_TRANSLATIONS.write().unwrap() = Some(translations);

    Some(())
}

/// Loads a TOML string into a map of translations.
pub fn load_translations(input: &str) -> anyhow::Result<TranslationMapSet> {
    let mut translations = deserialize(input, false)?;
    translations.par_iter_mut().for_each(|(_, v)| {
        v.par_iter_mut().for_each(|(_, v)| {
            if !v.starts_with('"') {
                return;
            }

            *v = serde_json::from_str::<String>(v)
                .unwrap_or_else(|e| panic!("cannot parse translation message: {e}, message: {v}"));
        });
    });

    Ok(translations)
}

/// Tries to translate a string to the current language.
#[macro_export]
macro_rules! t {
    ($key:expr, $message:expr) => {
        $crate::t_without_args($key, $message)
    };
    ($key:expr, $message:expr, $($args:expr),*) => {
        $crate::t_with_args($key, $message, &[$($args),*])
    };
}

/// Tries to get a translation for a key.
fn find_message(key: &'static str, message: &'static str) -> &'static str {
    let Some(translations) = LOCALE_TRANSLATIONS.read().unwrap().as_ref().copied() else {
        return message;
    };

    translations.get(key).map(String::as_str).unwrap_or(message)
}

/// Tries to translate a string to the current language.
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

/// Tries to translate a string to the current language.
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

/// Deserializes a TOML string into a map of translations.
pub fn deserialize(input: &str, key_first: bool) -> anyhow::Result<TranslationMapSet> {
    let lines = input
        .par_split('\n')
        .map(|line| line.trim())
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .collect::<Vec<_>>();

    let mut translations = FxHashMap::default();
    let mut key = String::new();

    for line in lines {
        if line.starts_with('[') {
            key = line[1..line.len() - 1].to_string();
        } else {
            let equal_index = line.find('=').map_or_else(
                || {
                    Err(anyhow::anyhow!(
                        "cannot find equal sign in translation line: {line}"
                    ))
                },
                Ok,
            )?;
            let lang = line[..equal_index].trim().to_string();
            let value = line[equal_index + 1..].trim().to_string();

            if key_first {
                translations
                    .entry(key.clone())
                    .or_insert_with(FxHashMap::default)
                    .insert(lang, value);
            } else {
                translations
                    .entry(lang)
                    .or_insert_with(FxHashMap::default)
                    .insert(key.clone(), value);
            }
        }
    }

    Ok(translations)
}
