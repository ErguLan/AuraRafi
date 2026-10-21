//! i18n localization system
use crate::config::Language;
use std::collections::HashMap;
use std::sync::OnceLock;

static EN_DICT: OnceLock<HashMap<String, String>> = OnceLock::new();
static ES_DICT: OnceLock<HashMap<String, String>> = OnceLock::new();

fn parse_json(raw: &str) -> HashMap<String, String> {
    let parsed: serde_json::Value = serde_json::from_str(raw).unwrap_or(serde_json::Value::Null);
    let mut map = HashMap::new();
    if let serde_json::Value::Object(obj) = parsed {
        for (k, v) in obj {
            if let serde_json::Value::String(s) = v {
                map.insert(k, s);
            }
        }
    }
    map
}

fn authoring_dictionary(base: &str, authoring: &str) -> HashMap<String, String> {
    let mut dictionary = parse_json(base);
    dictionary.extend(parse_json(authoring));
    dictionary
}

/// Translate a language key into the respective text
pub fn t(key: &str, lang: Language) -> String {
    let dict = match lang {
        Language::English => EN_DICT.get_or_init(|| {
            authoring_dictionary(
                include_str!("../locales/en.json"),
                include_str!("../locales/authoring_en.json"),
            )
        }),
        Language::Spanish => ES_DICT.get_or_init(|| {
            authoring_dictionary(
                include_str!("../locales/es.json"),
                include_str!("../locales/authoring_es.json"),
            )
        }),
    };

    dict.get(key).cloned().unwrap_or_else(|| key.to_string())
}
