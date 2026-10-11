//! Conservative static reference extraction. Never compiles or executes project code.
//! Dynamic expressions, generated names and cross-function data flow are deliberately unknown.
use crate::references;
use raf_core::scene::{SceneGraph, SceneNodeId};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SceneReference {
    Owner,
    Entity(String),
    Root(String),
    Parameter(String),
    Child(Box<SceneReference>, String),
    Parent(Box<SceneReference>),
}
impl SceneReference {
    pub fn resolve_with(
        &self,
        lookup: &references::SceneLookup<'_>,
        owner: SceneNodeId,
    ) -> Option<SceneNodeId> {
        self.resolve_with_source(lookup, owner, None)
    }
    pub fn resolve_with_source(
        &self,
        lookup: &references::SceneLookup<'_>,
        owner: SceneNodeId,
        source: Option<&str>,
    ) -> Option<SceneNodeId> {
        match self {
            Self::Owner => lookup.valid(owner).then_some(owner),
            Self::Entity(name) => lookup.entity(name),
            Self::Root(name) => lookup.root(name),
            Self::Parameter(name) => lookup.parameter(owner, source, name),
            Self::Child(parent, name) => {
                lookup.child(parent.resolve_with_source(lookup, owner, source)?, name)
            }
            Self::Parent(child) => lookup.parent(child.resolve_with_source(lookup, owner, source)?),
        }
    }
    pub fn resolve(&self, scene: &SceneGraph, owner: SceneNodeId) -> Option<SceneNodeId> {
        match self {
            Self::Owner => scene.is_valid_node(owner).then_some(owner),
            Self::Entity(name) => references::entity(scene, name),
            Self::Root(name) => references::root_child(scene, name),
            Self::Parameter(name) => {
                references::SceneLookup::new(scene).parameter(owner, None, name)
            }
            Self::Child(parent, name) => {
                references::child(scene, parent.resolve(scene, owner)?, name)
            }
            Self::Parent(child) => scene
                .get(child.resolve(scene, owner)?)?
                .parent
                .filter(|id| scene.is_valid_node(*id)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Ident(String),
    Text(String),
    Symbol(char),
    Unknown,
}

fn tokens(source: &str) -> Vec<Token> {
    if source.len() > 256 * 1024 {
        return Vec::new();
    }
    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() && out.len() < 65_536 {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            i += 2;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            let mut depth = 1;
            i += 2;
            while i + 1 < chars.len() && depth > 0 {
                if chars[i] == '/' && chars[i + 1] == '*' {
                    depth += 1;
                    i += 2;
                } else if chars[i] == '*' && chars[i + 1] == '/' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if depth > 0 {
                break;
            }
            continue;
        }
        if matches!(c, '"' | '\'' | '`') {
            let quote = c;
            i += 1;
            let mut text = String::new();
            let mut known = quote == '"';
            let mut closed = false;
            while i < chars.len() {
                let c = chars[i];
                i += 1;
                if c == quote {
                    closed = true;
                    break;
                }
                if c == '\\' {
                    let Some(&escape) = chars.get(i) else {
                        break;
                    };
                    i += 1;
                    match escape {
                        '"' => text.push('"'),
                        '\\' => text.push('\\'),
                        'n' => text.push('\n'),
                        'r' => text.push('\r'),
                        't' => text.push('\t'),
                        _ => known = false,
                    }
                } else {
                    text.push(c);
                }
            }
            if !closed {
                break;
            }
            out.push(if known {
                Token::Text(text)
            } else {
                Token::Unknown
            });
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push(Token::Ident(chars[start..i].iter().collect()));
            continue;
        }
        out.push(Token::Symbol(c));
        i += 1;
    }
    out
}

fn symbol(tokens: &[Token], index: usize, c: char) -> bool {
    tokens.get(index) == Some(&Token::Symbol(c))
}
fn ident(tokens: &[Token], index: usize, name: &str) -> bool {
    tokens.get(index) == Some(&Token::Ident(name.into()))
}
fn text(tokens: &[Token], index: usize) -> Option<String> {
    match tokens.get(index)? {
        Token::Text(value) => Some(value.clone()),
        _ => None,
    }
}
fn literal_call(tokens: &[Token], i: usize) -> Option<(String, usize)> {
    if !symbol(tokens, i, '(') || !symbol(tokens, i + 2, ')') {
        return None;
    }
    Some((text(tokens, i + 1)?, i + 3))
}

fn expression(
    tokens: &[Token],
    i: usize,
    aliases: &HashMap<String, SceneReference>,
    depth: usize,
) -> Option<(SceneReference, usize)> {
    if depth > 16 {
        return None;
    }
    let Token::Ident(name) = tokens.get(i)? else {
        return None;
    };
    let (mut reference, mut end) = match name.as_str() {
        "script" if symbol(tokens, i + 1, '.') && ident(tokens, i + 2, "parent") => {
            (SceneReference::Owner, i + 3)
        }
        "owner" | "self_node" if symbol(tokens, i + 1, '(') && symbol(tokens, i + 2, ')') => {
            (SceneReference::Owner, i + 3)
        }
        "game" if symbol(tokens, i + 1, '[') && symbol(tokens, i + 3, ']') => {
            (SceneReference::Root(text(tokens, i + 2)?), i + 4)
        }
        "game" if symbol(tokens, i + 1, '.') && ident(tokens, i + 2, "child") => {
            let (name, end) = literal_call(tokens, i + 3)?;
            (SceneReference::Root(name), end)
        }
        "entity" | "get_node" | "get_node_by_uuid" | "get_node_by_path" => {
            let (name, end) = literal_call(tokens, i + 1)?;
            (SceneReference::Entity(name), end)
        }
        "entity_parameter" => {
            let (name, end) = literal_call(tokens, i + 1)?;
            (SceneReference::Parameter(name), end)
        }
        "find_child" if symbol(tokens, i + 1, '(') => {
            let (parent, end) = expression(tokens, i + 2, aliases, depth + 1)?;
            if !symbol(tokens, end, ',') || !symbol(tokens, end + 2, ')') {
                return None;
            }
            (
                SceneReference::Child(Box::new(parent), text(tokens, end + 1)?),
                end + 3,
            )
        }
        "get_parent" if symbol(tokens, i + 1, '(') => {
            let (child, end) = expression(tokens, i + 2, aliases, depth + 1)?;
            if !symbol(tokens, end, ')') {
                return None;
            }
            (SceneReference::Parent(Box::new(child)), end + 1)
        }
        _ => (aliases.get(name)?.clone(), i + 1),
    };
    loop {
        if symbol(tokens, end, '.') && ident(tokens, end + 1, "parent") {
            reference = SceneReference::Parent(Box::new(reference));
            end += 2;
        } else if symbol(tokens, end, '.') && ident(tokens, end + 1, "child") {
            let (name, next) = literal_call(tokens, end + 2)?;
            reference = SceneReference::Child(Box::new(reference), name);
            end = next;
        } else if symbol(tokens, end, '[') && symbol(tokens, end + 2, ']') {
            reference = SceneReference::Child(Box::new(reference), text(tokens, end + 1)?);
            end += 3;
        } else {
            break;
        }
        if end - i > 128 {
            return None;
        }
    }
    Some((reference, end))
}

/// Returns potential references, not proof of execution or of a mutation.
pub fn analyze_rhai(source: &str) -> Vec<SceneReference> {
    let tokens = tokens(source);
    // Track only uniquely declared, never reassigned aliases. Shadowed/reassigned
    // identifiers require control-flow analysis and must not invent associations.
    let mut writes: HashMap<String, usize> = HashMap::new();
    for i in 0..tokens.len() {
        if let Some(Token::Ident(name)) = tokens.get(i) {
            if symbol(&tokens, i + 1, '=') && !symbol(&tokens, i + 2, '=') {
                *writes.entry(name.clone()).or_default() += 1;
            }
        }
    }
    let mut aliases = HashMap::new();
    let mut found = Vec::new();
    for i in 0..tokens.len() {
        if ident(&tokens, i, "let") || ident(&tokens, i, "const") {
            if let Some(Token::Ident(name)) = tokens.get(i + 1) {
                if symbol(&tokens, i + 2, '=') && writes.get(name) == Some(&1) {
                    if let Some((value, end)) = expression(&tokens, i + 3, &aliases, 0) {
                        if symbol(&tokens, end, ';') {
                            aliases.insert(name.clone(), value);
                        }
                    }
                }
            }
        }
        if i > 0 && symbol(&tokens, i - 1, '.') {
            continue;
        }
        if let Some((reference, _)) = expression(&tokens, i, &aliases, 0) {
            if !found.contains(&reference) {
                found.push(reference);
            }
            if found.len() >= 256 {
                break;
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literals_and_safe_aliases_ignore_comments_strings_and_dynamic_names() {
        let refs = analyze_rhai(
            r#"
            // entity("Fake")
            let text = "entity(\"Fake\")";
            let mine = script.parent;
            let target = mine.child("Lamp");
            target.move_by(1.0, 0.0, 0.0);
            game["Power"].child("Switch");
            entity(dynamic_name);
        "#,
        );
        assert!(refs.contains(&SceneReference::Child(
            Box::new(SceneReference::Owner),
            "Lamp".into()
        )));
        assert!(refs.contains(&SceneReference::Child(
            Box::new(SceneReference::Root("Power".into())),
            "Switch".into()
        )));
        assert!(!refs.iter().any(|r| matches!(r, SceneReference::Entity(_))));
    }
    #[test]
    fn ambiguous_names_and_deleted_nodes_never_bind_by_accident() {
        let mut scene = SceneGraph::new();
        let owner = scene.add_root("Owner");
        scene.add_root("Duplicate");
        scene.add_root("Duplicate");
        assert_eq!(
            SceneReference::Entity("Duplicate".into()).resolve(&scene, owner),
            None
        );
        let marker = scene.add_child(owner, "Marker");
        let lookup = references::SceneLookup::new(&scene);
        for reference in [
            "Duplicate",
            "Owner/Marker",
            "/Owner/Marker",
            "missing",
            "Owner/../Marker",
        ] {
            assert_eq!(
                lookup.entity(reference),
                references::entity(&scene, reference)
            );
        }
        assert_eq!(
            lookup.entity(&scene.get(marker).unwrap().uuid.to_string()),
            Some(marker)
        );
        scene.remove_node(owner);
        assert_eq!(SceneReference::Owner.resolve(&scene, owner), None);
    }
}
