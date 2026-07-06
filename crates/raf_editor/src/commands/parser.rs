use std::collections::BTreeMap;

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedInput {
    Message(String),
    Command(ParsedCommand),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCommand {
    pub raw: String,
    pub name: String,
    pub args: BTreeMap<String, String>,
    pub positional: Vec<String>,
}

impl ParsedCommand {
    pub fn arg(&self, name: &str) -> Option<&str> {
        self.args.get(name).map(String::as_str)
    }

    pub fn first_positional(&self) -> Option<&str> {
        self.positional.first().map(String::as_str)
    }

    pub fn arg_or_pos(&self, name: &str, index: usize) -> Option<&str> {
        self.arg(name)
            .or_else(|| self.positional.get(index).map(String::as_str))
    }

    pub fn bool_arg(&self, name: &str) -> bool {
        self.arg(name)
            .map(|value| matches!(value, "true" | "1" | "yes" | "on"))
            .unwrap_or(false)
    }
}

pub fn parse_console_input(input: &str) -> Result<ParsedInput, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(ParsedInput::Message(String::new()));
    }

    if !trimmed.starts_with('/') {
        return Ok(ParsedInput::Message(trimmed.to_string()));
    }

    let without_slash = trimmed.trim_start_matches('/');
    if without_slash.trim().is_empty() {
        return Ok(ParsedInput::Command(ParsedCommand {
            raw: trimmed.to_string(),
            name: "help".to_string(),
            args: BTreeMap::new(),
            positional: Vec::new(),
        }));
    }

    let (name, rest) = split_name_and_rest(without_slash);
    let mut args = BTreeMap::new();
    let mut positional = Vec::new();

    let rest = rest.trim();
    if rest.starts_with('{') {
        let value: Value =
            serde_json::from_str(rest).map_err(|error| format!("Invalid JSON payload: {error}"))?;
        if let Some(object) = value.as_object() {
            for (key, value) in object {
                args.insert(key.clone(), json_value_to_arg(value));
            }
        } else {
            return Err("JSON payload must be an object.".to_string());
        }
    } else {
        for token in tokenize(rest)? {
            if token.is_empty() {
                continue;
            }

            if let Some(flag) = token.strip_prefix("--") {
                if let Some((key, value)) = flag.split_once('=') {
                    args.insert(key.to_string(), value.to_string());
                } else {
                    args.insert(flag.to_string(), "true".to_string());
                }
            } else if let Some((key, value)) = token.split_once('=') {
                args.insert(key.to_string(), value.to_string());
            } else {
                positional.push(token);
            }
        }
    }

    Ok(ParsedInput::Command(ParsedCommand {
        raw: trimmed.to_string(),
        name: name.to_ascii_lowercase(),
        args,
        positional,
    }))
}

fn split_name_and_rest(input: &str) -> (&str, &str) {
    match input.find(char::is_whitespace) {
        Some(index) => (&input[..index], &input[index..]),
        None => (input, ""),
    }
}

fn tokenize(input: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in input.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }

        if ch == '\\' {
            escaped = true;
            continue;
        }

        match quote {
            Some(active) if ch == active => quote = None,
            Some(_) => current.push(ch),
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            None => current.push(ch),
        }
    }

    if quote.is_some() {
        return Err("Unclosed quote in command input.".to_string());
    }

    if escaped {
        current.push('\\');
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    Ok(tokens)
}

fn json_value_to_arg(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Null => String::new(),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_only_maps_to_help() {
        let parsed = parse_console_input("/").unwrap();
        match parsed {
            ParsedInput::Command(command) => assert_eq!(command.name, "help"),
            ParsedInput::Message(_) => panic!("expected command"),
        }
    }

    #[test]
    fn parses_key_values_and_quotes() {
        let parsed =
            parse_console_input("/game.add primitive=cube name=\"Player Start\" x=2").unwrap();
        match parsed {
            ParsedInput::Command(command) => {
                assert_eq!(command.name, "game.add");
                assert_eq!(command.arg("primitive"), Some("cube"));
                assert_eq!(command.arg("name"), Some("Player Start"));
                assert_eq!(command.arg("x"), Some("2"));
            }
            ParsedInput::Message(_) => panic!("expected command"),
        }
    }

    #[test]
    fn parses_json_payload() {
        let parsed = parse_console_input("/game.add {\"primitive\":\"sphere\",\"x\":1}").unwrap();
        match parsed {
            ParsedInput::Command(command) => {
                assert_eq!(command.arg("primitive"), Some("sphere"));
                assert_eq!(command.arg("x"), Some("1"));
            }
            ParsedInput::Message(_) => panic!("expected command"),
        }
    }
}
