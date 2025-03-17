//! Tinymist's localization library.

use core::panic;
use std::{
    borrow::Cow,
    collections::HashSet,
    path::Path,
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
    let lower_locale = locale.to_lowercase();
    let locale = lower_locale.as_str();
    let translations = translations.get(locale).or_else(|| {
        // Tries s to find a language that starts with the locale and follow a hyphen.
        translations
            .iter()
            .find(|(k, _)| locale.starts_with(*k) && locale.chars().nth(k.len()) == Some('-'))
            .map(|(_, v)| v)
    })?;

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

/// Updates disk translations with new key-value pairs.
pub fn update_disk_translations(
    mut key_values: Vec<(String, String)>,
    output: &Path,
) -> anyhow::Result<()> {
    key_values.sort_by(|(key_x, _), (key_y, _)| key_x.cmp(key_y));

    // Reads and parses existing translations
    let mut translations = match std::fs::read_to_string(output) {
        Ok(existing_translations) => deserialize(&existing_translations, true)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => TranslationMapSet::default(),
        Err(e) => Err(e)?,
    };

    // Removes unused translations
    update_translations(key_values, &mut translations);

    // Writes translations
    let result = serialize_translations(translations);
    std::fs::write(output, result)?;
    Ok(())
}

/// Updates a map of translations with new key-value pairs.
pub fn update_translations(
    key_values: Vec<(String, String)>,
    translations: &mut TranslationMapSet,
) {
    let used = key_values.iter().map(|e| &e.0).collect::<HashSet<_>>();
    translations.retain(|k, _| used.contains(k));

    // Updates translations
    let en = "en".to_owned();
    for (key, value) in key_values {
        translations
            .entry(key)
            .or_default()
            .insert(en.clone(), value);
    }
}

/// Writes a map of translations to a TOML string.
pub fn serialize_translations(translations: TranslationMapSet) -> String {
    let mut result = String::new();

    result.push_str("\n# The translations are partially generated by copilot\n");

    let mut translations = translations.into_iter().collect::<Vec<_>>();
    translations.sort_by(|a, b| a.0.cmp(&b.0));

    for (key, mut data) in translations {
        result.push_str(&format!("\n[{key}]\n"));

        let en = data.remove("en").expect("en translation is missing");
        result.push_str(&format!("en = {en}\n"));

        // sort by lang
        let mut data = data.into_iter().collect::<Vec<_>>();
        data.sort_by(|a, b| a.0.cmp(&b.0));

        for (lang, value) in data {
            result.push_str(&format!("{lang} = {value}\n"));
        }
    }

    result
}

/// Tries to translate a string to the current language.
#[macro_export]
macro_rules! t {
    ($key:expr, $message:expr) => {
        $crate::t_without_args($key, $message)
    };
    ($key:expr, $message:expr $(, $arg_key:ident = $arg_value:expr)+ $(,)?) => {
        $crate::t_with_args($key, $message, &[$((stringify!($arg_key), $arg_value)),*])
    };
}

/// Returns an error with a translated message.
#[macro_export]
macro_rules! bail {
    ($key:expr, $message:expr $(, $arg_key:ident = $args:expr)* $(,)?) => {{
        let msg = $crate::t!($key, $message $(, $arg_key = $args)*);
        return Err(tinymist_std::error::prelude::_msg(concat!(file!(), ":", line!(), ":", column!()), msg.into()));
    }};
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
    Str(Cow<'a, str>),
    /// An integer argument.
    Int(i64),
    /// A float argument.
    Float(f64),
}

impl<'a> From<&'a String> for Arg<'a> {
    fn from(s: &'a String) -> Self {
        Arg::Str(Cow::Borrowed(s.as_str()))
    }
}

impl<'a> From<&'a str> for Arg<'a> {
    fn from(s: &'a str) -> Self {
        Arg::Str(Cow::Borrowed(s))
    }
}

/// Converts an object to an argument of debug message.
pub trait DebugL10n {
    /// Returns a debug string for the current language.
    fn debug_l10n(&self) -> Arg<'_>;
}

impl<T: std::fmt::Debug> DebugL10n for T {
    fn debug_l10n(&self) -> Arg<'static> {
        Arg::Str(Cow::Owned(format!("{self:?}")))
    }
}

/// Tries to translate a string to the current language.
pub fn t_with_args(
    key: &'static str,
    message: &'static str,
    args: &[(&'static str, Arg)],
) -> Cow<'static, str> {
    let message = find_message(key, message);
    let mut result = String::new();

    // for c in message.chars() {
    //     if c == '{' {
    //         let Some(bracket_index) = message[arg_index..].find('}') else {
    //             result.push(c);
    //             continue;
    //         };

    //         let arg_index_str = &message[arg_index + 1..arg_index +
    // bracket_index];

    //         match arg {
    //             Arg::Str(s) => result.push_str(s.as_ref()),
    //             Arg::Int(i) => result.push_str(&i.to_string()),
    //             Arg::Float(f) => result.push_str(&f.to_string()),
    //         }

    //         arg_index += arg_index_str.len() + 2;
    //     } else {
    //         result.push(c);
    //     }
    // }

    let message_iter = &mut message.chars();
    while let Some(c) = message_iter.next() {
        if c == '{' {
            let arg_index_str = message_iter.take_while(|c| *c != '}').collect::<String>();
            message_iter.next();

            let Some(arg) = args
                .iter()
                .find(|(k, _)| k == &arg_index_str)
                .map(|(_, v)| v)
            else {
                result.push(c);
                result.push_str(&arg_index_str);
                continue;
            };

            match arg {
                Arg::Str(s) => result.push_str(s.as_ref()),
                Arg::Int(i) => result.push_str(&i.to_string()),
                Arg::Float(f) => result.push_str(&f.to_string()),
            }
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
