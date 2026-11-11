//! Bounded on-demand catalogs, scoped to endpoint and credential identity.
use crate::provider::AiProvider;
use std::sync::{Mutex, OnceLock};
const MAX_BYTES: u64 = 2 * 1024 * 1024;
const MAX_MODELS: usize = 4096;

#[derive(Clone, Debug, Default)]
pub struct Catalog {
    pub models: Vec<String>,
    pub loading: bool,
    pub error: Option<String>,
    /// Even an empty successful list supersedes offline suggestions.
    pub fetched: bool,
    requested: bool,
    identity: String,
    generation: u64,
}
fn catalogs() -> &'static Mutex<Vec<(AiProvider, Catalog)>> {
    static CATALOGS: OnceLock<Mutex<Vec<(AiProvider, Catalog)>>> = OnceLock::new();
    CATALOGS.get_or_init(|| Mutex::new(Vec::new()))
}
pub fn snapshot(provider: AiProvider) -> Catalog {
    catalogs()
        .try_lock()
        .ok()
        .and_then(|all| {
            all.iter()
                .find(|(p, _)| *p == provider)
                .map(|(_, c)| c.clone())
        })
        .unwrap_or_default()
}
pub fn key_required(provider: AiProvider) -> bool {
    provider != AiProvider::OpenRouter
}
/// Never project a previous credential's models into a different draft.
pub fn snapshot_for(provider: AiProvider, base_url: &str, api_key: &str) -> Catalog {
    let catalog = snapshot(provider);
    if catalog.identity == request_identity(&endpoint(provider, base_url), api_key) {
        catalog
    } else {
        Catalog {
            loading: catalog.loading,
            ..Catalog::default()
        }
    }
}
fn endpoint(provider: AiProvider, base_url: &str) -> String {
    match provider {
        AiProvider::OpenCode => return "https://opencode.ai/inference/v1/models".into(),
        AiProvider::OpenCodeGo => return "https://opencode.ai/inference/go/v1/models".into(),
        _ => {}
    }
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        String::new()
    } else {
        format!("{base}/models")
    }
}
/// Native text/tool suggestion policy, not a claim about general API access.
/// Explicit user ids are retained separately; account catalogs use visibility.
pub fn suggested_model(provider: AiProvider, id: &str) -> bool {
    if provider != AiProvider::OpenAI {
        return true;
    }
    let id = id.to_ascii_lowercase();
    ![
        "gpt-4",
        "chatgpt-4",
        "gpt-3",
        "gpt-5-mini",
        "gpt-5-nano",
        "o1",
        "o3",
        "o4",
        "codex-mini",
    ]
    .iter()
    .any(|prefix| id.starts_with(prefix))
        && ![
            "embedding",
            "tts",
            "whisper",
            "moderation",
            "dall-e",
            "image",
            "realtime",
            "audio",
            "transcrib",
            "search",
            "deep-research",
        ]
        .iter()
        .any(|kind| id.contains(kind))
}
fn request_identity(endpoint: &str, key: &str) -> String {
    use base64::Engine;
    let digest = ring::digest::digest(&ring::digest::SHA256, key.as_bytes());
    format!(
        "{endpoint}:{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest.as_ref())
    )
}
fn prepare(
    c: &mut Catalog,
    identity: &str,
    needs_key: bool,
    key: &str,
    force: bool,
) -> Option<u64> {
    if c.identity != identity {
        // A draft changing while a request is pending cannot create unbounded
        // workers. The next UI poll starts the replacement after completion.
        if c.loading {
            return None;
        }
        let generation = c.generation.wrapping_add(1);
        *c = Catalog {
            identity: identity.into(),
            generation,
            ..Catalog::default()
        };
    }
    if needs_key && key.trim().is_empty() {
        c.error = Some("catalog_needs_key".into());
        return None;
    }
    if c.loading || (c.requested && !force) {
        return None;
    }
    c.requested = true;
    c.loading = true;
    c.error = None;
    c.generation = c.generation.wrapping_add(1);
    Some(c.generation)
}
fn complete(c: &mut Catalog, identity: &str, generation: u64, result: Result<Vec<String>, String>) {
    if c.identity != identity || c.generation != generation {
        return;
    }
    c.loading = false;
    match result {
        Ok(models) => {
            c.models = models;
            c.fetched = true;
            c.error = None;
        }
        Err(error) => c.error = Some(error),
    }
}
pub fn refresh(provider: AiProvider, base_url: &str, api_key: &str) {
    request(provider, base_url, api_key, true);
}
pub fn ensure_requested(provider: AiProvider, base_url: &str, api_key: &str) {
    request(provider, base_url, api_key, false);
}
fn request(provider: AiProvider, base_url: &str, api_key: &str, force: bool) {
    let endpoint = endpoint(provider, base_url);
    let identity = request_identity(&endpoint, api_key);
    let generation = {
        let Ok(mut all) = catalogs().try_lock() else {
            return;
        };
        let index = all
            .iter()
            .position(|(p, _)| *p == provider)
            .unwrap_or_else(|| {
                all.push((provider, Catalog::default()));
                all.len() - 1
            });
        let c = &mut all[index].1;
        let Some(generation) = prepare(c, &identity, key_required(provider), api_key, force) else {
            return;
        };
        if endpoint.is_empty() {
            complete(c, &identity, generation, Err("catalog_invalid".into()));
            return;
        }
        generation
    };
    let api_key = api_key.to_owned();
    let worker_identity = identity.clone();
    let spawn = std::thread::Builder::new()
        .name("agent-model-catalog".into())
        .spawn(move || {
            let result = fetch(provider, &endpoint, &api_key);
            if let Ok(mut all) = catalogs().lock() {
                if let Some((_, c)) = all.iter_mut().find(|(p, _)| *p == provider) {
                    complete(c, &worker_identity, generation, result);
                }
            }
        });
    if spawn.is_err() {
        if let Ok(mut all) = catalogs().try_lock() {
            if let Some((_, c)) = all.iter_mut().find(|(p, _)| *p == provider) {
                complete(c, &identity, generation, Err("catalog_unavailable".into()));
            }
        }
    }
}
fn fetch(provider: AiProvider, endpoint: &str, api_key: &str) -> Result<Vec<String>, String> {
    use std::io::Read;
    let mut request = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(20))
        .redirects(0)
        .build()
        .get(endpoint)
        .set("User-Agent", "AuraRafi-Agent/1.0");
    if !api_key.trim().is_empty() {
        request = request.set("Authorization", &format!("Bearer {api_key}"));
    }
    let response = request
        .call()
        .map_err(|_| "catalog_unavailable".to_string())?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "catalog_unavailable")?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("catalog_invalid".into());
    }
    let value = serde_json::from_slice(&bytes).map_err(|_| "catalog_invalid")?;
    parse_models(provider, &value)
}
fn parse_models(provider: AiProvider, value: &serde_json::Value) -> Result<Vec<String>, String> {
    let entries = value
        .get("data")
        .or_else(|| value.get("models"))
        .and_then(serde_json::Value::as_array)
        .ok_or("catalog_invalid")?;
    if entries.len() > MAX_MODELS {
        return Err("catalog_invalid".into());
    }
    let mut models = Vec::new();
    for entry in entries {
        let Some(id) = entry
            .get("id")
            .or_else(|| entry.get("slug"))
            .and_then(serde_json::Value::as_str)
        else {
            continue;
        };
        if id.is_empty()
            || id.len() > 160
            || id.chars().any(char::is_control)
            || !suggested_model(provider, id)
        {
            continue;
        }
        if provider == AiProvider::OpenCode
            && (id.ends_with("-free") || matches!(id, "big-pickle" | "test"))
        {
            continue;
        }
        if !models.iter().any(|known| known == id) {
            models.push(id.to_string());
        }
    }
    Ok(models)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_lifecycle_is_bounded_and_credential_scoped() {
        let mut c = Catalog::default();
        assert!(prepare(&mut c, "first", true, "", false).is_none());
        assert_eq!(c.error.as_deref(), Some("catalog_needs_key"));
        let first = prepare(&mut c, "first-key", true, "key", false).unwrap();
        assert!(prepare(&mut c, "first-key", true, "key", false).is_none());
        assert!(prepare(&mut c, "changed-key", true, "new-key", false).is_none());
        complete(&mut c, "first-key", first, Ok(vec!["previous".into()]));
        let next = prepare(&mut c, "changed-key", true, "new-key", false).unwrap();
        complete(&mut c, "first-key", first, Ok(vec!["stale".into()]));
        assert!(c.models.is_empty() && c.loading);
        complete(&mut c, "changed-key", next, Ok(Vec::new()));
        assert!(c.fetched && !c.loading && c.error.is_none());
        assert!(prepare(&mut c, "changed-key", true, "new-key", false).is_none());
        assert!(prepare(&mut c, "changed-key", true, "new-key", true).is_some());
        assert_ne!(
            request_identity("one", "key"),
            request_identity("two", "key")
        );
    }
    #[test]
    fn catalog_refresh_registers_missing_state_without_network() {
        refresh(AiProvider::OpenAI, "", "");
        assert_eq!(
            snapshot(AiProvider::OpenAI).error.as_deref(),
            Some("catalog_needs_key")
        );
    }
    #[test]
    fn catalog_preserves_server_order_without_legacy_or_non_agent_suggestions() {
        let parsed = parse_models(
            AiProvider::OpenAI,
            &serde_json::json!({"data":[
                {"id":"gpt-6.1-sol"},{"id":"gpt-4o"},{"id":"gpt-5-mini"},
                {"id":"gpt-image-2"},{"id":"gpt-6-luna"},{"id":"gpt-6.1-sol"},{"id":"future-model"}
            ]}),
        )
        .unwrap();
        assert_eq!(parsed, vec!["gpt-6.1-sol", "gpt-6-luna", "future-model"]);
        assert_eq!(
            endpoint(AiProvider::OpenAI, "https://api.openai.com/v1/"),
            "https://api.openai.com/v1/models"
        );
        assert_eq!(endpoint(AiProvider::OpenRouter, ""), "");
    }
}
