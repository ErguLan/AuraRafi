//! User-initiated account authorization and protected local credential storage.
//!
//! Inference uses the public Responses endpoint. Credentials never enter project
//! history, engine settings, command arguments, or logs.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ring::{digest, rand::{SecureRandom, SystemRandom}, signature};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

const ISSUER: &str = "https://auth.openai.com";
const AUTHORIZE: &str = "https://auth.openai.com/api/accounts/authorize";
const TOKEN: &str = "https://auth.openai.com/api/accounts/oauth/token";
const JWKS: &str = "https://auth.openai.com/.well-known/jwks.json";
const REVOKE: &str = "https://auth.openai.com/api/accounts/oauth/revoke";
const RESOURCE: &str = "https://api.openai.com/v1";
const SCOPE: &str = "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
const REQUIRED_SCOPE: &str = "chatgpt.tokens.use.direct";
const CALLBACK_PATH: &str = "/auth/callback";
const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
const LOGIN_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountView {
    pub label: String,
    pub connected: bool,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelView {
    pub slug: String,
    pub label: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthView {
    pub busy: bool,
    pub accounts: Vec<AccountView>,
    pub models: Vec<ModelView>,
    pub message: Option<String>,
    pub needs_welcome: bool,
    pub generation: u64,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct Store {
    host_id: String,
    profiles: Vec<Profile>,
    active: Option<usize>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Profile {
    subject: String,
    email: String,
    client_id: String,
    id_token: String,
    access_token: String,
    refresh_token: String,
    scopes: Vec<String>,
    expires_at: i64,
    #[serde(default)]
    welcome_seen: bool,
}

struct State {
    store: Store,
    busy: bool,
    models: Vec<ModelView>,
    message: Option<String>,
    storage_problem: Option<String>,
    generation: u64,
    models_requested: bool,
}

fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| {
        let (store, message) = match load_store() {
            Ok(store) => (store, None),
            Err(error) => (Store::default(), Some(error)),
        };
        Mutex::new(State { store, busy: false, models: Vec::new(), storage_problem: message.clone(), message, generation: 0, models_requested: false })
    })
}

/// Never waits for an ongoing token refresh on the presentation thread.
pub fn snapshot() -> Option<AuthView> {
    let state = state().try_lock().ok()?;
    Some(AuthView {
        busy: state.busy,
        accounts: state.store.profiles.iter().enumerate().map(|(index, profile)| AccountView {
            label: if profile.email.is_empty() { profile.subject.clone() } else { profile.email.clone() },
            connected: !profile.refresh_token.is_empty() && profile.scopes.iter().any(|s| s == REQUIRED_SCOPE),
            active: state.store.active == Some(index),
        }).collect(),
        models: state.models.clone(),
        message: state.message.clone(),
        needs_welcome: state.store.active.and_then(|index| state.store.profiles.get(index))
            .is_some_and(|profile| !profile.welcome_seen && !profile.refresh_token.is_empty()),
        generation: state.generation,
    })
}

pub fn active_connected() -> bool {
    snapshot().is_some_and(|view| view.accounts.iter().any(|a| a.active && a.connected))
}

pub fn active_registration() -> Option<String> {
    let state = state().try_lock().ok()?;
    state.store.active.and_then(|index| state.store.profiles.get(index))
        .filter(|profile| !profile.refresh_token.is_empty())
        .map(|profile| profile.client_id.clone())
}

/// `profile` is None for a new registration; saved registrations reuse their client ID.
pub fn begin_sign_in(profile: Option<usize>, language: raf_core::Language) -> Result<(), String> {
    let (host, previous_client, previous_subject) = {
        let mut state = state().lock().map_err(|_| "Account state unavailable.")?;
        if state.busy { return Err("An account action is already running.".into()); }
        if let Some(error) = &state.storage_problem { return Err(error.clone()); }
        if state.store.host_id.is_empty() {
            let _lock = lock_store()?;
            let mut latest = load_store()?;
            if latest.host_id.is_empty() { latest.host_id = format!("urn:uuid:{}", Uuid::new_v4()); }
            save_store(&latest)?;
            state.store = latest;
        }
        let previous = match profile {
            Some(index) => Some(state.store.profiles.get(index).ok_or("Account no longer exists.")?),
            None => None,
        };
        let result = (state.store.host_id.clone(), previous.map(|p| p.client_id.clone()), previous.map(|p| p.subject.clone()));
        state.busy = true;
        state.message = None;
        result
    };
    std::thread::Builder::new().name("agent-account-sign-in".into()).spawn(move || {
        let result = sign_in(&host, previous_client.as_deref(), previous_subject.as_deref(), language);
        let mut state = match state().lock() { Ok(state) => state, Err(_) => return };
        match result {
            Ok(new_profile) => {
                let _lock = match lock_store() { Ok(lock) => lock, Err(error) => { state.message = Some(error); state.busy = false; return } };
                let mut latest = match load_store() { Ok(store) => store, Err(error) => { state.message = Some(error); state.busy = false; return } };
                let index = latest.profiles.iter().position(|p| p.client_id == new_profile.client_id);
                let index = if let Some(index) = index {
                    if latest.profiles[index].subject != new_profile.subject {
                        state.message = Some("The account identity changed. Register a separate account.".into());
                        state.busy = false;
                        return;
                    }
                    let mut new_profile = new_profile;
                    new_profile.welcome_seen = latest.profiles[index].welcome_seen;
                    latest.profiles[index] = new_profile;
                    index
                } else {
                    latest.profiles.push(new_profile);
                    latest.profiles.len() - 1
                };
                latest.active = Some(index);
                match save_store(&latest) {
                    Ok(()) => {
                        state.store = latest;
                        state.generation = state.generation.wrapping_add(1);
                        state.models_requested = true;
                        let token = state.store.profiles[index].access_token.clone();
                        match fetch_models(&token) {
                            Ok(models) => state.models = models,
                            Err(error) => { state.models.clear(); state.message = Some(error); }
                        }
                    }
                    Err(error) => { state.message = Some(error); }
                }
            }
            Err(error) => state.message = Some(error),
        }
        state.busy = false;
    }).map_err(|e| { if let Ok(mut state) = state().lock() { state.busy = false; } format!("Could not start account worker: {e}") })?;
    Ok(())
}

pub fn acknowledge_welcome() -> Result<(), String> {
    let mut state = state().lock().map_err(|_| "Account state unavailable.")?;
    let _lock = lock_store()?;
    let active = state.store.active.and_then(|index| state.store.profiles.get(index)).map(|p| p.client_id.clone()).ok_or("No account is selected.")?;
    let mut latest = load_store()?;
    let index = latest.profiles.iter().position(|p| p.client_id == active).ok_or("Selected account is unavailable.")?;
    latest.profiles[index].welcome_seen = true;
    save_store(&latest)?;
    state.store = latest;
    Ok(())
}

pub fn select_account(index: usize) -> Result<(), String> {
    let mut state = state().lock().map_err(|_| "Account state unavailable.")?;
    if state.busy { return Err("An account action is already running.".into()); }
    let profile = state.store.profiles.get(index).ok_or("Account no longer exists.")?;
    if profile.refresh_token.is_empty() || !profile.scopes.iter().any(|s| s == REQUIRED_SCOPE) {
        return Err("Sign in to this account again before using its plan.".into());
    }
    let client = profile.client_id.clone();
    let _lock = lock_store()?;
    let mut latest = load_store()?;
    let index = latest.profiles.iter().position(|p| p.client_id == client).ok_or("Selected account is unavailable.")?;
    latest.active = Some(index);
    save_store(&latest)?;
    state.store = latest;
    state.generation = state.generation.wrapping_add(1);
    state.models.clear();
    state.models_requested = false;
    state.message = None;
    Ok(())
}

pub fn begin_model_refresh() -> Result<(), String> {
    {
        let mut state = state().lock().map_err(|_| "Account state unavailable.")?;
        if state.busy { return Err("An account action is already running.".into()); }
        state.busy = true;
        state.models_requested = true;
        state.message = None;
    }
    std::thread::Builder::new().name("agent-account-models".into()).spawn(|| {
        let result = access_token().and_then(|token| fetch_models(&token));
        if let Ok(mut state) = state().lock() {
            match result { Ok(models) => state.models = models, Err(error) => state.message = Some(error) }
            state.busy = false;
        }
    }).map_err(|e| { if let Ok(mut state) = state().lock() { state.busy = false; state.models_requested = false; } format!("Could not start model worker: {e}") })?;
    Ok(())
}

/// Starts one background catalog request per selected account, on demand.
pub fn ensure_models_requested() {
    let should_start = state().try_lock().ok().is_some_and(|state| {
        !state.busy && !state.models_requested && state.store.active
            .and_then(|index| state.store.profiles.get(index))
            .is_some_and(|profile| !profile.refresh_token.is_empty())
    });
    if should_start { let _ = begin_model_refresh(); }
}

pub fn begin_sign_out() -> Result<(), String> {
    {
        let mut state = state().lock().map_err(|_| "Account state unavailable.")?;
        if state.busy { return Err("An account action is already running.".into()); }
        if state.store.active.is_none() { return Err("No account is selected.".into()); }
        state.busy = true;
        state.message = None;
    }
    std::thread::Builder::new().name("agent-account-sign-out".into()).spawn(|| {
        let mut state = match state().lock() { Ok(state) => state, Err(_) => return };
        let Some(client) = state.store.active.and_then(|i| state.store.profiles.get(i)).map(|p| p.client_id.clone()) else { state.busy = false; return };
        let _lock = match lock_store() { Ok(lock) => lock, Err(error) => { state.message = Some(error); state.busy = false; return } };
        let mut latest = match load_store() { Ok(store) => store, Err(error) => { state.message = Some(error); state.busy = false; return } };
        let Some(index) = latest.profiles.iter().position(|p| p.client_id == client) else { state.message = Some("Selected account is unavailable.".into()); state.busy = false; return };
        let profile = &mut latest.profiles[index];
        let revoked = revoke(&profile.client_id, &profile.refresh_token).is_ok();
        profile.access_token.clear();
        profile.refresh_token.clear();
        profile.id_token.clear();
        profile.scopes.clear();
        profile.expires_at = 0;
        if latest.active == Some(index) { latest.active = None; }
        state.models.clear();
        state.message = if revoked { None } else { Some("Signed out locally; remote revocation was not confirmed. Disconnect the app in account settings.".into()) };
        match save_store(&latest) {
            Ok(()) => { state.store = latest; state.generation = state.generation.wrapping_add(1); }
            Err(error) => state.message = Some(error),
        }
        state.busy = false;
    }).map_err(|e| { if let Ok(mut state) = state().lock() { state.busy = false; } format!("Could not start sign-out worker: {e}") })?;
    Ok(())
}

/// Called only by a provider worker. Refresh tokens rotate under the same mutex.
pub fn access_token() -> Result<String, String> {
    let client = active_registration().ok_or("Connect a ChatGPT account in AI settings.")?;
    access_token_for(&client)
}

/// Resolves the registration captured at the beginning of the run.
pub fn access_token_for(client: &str) -> Result<String, String> {
    let mut state = state().lock().map_err(|_| "Account state unavailable.")?;
    let _lock = lock_store()?;
    let mut latest = load_store()?;
    let index = latest.profiles.iter().position(|p| p.client_id == client).ok_or("Selected account is unavailable.")?;
    let profile = &mut latest.profiles[index];
    if profile.refresh_token.is_empty() || !profile.scopes.iter().any(|s| s == REQUIRED_SCOPE) {
        return Err("ChatGPT plan permission is unavailable. Sign in again or choose another provider.".into());
    }
    if profile.expires_at > chrono::Utc::now().timestamp() + 60 {
        let access = profile.access_token.clone();
        state.store = latest;
        return Ok(access);
    }
    let response = form_post(TOKEN, &[("grant_type", "refresh_token"), ("client_id", &profile.client_id),
        ("refresh_token", &profile.refresh_token), ("resource", RESOURCE)])?;
    let (access, refresh, scopes, expires) = parse_tokens(&response, Some(&profile.scopes))?;
    profile.access_token = access;
    profile.refresh_token = refresh;
    profile.scopes = scopes;
    profile.expires_at = expires;
    let access = profile.access_token.clone();
    save_store(&latest)?;
    state.store = latest;
    Ok(access)
}

fn sign_in(host: &str, client: Option<&str>, expected_subject: Option<&str>, language: raf_core::Language) -> Result<Profile, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("Could not start local callback: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let redirect = format!("http://127.0.0.1:{}{CALLBACK_PATH}", listener.local_addr().map_err(|e| e.to_string())?.port());
    let verifier = random_urlsafe(32)?;
    let challenge = URL_SAFE_NO_PAD.encode(digest::digest(&digest::SHA256, verifier.as_bytes()).as_ref());
    let csrf = random_urlsafe(32)?;
    let nonce = random_urlsafe(32)?;
    let mut url = url::Url::parse(AUTHORIZE).map_err(|e| e.to_string())?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("client_id", client.unwrap_or("dynamic_agent_client"));
        if client.is_none() { q.append_pair("agent_name_hint", "AuraRafi"); }
        q.append_pair("ext_agent_host_id", host).append_pair("response_type", "code")
            .append_pair("redirect_uri", &redirect).append_pair("scope", SCOPE).append_pair("resource", RESOURCE)
            .append_pair("state", &csrf).append_pair("nonce", &nonce)
            .append_pair("code_challenge_method", "S256").append_pair("code_challenge", &challenge);
    }
    open_browser(url.as_str())?;
    let (code, issued_client) = receive_callback(listener, &csrf, client, language)?;
    let response = form_post(TOKEN, &[("grant_type", "authorization_code"), ("client_id", &issued_client),
        ("code", &code), ("code_verifier", &verifier), ("redirect_uri", &redirect), ("resource", RESOURCE)])?;
    let id_token = required_string(&response, "id_token")?;
    let claims = validate_id_token(&id_token, &issued_client, &nonce)?;
    let subject = required_string(&claims, "sub")?;
    if expected_subject.is_some_and(|expected| expected != subject) {
        return Err("The account identity does not match the selected registration.".into());
    }
    let (access_token, refresh_token, scopes, expires_at) = parse_tokens(&response, None)?;
    Ok(Profile { subject, email: claims.get("email").and_then(Value::as_str).unwrap_or("").to_string(),
        client_id: issued_client, id_token, access_token, refresh_token, scopes, expires_at, welcome_seen: false })
}

fn receive_callback(listener: TcpListener, expected_state: &str, existing_client: Option<&str>, language: raf_core::Language) -> Result<(String, String), String> {
    let started = Instant::now();
    while started.elapsed() < LOGIN_TIMEOUT {
        let (mut stream, _) = match listener.accept() {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => { std::thread::sleep(Duration::from_millis(80)); continue; }
            Err(error) => return Err(format!("Account callback failed: {error}")),
        };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let mut request = [0u8; 8192];
        let size = stream.read(&mut request).unwrap_or(0);
        let line = String::from_utf8_lossy(&request[..size]);
        let target = line.lines().next().and_then(|line| line.strip_prefix("GET ")).and_then(|line| line.split_whitespace().next());
        let Some(target) = target else { callback_reply(&mut stream, false, language); continue };
        let Ok(url) = url::Url::parse(&format!("http://127.0.0.1{target}")) else { callback_reply(&mut stream, false, language); continue };
        if url.path() != CALLBACK_PATH { callback_reply(&mut stream, false, language); continue; }
        let pairs: Vec<_> = url.query_pairs().collect();
        let one = |key: &str| -> Option<String> {
            let mut values = pairs.iter().filter(|(name, _)| name == key).map(|(_, value)| value.to_string());
            let value = values.next()?;
            if values.next().is_some() { None } else { Some(value) }
        };
        if one("state").as_deref() != Some(expected_state) { callback_reply(&mut stream, false, language); continue; }
        if one("error").is_some() { callback_reply(&mut stream, false, language); return Err("Account authorization was declined or failed.".into()); }
        let Some(code) = one("code").filter(|c| !c.is_empty() && c.len() <= 4096) else { callback_reply(&mut stream, false, language); continue };
        let supplied_client = one("client_id");
        if existing_client.is_some_and(|expected| supplied_client.as_deref().is_some_and(|actual| actual != expected)) {
            callback_reply(&mut stream, false, language);
            return Err("Account registration did not match the selected account.".into());
        }
        let client = supplied_client.or_else(|| existing_client.map(str::to_owned)).ok_or("The account registration did not return a client ID.")?;
        if client.is_empty() || client == "dynamic_agent_client" || client.len() > 256 || !client.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-')) {
            callback_reply(&mut stream, false, language);
            return Err("The account registration returned an invalid client ID.".into());
        }
        callback_reply(&mut stream, true, language);
        return Ok((code, client));
    }
    Err("Account sign-in timed out. Try again from settings.".into())
}

fn callback_reply(stream: &mut TcpStream, received: bool, language: raf_core::Language) {
    let key = if received { "agent.account.callback_received" } else { "agent.account.callback_invalid" };
    let body = raf_core::i18n::t(key, language);
    let reply = format!("HTTP/1.1 {}\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", if received { "200 OK" } else { "400 Bad Request" }, body.len(), body);
    let _ = stream.write_all(reply.as_bytes());
}

fn random_urlsafe(len: usize) -> Result<String, String> {
    let mut bytes = vec![0u8; len];
    SystemRandom::new().fill(&mut bytes).map_err(|_| "Secure randomness unavailable.")?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn http() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(20)).redirects(0).build()
}

fn response_json(response: ureq::Response) -> Result<Value, String> {
    let mut body = Vec::new();
    response.into_reader().take(MAX_RESPONSE_BYTES + 1).read_to_end(&mut body).map_err(|e| e.to_string())?;
    if body.len() as u64 > MAX_RESPONSE_BYTES { return Err("Account response exceeded its size limit.".into()); }
    serde_json::from_slice(&body).map_err(|_| "Invalid account response.".into())
}

fn form_post(url: &str, values: &[(&str, &str)]) -> Result<Value, String> {
    let response = http().post(url).send_form(values).map_err(|error| match error {
        ureq::Error::Status(status, _) => format!("Account request failed (HTTP {status})."),
        ureq::Error::Transport(_) => "Account service unavailable; credentials were kept.".into(),
    })?;
    response_json(response)
}

fn required_string(value: &Value, key: &str) -> Result<String, String> {
    value.get(key).and_then(Value::as_str).filter(|v| !v.is_empty()).map(str::to_owned)
        .ok_or_else(|| format!("Account response is missing {key}."))
}

fn parse_tokens(value: &Value, prior_scopes: Option<&[String]>) -> Result<(String, String, Vec<String>, i64), String> {
    let access = required_string(value, "access_token")?;
    let refresh = required_string(value, "refresh_token")?;
    if !value.get("token_type").and_then(Value::as_str).is_some_and(|v| v.eq_ignore_ascii_case("bearer")) {
        return Err("Account token type is unsupported.".into());
    }
    let scopes = value.get("scope").and_then(Value::as_str).map(|s| s.split_whitespace().map(str::to_owned).collect::<Vec<_>>())
        .or_else(|| prior_scopes.map(|s| s.to_vec())).unwrap_or_default();
    if !scopes.iter().any(|s| s == REQUIRED_SCOPE) { return Err("ChatGPT plan permission was not granted.".into()); }
    let seconds = value.get("expires_in").and_then(Value::as_i64).filter(|n| *n > 0 && *n <= 86_400).ok_or("Invalid account token lifetime.")?;
    Ok((access, refresh, scopes, chrono::Utc::now().timestamp() + seconds))
}

fn validate_id_token(token: &str, client_id: &str, nonce: &str) -> Result<Value, String> {
    if token.len() > 64 * 1024 { return Err("Account identity token is too large.".into()); }
    let parts = token.split('.').collect::<Vec<_>>();
    if parts.len() != 3 { return Err("Invalid account identity token.".into()); }
    let decode = |part: &str| URL_SAFE_NO_PAD.decode(part).map_err(|_| "Invalid account identity token encoding.");
    let header: Value = serde_json::from_slice(&decode(parts[0])?).map_err(|_| "Invalid identity header.")?;
    if header.get("alg").and_then(Value::as_str) != Some("RS256") { return Err("Unsupported identity signature.".into()); }
    let kid = required_string(&header, "kid")?;
    let keys = response_json(http().get(JWKS).call().map_err(|_| "Identity keys unavailable.")?)?;
    let key = keys.get("keys").and_then(Value::as_array).and_then(|keys| keys.iter().find(|key|
        key.get("kid").and_then(Value::as_str) == Some(kid.as_str())
            && key.get("kty").and_then(Value::as_str) == Some("RSA")
            && key.get("alg").and_then(Value::as_str).is_none_or(|alg| alg == "RS256")
            && key.get("use").and_then(Value::as_str).is_none_or(|usage| usage == "sig")
    )).ok_or("Identity signing key was not found.")?;
    let modulus = decode(&required_string(key, "n")?)?;
    let exponent = decode(&required_string(key, "e")?)?;
    let sig = decode(parts[2])?;
    signature::RsaPublicKeyComponents { n: &modulus, e: &exponent }
        .verify(&signature::RSA_PKCS1_2048_8192_SHA256, format!("{}.{}", parts[0], parts[1]).as_bytes(), &sig)
        .map_err(|_| "Account identity signature could not be verified.")?;
    let claims: Value = serde_json::from_slice(&decode(parts[1])?).map_err(|_| "Invalid identity claims.")?;
    let now = chrono::Utc::now().timestamp();
    let audience = claims.get("aud");
    let aud_ok = audience.and_then(Value::as_str) == Some(client_id)
        || audience.and_then(Value::as_array).is_some_and(|items| items.iter().any(|item| item.as_str() == Some(client_id)));
    let multiple_audiences = audience.and_then(Value::as_array).is_some_and(|items| items.len() > 1);
    if claims.get("iss").and_then(Value::as_str) != Some(ISSUER) || !aud_ok
        || (multiple_audiences && claims.get("azp").and_then(Value::as_str) != Some(client_id))
        || claims.get("exp").and_then(Value::as_i64).is_none_or(|exp| exp <= now)
        || claims.get("nbf").and_then(Value::as_i64).is_some_and(|nbf| nbf > now + 60)
        || claims.get("nonce").and_then(Value::as_str) != Some(nonce)
    { return Err("Account identity claims did not match the sign-in attempt.".into()); }
    Ok(claims)
}

fn fetch_models(token: &str) -> Result<Vec<ModelView>, String> {
    let response = http().get("https://api.openai.com/v1/models").set("Authorization", &format!("Bearer {token}"))
        .call().map_err(|error| match error { ureq::Error::Status(status, _) => format!("Could not load account models (HTTP {status})."), _ => "Could not load account models.".into() })?;
    let value = response_json(response)?;
    let models = value.get("models").and_then(Value::as_array).ok_or("Account model catalog is unavailable.")?;
    Ok(models.iter().filter(|model| model.get("visibility").and_then(Value::as_str) == Some("list"))
        .filter_map(|model| Some(ModelView { slug: model.get("slug")?.as_str()?.to_string(),
            label: model.get("display_name").and_then(Value::as_str).unwrap_or(model.get("slug")?.as_str()?).to_string() }))
        .collect())
}

fn revoke(client: &str, token: &str) -> Result<(), String> {
    if token.is_empty() { return Ok(()); }
    http().post(REVOKE).send_form(&[("token", token), ("token_type_hint", "refresh_token"), ("client_id", client)])
        .map(|_| ()).map_err(|_| "Account revocation was not confirmed.".into())
}

fn storage_path() -> Result<PathBuf, String> {
    Ok(dirs_next::config_dir().ok_or("Account storage directory is unavailable.")?.join("AuraRafi").join("agent-account.dat"))
}

/// OS-backed exclusive lock releases on crash, serializing rotating credentials
/// even when several editor processes share this account store.
fn lock_store() -> Result<std::fs::File, String> {
    let path = storage_path()?;
    let parent = path.parent().ok_or("Account storage directory is unavailable.")?;
    std::fs::create_dir_all(parent).map_err(|_| "Could not create account storage directory.")?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
    let file = options.open(parent.join("agent-account.lock")).map_err(|_| "Could not open account lock.")?;
    file.lock().map_err(|_| "Could not lock account credentials.")?;
    Ok(file)
}

fn load_store() -> Result<Store, String> {
    let path = storage_path()?;
    if !path.exists() { return Ok(Store::default()); }
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::symlink_metadata(&path).map_err(|_| "Could not inspect account storage.")?;
        if !metadata.file_type().is_file() || metadata.permissions().mode() & 0o077 != 0 {
            return Err("Account storage must be a private file accessible only to its owner.".into());
        }
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path).map_err(|_| "Could not open protected account storage.")?
        .take(MAX_RESPONSE_BYTES + 1).read_to_end(&mut bytes).map_err(|_| "Could not read account storage.")?;
    if bytes.len() as u64 > MAX_RESPONSE_BYTES { return Err("Account storage exceeds its limit.".into()); }
    let plaintext = unprotect(&bytes)?;
    serde_json::from_slice(&plaintext).map_err(|_| "Account storage could not be decoded.".into())
}

fn save_store(store: &Store) -> Result<(), String> {
    let path = storage_path()?;
    let parent = path.parent().ok_or("Account storage directory is unavailable.")?;
    std::fs::create_dir_all(parent).map_err(|_| "Could not create account storage directory.")?;
    let bytes = protect(&serde_json::to_vec(store).map_err(|_| "Could not encode account storage.")?)?;
    let temporary = parent.join(format!(".agent-account-{}.tmp", Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
    let mut file = options.open(&temporary).map_err(|_| "Could not create protected account storage.")?;
    file.write_all(&bytes).and_then(|_| file.sync_all()).map_err(|_| "Could not write account storage.")?;
    drop(file);
    std::fs::rename(&temporary, &path).map_err(|_| "Could not replace account storage.")?;
    Ok(())
}

#[cfg(unix)]
fn protect(bytes: &[u8]) -> Result<Vec<u8>, String> { Ok(bytes.to_vec()) }
#[cfg(unix)]
fn unprotect(bytes: &[u8]) -> Result<Vec<u8>, String> { Ok(bytes.to_vec()) }

#[cfg(windows)]
fn protect(bytes: &[u8]) -> Result<Vec<u8>, String> { crypt(bytes, true) }
#[cfg(windows)]
fn unprotect(bytes: &[u8]) -> Result<Vec<u8>, String> { crypt(bytes, false) }

#[cfg(windows)]
fn crypt(bytes: &[u8], encrypt: bool) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN};
    let input = CRYPT_INTEGER_BLOB { cbData: u32::try_from(bytes.len()).map_err(|_| "Account storage is too large.")?, pbData: bytes.as_ptr() as *mut u8 };
    let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    let result = unsafe {
        if encrypt { CryptProtectData(&input, std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut output) }
        else { CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut output) }
    };
    if result == 0 || output.pbData.is_null() { return Err("Windows could not protect account credentials for this user.".into()); }
    let result = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe { LocalFree(output.pbData as *mut _); }
    Ok(result)
}

#[cfg(not(any(unix, windows)))]
fn protect(_: &[u8]) -> Result<Vec<u8>, String> { Err("Protected account storage is not available on this platform.".into()) }
#[cfg(not(any(unix, windows)))]
fn unprotect(_: &[u8]) -> Result<Vec<u8>, String> { Err("Protected account storage is not available on this platform.".into()) }

#[cfg(windows)]
fn open_browser(url: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = url.encode_utf16().chain(std::iter::once(0)).collect::<Vec<_>>();
    let result = unsafe { ShellExecuteW(std::ptr::null_mut(), std::ptr::null(), wide.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) };
    if result as isize <= 32 { Err("Could not open the system browser.".into()) } else { Ok(()) }
}

#[cfg(not(windows))]
fn open_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")] let program = "open";
    #[cfg(not(target_os = "macos"))] let program = "xdg-open";
    std::process::Command::new(program).arg(url).spawn().map(|_| ()).map_err(|_| "Could not open the system browser.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn token_scope_and_lifetime_are_required() {
        let value = serde_json::json!({"access_token":"a", "refresh_token":"r", "token_type":"Bearer", "expires_in":3600, "scope":"openid chatgpt.tokens.use.direct"});
        assert!(parse_tokens(&value, None).is_ok());
        let mut denied = value.clone(); denied["scope"] = Value::String("openid".into());
        assert!(parse_tokens(&denied, None).is_err());
    }
    #[test]
    fn protected_storage_round_trips_without_exposing_plaintext_on_windows() {
        let bytes = b"fixture-account-token";
        let protected = protect(bytes).unwrap();
        #[cfg(windows)] assert!(!protected.windows(bytes.len()).any(|window| window == bytes));
        assert_eq!(unprotect(&protected).unwrap(), bytes);
    }
}
