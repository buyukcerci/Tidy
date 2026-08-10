use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::{STANDARD as BASE64_STD, URL_SAFE_NO_PAD as BASE64_URL};
use base64::Engine;
use getrandom::fill as fill_random;
use reqwest::blocking::{Client, Response};
use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const OAUTH_SERVICE: &str = "com.buyukcerci.tidy";

const AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const USERINFO_ENDPOINT: &str = "https://www.googleapis.com/oauth2/v3/userinfo";
const REVOKE_ENDPOINT: &str = "https://oauth2.googleapis.com/revoke";
const CALLBACK_PATH: &str = "/oauth/callback";
const FLOW_TIMEOUT: Duration = Duration::from_secs(300);
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

pub const REQUESTED_SCOPES: &[&str] = &[
    "openid",
    "email",
    "profile",
    "https://www.googleapis.com/auth/drive",
];

#[derive(Debug, Clone, serde::Serialize)]
pub struct OauthConfig {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GoogleIdentity {
    pub subject_id: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
}

pub struct FlowResult {
    pub identity: GoogleIdentity,
    pub access_token: String,
    pub refresh_token: String,
    pub access_token_expires_in: u64,
}

#[derive(Debug)]
pub struct TokenEndpointError {
    pub status: u16,
    pub code: Option<String>,
    pub message: String,
}

impl TokenEndpointError {
    pub fn requires_reauthentication(&self) -> bool {
        matches!(
            self.code.as_deref(),
            Some("invalid_grant" | "invalid_client")
        ) || ((400..500).contains(&self.status) && !matches!(self.status, 408 | 429))
    }
}

#[derive(Debug)]
pub enum OAuthError {
    Entropy,
    Listener(std::io::Error),
    Browser,
    Timeout,
    Cancelled,
    InvalidState,
    Denied(String),
    MissingCode,
    Http(String),
    TokenResponse(TokenEndpointError),
    Identity(String),
}

impl std::fmt::Display for OAuthError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::Entropy => "could not generate secure random data".to_string(),
            Self::Listener(error) => format!("could not start local callback server: {error}"),
            Self::Browser => "could not open your default web browser".to_string(),
            Self::Timeout => "sign-in timed out. Please try again.".to_string(),
            Self::Cancelled => "sign-in was cancelled".to_string(),
            Self::InvalidState => {
                "sign-in response could not be verified. Please try again.".to_string()
            }
            Self::Denied(reason) => format!("sign-in was not completed: {reason}"),
            Self::MissingCode => {
                "sign-in response did not include an authorization code".to_string()
            }
            Self::Http(error) => format!("could not contact Google (HTTP {error})"),
            Self::TokenResponse(error) => {
                format!("Google rejected the sign-in response: {}", error.message)
            }
            Self::Identity(message) => {
                format!("could not read account identity from Google: {message}")
            }
        };
        formatter.write_str(&message)
    }
}

impl std::error::Error for OAuthError {}

fn sanitize_sentence(message: &str) -> String {
    let mut clean = String::new();
    let mut previous_space = false;
    for character in message.chars() {
        if character.is_ascii() && !character.is_ascii_control() && character != '"' {
            if character == ' ' {
                if previous_space {
                    continue;
                }
                previous_space = true;
            } else {
                previous_space = false;
            }
            clean.push(character);
        }
    }
    clean.trim().chars().take(240).collect()
}

fn next_random_bytes<const N: usize>() -> Result<[u8; N], OAuthError> {
    let mut bytes = [0u8; N];
    fill_random(&mut bytes).map_err(|_| OAuthError::Entropy)?;
    Ok(bytes)
}

fn generate_pkce_verifier() -> Result<String, OAuthError> {
    // 32 random bytes base64url-encoded produce a 43-character verifier,
    // within the 43..=128 range required by RFC 7636.
    let bytes = next_random_bytes::<32>()?;
    Ok(BASE64_URL.encode(bytes))
}

fn pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    BASE64_URL.encode(digest)
}

fn generate_state() -> Result<String, OAuthError> {
    let bytes = next_random_bytes::<32>()?;
    Ok(BASE64_STD.encode(bytes))
}

fn encode_query(parameters: &[(&str, String)]) -> String {
    let mut query = Vec::new();
    for (key, value) in parameters {
        query.push(
            url::form_urlencoded::Serializer::new(String::new())
                .append_pair(key, value)
                .finish(),
        );
    }
    query.join("&")
}

struct PendingFlow {
    verifier: String,
    state: String,
    redirect_uri: String,
}

impl PendingFlow {
    fn start() -> Result<(Self, TcpListener), OAuthError> {
        let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .map_err(OAuthError::Listener)?;
        listener
            .set_nonblocking(true)
            .map_err(OAuthError::Listener)?;
        let port = listener.local_addr().map_err(OAuthError::Listener)?.port();
        let redirect_uri = format!("http://127.0.0.1:{port}{CALLBACK_PATH}");
        Ok((
            Self {
                verifier: generate_pkce_verifier()?,
                state: generate_state()?,
                redirect_uri,
            },
            listener,
        ))
    }

    fn authorization_url(&self, config: &OauthConfig) -> String {
        let scope = REQUESTED_SCOPES.join(" ");
        format!(
            "{AUTH_ENDPOINT}?{}",
            encode_query(&[
                ("client_id", config.client_id.clone()),
                ("redirect_uri", self.redirect_uri.clone()),
                ("response_type", "code".to_string()),
                ("scope", scope),
                ("state", self.state.clone()),
                ("code_challenge", pkce_challenge(&self.verifier)),
                ("code_challenge_method", "S256".to_string()),
                ("access_type", "offline".to_string()),
                ("prompt", "consent".to_string()),
            ])
        )
    }
}

pub struct CallbackRequest {
    state: Option<String>,
    code: Option<String>,
    error: Option<String>,
}

fn read_http_request(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") || buffer.len() > 65536 {
            break;
        }
    }
    Ok(buffer)
}

fn parse_callback_request(buffer: &[u8]) -> CallbackRequest {
    let first_line = String::from_utf8_lossy(
        &buffer[..buffer
            .iter()
            .position(|&byte| byte == b'\n')
            .unwrap_or(buffer.len())],
    )
    .trim()
    .to_string();
    let target = first_line.split_whitespace().nth(1).unwrap_or("/");
    let parsed = url::Url::parse(&format!("http://127.0.0.1{target}"))
        .ok()
        .filter(|parsed| parsed.path() == CALLBACK_PATH);
    if let Some(parsed) = parsed {
        let parameters: Vec<(String, String)> = parsed.query_pairs().into_owned().collect();
        CallbackRequest {
            state: get_param(&parameters, "state"),
            code: get_param(&parameters, "code"),
            error: get_param(&parameters, "error"),
        }
    } else {
        CallbackRequest {
            state: None,
            code: None,
            error: None,
        }
    }
}

fn get_param(parameters: &[(String, String)], key: &str) -> Option<String> {
    parameters
        .iter()
        .find(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.clone())
}

fn send_response(stream: &mut TcpStream, status: &str, title: &str, body: &str) {
    let html = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <title>{title}</title></head><body style=\"font-family:system-ui,sans-serif;padding:40px\">\
         <p>{body}</p></body></html>"
    );
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{html}",
        html.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn wait_for_callback(
    listener: &TcpListener,
    flow: &PendingFlow,
    cancel: &AtomicBool,
) -> Result<CallbackRequest, OAuthError> {
    let deadline = Instant::now() + FLOW_TIMEOUT;
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(OAuthError::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(OAuthError::Timeout);
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                let buffer = read_http_request(&mut stream).map_err(OAuthError::Listener)?;
                let request = parse_callback_request(&buffer);
                let page_body = if request.state.as_deref() == Some(flow.state.as_str()) {
                    send_response(
                        &mut stream,
                        "200 OK",
                        "Tidy Sign-In",
                        "Sign-in complete. You can close this window and return to Tidy.",
                    );
                    request
                } else {
                    send_response(
                        &mut stream,
                        "400 Bad Request",
                        "Tidy Sign-In",
                        "This sign-in request could not be verified. Close this window and try again in Tidy.",
                    );
                    return Err(OAuthError::InvalidState);
                };
                return Ok(page_body);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(OAuthError::Listener(error)),
        }
    }
}

pub fn run_flow(config: &OauthConfig, cancel: Arc<AtomicBool>) -> Result<FlowResult, OAuthError> {
    let (flow, listener) = PendingFlow::start()?;
    let authorization_url = flow.authorization_url(config);

    opener::open(&authorization_url).map_err(|_| OAuthError::Browser)?;

    let callback = wait_for_callback(&listener, &flow, &cancel)?;

    if let Some(error) = callback.error {
        return Err(OAuthError::Denied(sanitize_sentence(&error)));
    }
    let authorization_code = callback.code.ok_or(OAuthError::MissingCode)?;

    let client = http_client();
    let token_response = exchange_authorization_code(&client, config, &flow, &authorization_code)?;

    // Resolve the account identity through Google's HTTPS userinfo endpoint using
    // the freshly issued access token. Claim values are read from a verified
    // transport, and a subject is mandatory, so the persisted account identity
    // cannot be forged by tampering with an unverified ID token.
    let identity = fetch_identity(&client, &token_response.access_token)?;

    let refresh_token = token_response.refresh_token.ok_or_else(|| {
        OAuthError::TokenResponse(TokenEndpointError {
            status: 200,
            code: None,
            message: "no refresh token returned".to_string(),
        })
    })?;

    Ok(FlowResult {
        identity,
        access_token: token_response.access_token,
        refresh_token,
        access_token_expires_in: token_response.expires_in.unwrap_or(3600),
    })
}

fn http_client() -> Client {
    let builder = reqwest::blocking::Client::builder().timeout(HTTP_TIMEOUT);
    builder.build().unwrap_or_else(|_| Client::new())
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
}

fn validate_token_response(response: Response) -> Result<TokenResponse, OAuthError> {
    let status = response.status();
    if !status.is_success() {
        let status = status.as_u16();
        let body = response.text().unwrap_or_default();
        return Err(OAuthError::TokenResponse(parse_token_endpoint_error(
            status, &body,
        )));
    }
    response.json::<TokenResponse>().map_err(|_| {
        OAuthError::TokenResponse(TokenEndpointError {
            status: status.as_u16(),
            code: None,
            message: "malformed response".to_string(),
        })
    })
}

#[derive(Debug, Deserialize)]
struct GoogleTokenError {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

fn parse_token_endpoint_error(status: u16, body: &str) -> TokenEndpointError {
    let parsed = serde_json::from_str::<GoogleTokenError>(body).ok();
    let code = parsed.as_ref().map(|error| sanitize_sentence(&error.error));
    let message = parsed
        .and_then(|error| error.error_description)
        .map(|message| sanitize_sentence(&message))
        .filter(|message| !message.is_empty())
        .or_else(|| code.clone())
        .unwrap_or_else(|| format!("HTTP {status}"));
    TokenEndpointError {
        status,
        code,
        message,
    }
}

#[derive(Debug, Deserialize)]
struct UserInfoResponse {
    sub: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

fn parse_userinfo_response(body: &[u8]) -> Result<GoogleIdentity, OAuthError> {
    let payload: UserInfoResponse = serde_json::from_slice(body)
        .map_err(|_| OAuthError::Identity("malformed user profile".to_string()))?;
    if payload.sub.is_empty() {
        return Err(OAuthError::Identity("missing subject".to_string()));
    }
    Ok(GoogleIdentity {
        subject_id: payload.sub,
        email: payload.email,
        display_name: payload.name,
    })
}

fn fetch_identity(client: &Client, access_token: &str) -> Result<GoogleIdentity, OAuthError> {
    let response = client
        .get(USERINFO_ENDPOINT)
        .bearer_auth(access_token)
        .send()
        .map_err(|error| OAuthError::Http(error.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        if let Ok(text) = response.text() {
            return Err(OAuthError::Identity(sanitize_sentence(&text)));
        }
        return Err(OAuthError::Http(status.as_u16().to_string()));
    }
    let body = response
        .bytes()
        .map_err(|_| OAuthError::Identity("empty user profile".to_string()))?;
    parse_userinfo_response(&body)
}

fn exchange_authorization_code(
    client: &Client,
    config: &OauthConfig,
    flow: &PendingFlow,
    authorization_code: &str,
) -> Result<TokenResponse, OAuthError> {
    let response = client
        .post(TOKEN_ENDPOINT)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", authorization_code),
            ("redirect_uri", flow.redirect_uri.as_str()),
            ("client_id", config.client_id.as_str()),
            ("client_secret", config.client_secret.as_str()),
            ("code_verifier", flow.verifier.as_str()),
        ])
        .send()
        .map_err(|error| OAuthError::Http(error.to_string()))?;
    validate_token_response(response)
}

pub fn refresh_access_token(
    config: &OauthConfig,
    refresh_token: &str,
) -> Result<(String, u64), OAuthError> {
    let client = http_client();
    let response = client
        .post(TOKEN_ENDPOINT)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", config.client_id.as_str()),
            ("client_secret", config.client_secret.as_str()),
            ("refresh_token", refresh_token),
        ])
        .send()
        .map_err(|error| OAuthError::Http(error.to_string()))?;
    let token_response = validate_token_response(response)?;
    Ok((
        token_response.access_token,
        token_response.expires_in.unwrap_or(3600),
    ))
}

pub fn revoke_token(token: &str) -> Result<(), OAuthError> {
    let client = http_client();
    let response = client
        .post(REVOKE_ENDPOINT)
        .form(&[("token", token)])
        .send()
        .map_err(|error| OAuthError::Http(error.to_string()))?;
    if response.status().is_success() || response.status().as_u16() == 400 {
        Ok(())
    } else {
        Err(OAuthError::Http(response.status().as_u16().to_string()))
    }
}

pub fn keychain_username(subject_id: &str) -> String {
    format!("google:{}", subject_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_verifier_meets_rfc7636_requirements() {
        let verifier = generate_pkce_verifier().expect("verifier should generate");
        assert!((43..=128).contains(&verifier.len()));
        assert!(verifier
            .chars()
            .all(|character| character.is_ascii_alphanumeric()
                || character == '-'
                || character == '_'));
        let challenge = pkce_challenge(&verifier);
        assert_eq!(challenge.len(), 43);
    }

    #[test]
    fn verifiers_are_unique() {
        let first = generate_pkce_verifier().expect("verifier should generate");
        let second = generate_pkce_verifier().expect("verifier should generate");
        assert_ne!(first, second);
    }

    #[test]
    fn state_has_enough_entropy() {
        let state = generate_state().expect("state should generate");
        assert_eq!(state.len(), 44);
        assert_ne!(state, generate_state().expect("state should generate"));
    }

    #[test]
    fn pkce_challenge_matches_known_vector() {
        // RFC 7636 Appendix B example.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            pkce_challenge(verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn parse_userinfo_response_extracts_identity() {
        let body = br#"{"sub":"1234567890","email":"user@example.com","name":"Example User","picture":"https://example.com/photo","locale":"en"}"#;
        let identity = parse_userinfo_response(body).expect("identity should parse");
        assert_eq!(identity.subject_id, "1234567890");
        assert_eq!(identity.email, Some("user@example.com".to_string()));
        assert_eq!(identity.display_name, Some("Example User".to_string()));
    }

    #[test]
    fn parse_userinfo_response_allows_optional_fields() {
        let body = br#"{"sub":"1234567890"}"#;
        let identity = parse_userinfo_response(body).expect("identity should parse");
        assert_eq!(identity.subject_id, "1234567890");
        assert_eq!(identity.email, None);
        assert_eq!(identity.display_name, None);
    }

    #[test]
    fn parse_userinfo_response_rejects_malformed_or_missing_subject() {
        assert!(parse_userinfo_response(b"not-json").is_err());
        assert!(parse_userinfo_response(br#"{"email":"user@example.com"}"#).is_err());
        assert!(parse_userinfo_response(br#"{"sub":""}"#).is_err());
        assert!(parse_userinfo_response(br#"{}"#).is_err());
    }

    #[test]
    fn invalid_grant_requires_reauthentication() {
        let error = parse_token_endpoint_error(
            400,
            r#"{"error":"invalid_grant","error_description":"Token has been revoked."}"#,
        );
        assert!(error.requires_reauthentication());
        assert_eq!(error.code.as_deref(), Some("invalid_grant"));
        assert_eq!(error.message, "Token has been revoked.");
    }

    #[test]
    fn transient_token_failures_do_not_require_reauthentication() {
        for status in [408, 429, 500, 502, 503, 504] {
            let error = parse_token_endpoint_error(status, "");
            assert!(
                !error.requires_reauthentication(),
                "HTTP {status} should be temporary"
            );
        }
    }

    #[test]
    fn unknown_non_retryable_client_error_requires_reauthentication() {
        let error = parse_token_endpoint_error(403, r#"{"error":"access_denied"}"#);
        assert!(error.requires_reauthentication());
    }

    #[test]
    fn callback_parsing_extracts_code_and_state() {
        let buffer =
            b"GET /oauth/callback?code=ABC123&state=XYZ HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        let request = parse_callback_request(buffer);
        assert_eq!(request.code.as_deref(), Some("ABC123"));
        assert_eq!(request.state.as_deref(), Some("XYZ"));
        assert_eq!(request.error, None);
    }

    #[test]
    fn callback_parsing_extracts_error() {
        let buffer = b"GET /oauth/callback?error=access_denied&state=XYZ HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        let request = parse_callback_request(buffer);
        assert_eq!(request.error.as_deref(), Some("access_denied"));
        assert_eq!(request.code, None);
    }

    #[test]
    fn callback_parsing_rejects_wrong_path() {
        let buffer = b"GET /other?code=ABC HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        let request = parse_callback_request(buffer);
        assert_eq!(request.code, None);
        assert_eq!(request.state, None);
    }

    #[test]
    fn sanitize_sentence_removes_quotes_and_controls() {
        let message = sanitize_sentence("error=\"confidential\"\n desc         with spaces");
        assert!(!message.contains('"'));
        assert!(!message.contains('\n'));
        assert!(!message.contains("  "));
    }

    #[test]
    fn sanitize_sentence_is_bounded() {
        let long = "x".repeat(10_000);
        assert!(sanitize_sentence(&long).len() <= 240);
    }

    #[test]
    fn keychain_username_embeds_subject() {
        assert_eq!(keychain_username("abc"), "google:abc");
    }
}
