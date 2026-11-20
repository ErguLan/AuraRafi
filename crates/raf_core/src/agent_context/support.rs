//! Shared argument, pagination and budget helpers for bounded observations.

use super::{DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE};
use serde_json::{json, Value};

pub(super) fn page(args: &Value) -> (usize, usize) {
    let offset = args
        .get("cursor")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(usize::MAX as u64) as usize;
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_PAGE_SIZE as u64)
        .clamp(1, MAX_PAGE_SIZE as u64) as usize;
    (offset, limit)
}

pub(super) fn paged_data(
    items: Vec<Value>,
    total: usize,
    offset: usize,
    limit: usize,
    mut extra: Value,
) -> Value {
    let returned = items.len();
    let next_cursor = (offset + returned < total).then_some(offset + returned);
    let object = extra.as_object_mut().unwrap_or_else(|| unreachable!());
    object.insert("items".to_string(), Value::Array(items));
    object.insert("total".to_string(), json!(total));
    object.insert("returned".to_string(), json!(returned));
    object.insert("cursor".to_string(), json!(offset));
    object.insert("next_cursor".to_string(), json!(next_cursor));
    object.insert("has_more".to_string(), json!(next_cursor.is_some()));
    object.insert("page_size".to_string(), json!(limit));
    extra
}

pub(super) fn string_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

pub(super) fn string_array_arg(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn bool_arg(args: &Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(default)
}
