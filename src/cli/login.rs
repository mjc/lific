//! `lific login` / `lific logout` — device-flow authentication (LIF-252).
//!
//! The device flow (RFC 8628) works everywhere a browser-redirect OAuth flow
//! does not: over SSH, in containers, in CI, and for agents. The CLI asks the
//! server for a `device_code` + short `user_code`, shows the human a URL and
//! the code, and polls the token endpoint until the human approves on any
//! device.
//!
//! Two entry shapes, mirroring Stripe's non-interactive pattern:
//!
//! - **Interactive** (`lific login` at a TTY): request a device code, print the
//!   URL + code, poll to completion, store the token.
//! - **Non-interactive** (`--non-interactive`, or stdin not a TTY): request a
//!   device code, print it as JSON with a `next_step`, and exit 0 without
//!   polling. A follow-up `lific login --complete <device_code>` polls to
//!   completion once a human has approved.
//!
//! ## Testability
//!
//! Network calls go through the [`DeviceFlow`] trait so the polling loop and
//! its interval/backoff arithmetic are unit-testable against a scripted fake
//! (see tests). [`poll_backoff`] is the pure interval calculation.

use std::time::Duration;

use serde::Deserialize;

use crate::config::Config;

/// The device-code grant type string (RFC 8628).
const DEVICE_CODE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// Resolve the base URL: explicit `--url` wins, else `server.public_url`, else
/// `http://127.0.0.1:<port>`.
pub fn resolve_base_url(url: Option<&str>, cfg: &Config) -> String {
    if let Some(u) = url {
        return u.trim().trim_end_matches('/').to_string();
    }
    if let Some(pu) = cfg.server.public_url.as_deref() {
        return pu.trim().trim_end_matches('/').to_string();
    }
    format!("http://127.0.0.1:{}", cfg.server.port)
}

/// The response from `POST /oauth/device_authorization`.
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceAuthResponse {
    /// The client sent with this grant request, retained locally for polling.
    #[serde(skip)]
    pub client_id: Option<String>,
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Deserialize)]
struct ClientRegistrationResponse {
    client_id: String,
}

/// Terminal outcome of a token poll.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollOutcome {
    /// The token was minted; carries the raw access token.
    Approved(String),
    /// The user denied the request.
    Denied,
    /// The device code expired before approval.
    Expired,
}

/// A single non-terminal poll signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollSignal {
    Pending,
    SlowDown,
    Terminal(PollOutcome),
}

/// Abstraction over the two network operations the login flow needs, so the
/// polling loop is testable without a live server.
pub trait DeviceFlow {
    /// `POST {base}/oauth/device_authorization`.
    fn request_device_code(&self, label: Option<&str>) -> Result<DeviceAuthResponse, String>;
    /// One `POST {base}/oauth/token` with the device grant. Maps the RFC 8628
    /// error/status into a [`PollSignal`].
    fn poll_token(&self, device_code: &str, client_id: Option<&str>) -> Result<PollSignal, String>;
    /// Best-effort `POST {base}/oauth/revoke` (used by logout). Returns Ok even
    /// if the server rejects it; only transport errors surface.
    fn revoke(&self, token: &str) -> Result<(), String>;
}

/// Compute the next sleep interval given the base interval and how many
/// consecutive `slow_down` signals we've seen. RFC 8628 says to increase the
/// interval by 5 seconds on each `slow_down`.
pub fn poll_backoff(base_interval: u64, slow_downs: u32) -> u64 {
    base_interval + 5 * (slow_downs as u64)
}

/// Run the polling loop to a terminal outcome, sleeping `sleep` between polls.
/// `deadline_secs` is a hard cap (the device code's `expires_in`); if exceeded
/// we return `Expired`. Factored to take a `sleep` closure so tests can run it
/// instantly.
pub fn poll_loop<F, S>(
    flow: &F,
    device_code: &str,
    client_id: Option<&str>,
    interval: u64,
    deadline_secs: u64,
    mut sleep: S,
) -> Result<PollOutcome, String>
where
    F: DeviceFlow,
    S: FnMut(Duration),
{
    let start = std::time::Instant::now();
    let mut slow_downs: u32 = 0;
    loop {
        if start.elapsed().as_secs() >= deadline_secs {
            return Ok(PollOutcome::Expired);
        }
        match flow.poll_token(device_code, client_id)? {
            PollSignal::Terminal(outcome) => return Ok(outcome),
            PollSignal::Pending => {
                sleep(Duration::from_secs(poll_backoff(interval, slow_downs)));
            }
            PollSignal::SlowDown => {
                slow_downs += 1;
                sleep(Duration::from_secs(poll_backoff(interval, slow_downs)));
            }
        }
    }
}

/// Build the non-interactive JSON payload printed by `--non-interactive`.
/// Arbitrary client IDs requiring quoting mark `next_step_shell` as `posix`;
/// consumers using another shell can pass the separate `client_id` directly.
pub fn non_interactive_json(resp: &DeviceAuthResponse, base: &str) -> serde_json::Value {
    let mut needs_posix_shell = false;
    let next_step = (|| {
        let quoted_code = shlex::try_quote(&resp.device_code)?;
        let quoted_base = shlex::try_quote(base)?;
        needs_posix_shell =
            quoted_code.as_ref() != resp.device_code || quoted_base.as_ref() != base;
        let mut command = format!("lific login --complete {quoted_code} --url {quoted_base}");
        if let Some(client_id) = &resp.client_id {
            command.push_str(" --client-id=");
            // UUIDs and this restricted alphabet remain usable in POSIX,
            // PowerShell and cmd.exe, where single quotes are literal.
            if !client_id.is_empty()
                && client_id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            {
                command.push_str(client_id);
            } else {
                needs_posix_shell = true;
                command.push_str(&shlex::try_quote(client_id)?);
            }
        }
        Ok::<_, shlex::QuoteError>(command)
    })();
    let cannot_quote = next_step.is_err();
    let mut payload = serde_json::json!({
        "verification_uri": resp.verification_uri,
        "verification_uri_complete": resp.verification_uri_complete,
        "user_code": resp.user_code,
        "device_code": resp.device_code,
        "interval": resp.interval,
        "expires_in": resp.expires_in,
        "next_step": next_step.ok(),
    });
    if let Some(client_id) = &resp.client_id {
        payload["client_id"] = client_id.clone().into();
    }
    if cannot_quote {
        payload["next_step_error"] = "Completion arguments cannot contain NUL bytes".into();
    } else if needs_posix_shell {
        payload["next_step_shell"] = "posix".into();
    }
    payload
}

// ── reqwest-backed implementation ────────────────────────────────────────

/// Live [`DeviceFlow`] backed by a blocking reqwest client against `base`.
pub struct HttpDeviceFlow {
    base: String,
    client: reqwest::blocking::Client,
}

impl HttpDeviceFlow {
    pub fn new(base: &str) -> Result<Self, String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| format!("failed to build HTTP client: {e}"))?;
        Ok(Self {
            base: base.trim_end_matches('/').to_string(),
            client,
        })
    }
}

/// Why a device-authorization request failed, when the caller can react to it.
enum DeviceAuthFailure {
    /// The server does not know the `client_id` we sent. It was reclaimed, or
    /// the database was rebuilt; registering again is the right response.
    UnknownClient,
    Other(String),
}

impl HttpDeviceFlow {
    /// Register a redirect-free device client. `Ok(None)` means the server
    /// refused the registration itself, which is how a server predating
    /// registered device clients answers: it requires at least one
    /// redirect_uri. That is not a failure, it is a signal to fall back.
    fn register_device_client(&self, label: Option<&str>) -> Result<Option<String>, String> {
        let registration = serde_json::json!({
            "redirect_uris": [],
            "client_name": label.unwrap_or("Lific CLI"),
            "grant_types": [DEVICE_CODE_GRANT],
            "response_types": [],
        });
        let response = self
            .client
            .post(format!("{}/oauth/register", self.base))
            .json(&registration)
            .send()
            .map_err(|e| format!("client registration failed: {e}"))?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            // An older server rejects a redirect-free registration with
            // `invalid_redirect_uri`, because it requires at least one. Match
            // on that specifically: any other rejection is a real failure and
            // must not be masked by silently falling back.
            if status == reqwest::StatusCode::BAD_REQUEST && body.contains("invalid_redirect_uri") {
                return Ok(None);
            }
            return Err(format!(
                "client registration failed (HTTP {}): {body}",
                status.as_u16()
            ));
        }
        let registration: ClientRegistrationResponse = response
            .json()
            .map_err(|e| format!("invalid client registration response: {e}"))?;
        Ok(Some(registration.client_id))
    }

    /// One `POST /oauth/device_authorization` with an already-chosen form.
    fn device_authorization(
        &self,
        form: &[(&str, &str)],
    ) -> Result<DeviceAuthResponse, DeviceAuthFailure> {
        let resp = self
            .client
            .post(format!("{}/oauth/device_authorization", self.base))
            .form(form)
            .send()
            .map_err(|e| {
                DeviceAuthFailure::Other(format!("device authorization request failed: {e}"))
            })?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            if status == reqwest::StatusCode::BAD_REQUEST && body.contains("invalid_client") {
                return Err(DeviceAuthFailure::UnknownClient);
            }
            return Err(DeviceAuthFailure::Other(format!(
                "device authorization failed (HTTP {}): {body}",
                status.as_u16()
            )));
        }
        let mut authorization = resp.json::<DeviceAuthResponse>().map_err(|e| {
            DeviceAuthFailure::Other(format!("invalid device authorization response: {e}"))
        })?;
        authorization.client_id = form
            .iter()
            .find_map(|(key, value)| (*key == "client_id").then(|| (*value).to_string()));
        Ok(authorization)
    }
}

impl DeviceFlow for HttpDeviceFlow {
    fn request_device_code(&self, label: Option<&str>) -> Result<DeviceAuthResponse, String> {
        // Reuse the client already registered with this server if there is
        // one. A server can never reclaim a client that has minted a token,
        // so registering per login would leak one of its dynamic-client slots
        // every time, and spend one of this IP's ten hourly registrations.
        if let Some(client_id) = crate::cli::credentials::load_client_id(&self.base)
            .map_err(|error| error.to_string())?
        {
            match self.device_authorization(&[("scope", "mcp"), ("client_id", &client_id)]) {
                Ok(resp) => return Ok(resp),
                Err(DeviceAuthFailure::Other(e)) => return Err(e),
                // Stale id: fall through and register a new one.
                Err(DeviceAuthFailure::UnknownClient) => {
                    if let Err(error) = crate::cli::credentials::forget_client_id(&self.base) {
                        eprintln!(
                            "warning: could not remove stale OAuth client cache entry for {}: {error}; continuing with a new registration",
                            self.base
                        );
                    }
                }
            }
        }

        let Some(client_id) = self.register_device_client(label)? else {
            // Older server: it has no registered device clients and takes the
            // client name on the device request itself. Keeping this path
            // means a freshly installed CLI still logs into an older
            // instance instead of failing at a registration it cannot make.
            let mut form: Vec<(&str, &str)> = Vec::new();
            if let Some(label) = label {
                form.push(("client_name", label));
            }
            return self.device_authorization(&form).map_err(|e| match e {
                DeviceAuthFailure::UnknownClient => "device authorization failed: the server \
                                                         requires a registered client but \
                                                         refused to register one"
                    .to_string(),
                DeviceAuthFailure::Other(e) => e,
            });
        };
        if let Err(error) = crate::cli::credentials::store_client_id(&self.base, &client_id) {
            eprintln!(
                "warning: could not cache OAuth client registration for {}: {error}; continuing with the registered client",
                self.base
            );
        }

        self.device_authorization(&[("scope", "mcp"), ("client_id", &client_id)])
            .map_err(|e| match e {
                DeviceAuthFailure::UnknownClient => {
                    "device authorization failed: the server did not recognize the client it \
                     had just registered"
                        .to_string()
                }
                DeviceAuthFailure::Other(e) => e,
            })
    }

    fn poll_token(&self, device_code: &str, client_id: Option<&str>) -> Result<PollSignal, String> {
        let url = format!("{}/oauth/token", self.base);
        let mut form = vec![
            ("grant_type", DEVICE_CODE_GRANT),
            ("device_code", device_code),
        ];
        if let Some(client_id) = client_id {
            form.push(("client_id", client_id));
        }
        let resp = self
            .client
            .post(&url)
            .form(&form)
            .send()
            .map_err(|e| format!("token poll failed: {e}"))?;
        let status = resp.status();
        let body: serde_json::Value = resp.json().unwrap_or_else(|_| serde_json::json!({}));
        if status.is_success() {
            let token = body
                .get("access_token")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "token response missing access_token".to_string())?;
            return Ok(PollSignal::Terminal(PollOutcome::Approved(
                token.to_string(),
            )));
        }
        let err = body.get("error").and_then(|v| v.as_str()).unwrap_or("");
        Ok(classify_poll_error(err))
    }

    fn revoke(&self, token: &str) -> Result<(), String> {
        let url = format!("{}/oauth/revoke", self.base);
        let form = [("token", token)];
        // Revoke requires auth; present the token itself as the bearer.
        let _ = self
            .client
            .post(&url)
            .bearer_auth(token)
            .form(&form)
            .send()
            .map_err(|e| format!("revoke request failed: {e}"))?;
        Ok(())
    }
}

/// Map an RFC 8628 §3.5 `error` code to a poll signal.
pub fn classify_poll_error(error: &str) -> PollSignal {
    match error {
        "authorization_pending" => PollSignal::Pending,
        "slow_down" => PollSignal::SlowDown,
        "access_denied" => PollSignal::Terminal(PollOutcome::Denied),
        "expired_token" => PollSignal::Terminal(PollOutcome::Expired),
        // Any other error (invalid_grant, etc.) is terminal-ish; treat as
        // expired so the loop stops rather than spinning forever.
        _ => PollSignal::Terminal(PollOutcome::Expired),
    }
}

// ── Command entry points ─────────────────────────────────────────────────

/// Arguments for [`run_login`], mirrored from the CLI enum.
pub struct LoginArgs {
    pub url: Option<String>,
    pub non_interactive: bool,
    pub complete: Option<String>,
    pub client_id: Option<String>,
    pub label: Option<String>,
    pub no_store: bool,
}

/// `lific login`. Returns `Ok(())` on success (or after printing the
/// non-interactive JSON), `Err` on failure.
pub fn run_login(args: &LoginArgs, cfg: &Config, json: bool) -> Result<(), String> {
    let base = resolve_base_url(args.url.as_deref(), cfg);
    let flow = HttpDeviceFlow::new(&base)?;
    run_login_with_flow(args, &base, &flow, json, crate::cli::term::stdin_is_tty())
}

/// Testable core: takes an injected [`DeviceFlow`] and TTY state.
pub fn run_login_with_flow<F: DeviceFlow>(
    args: &LoginArgs,
    base: &str,
    flow: &F,
    json: bool,
    stdin_tty: bool,
) -> Result<(), String> {
    // `--complete <device_code>`: skip requesting a new code; poll the given one.
    if let Some(device_code) = &args.complete {
        // New next_step commands carry the exact registration for this grant.
        // Older manual completions can use the remembered server registration.
        let client_id = match &args.client_id {
            Some(client_id) => Some(client_id.clone()),
            None => {
                crate::cli::credentials::load_client_id(base).map_err(|error| error.to_string())?
            }
        };
        // Interval unknown here (we didn't request the code), so use the RFC
        // default of 5s and a generous 15-minute deadline.
        let outcome = poll_loop(
            flow,
            device_code,
            client_id.as_deref(),
            5,
            900,
            std::thread::sleep,
        )?;
        return finish(args, base, outcome, json);
    }

    let resp = flow.request_device_code(args.label.as_deref())?;

    // Non-interactive (explicit flag OR no TTY): print JSON and exit without
    // polling — a second `--complete` call finishes the login.
    if args.non_interactive || !stdin_tty {
        let payload = non_interactive_json(&resp, base);
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_default()
        );
        return Ok(());
    }

    // Interactive: show the code prominently, then poll under a spinner.
    let complete = resp
        .verification_uri_complete
        .clone()
        .unwrap_or_else(|| resp.verification_uri.clone());
    use crate::cli::ui;
    ui::intro("lific login");
    ui::note(
        "To sign in, enter this code",
        format!(
            "{}\n\nat {}\nor open directly: {}",
            resp.user_code, resp.verification_uri, complete
        ),
    );

    let spinner = cliclack::spinner();
    spinner.start("Waiting for approval on the other device…");
    let outcome = poll_loop(
        flow,
        &resp.device_code,
        resp.client_id.as_deref(),
        resp.interval.max(1),
        resp.expires_in,
        std::thread::sleep,
    );
    match &outcome {
        Ok(PollOutcome::Approved(_)) => spinner.stop("Approved"),
        Ok(PollOutcome::Denied) => spinner.error("Denied"),
        Ok(PollOutcome::Expired) => spinner.error("Expired"),
        Err(_) => spinner.error("Failed"),
    }
    finish(args, base, outcome?, json)
}

/// Handle a terminal poll outcome: store the token (unless `--no-store`) and
/// print a confirmation, or report denial/expiry as an error.
fn finish(args: &LoginArgs, base: &str, outcome: PollOutcome, json: bool) -> Result<(), String> {
    match outcome {
        PollOutcome::Approved(token) => {
            if args.no_store {
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "status": "approved",
                            "stored": false,
                            "access_token": token,
                        }))
                        .unwrap_or_default()
                    );
                } else {
                    crate::cli::ui::note("Approved. Token (not stored)", &token);
                    crate::cli::ui::outro("Done");
                }
                return Ok(());
            }
            crate::cli::credentials::store(base, &token).map_err(|error| error.to_string())?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "status": "approved",
                        "stored": true,
                        "url": base,
                    }))
                    .unwrap_or_default()
                );
            } else {
                crate::cli::ui::outro(format!("Signed in to {base}. Token stored."));
            }
            Ok(())
        }
        PollOutcome::Denied => Err("login denied by user".to_string()),
        PollOutcome::Expired => {
            Err("login timed out / device code expired — run `lific login` again".to_string())
        }
    }
}

/// `lific logout`: delete the stored credential and best-effort revoke it.
pub fn run_logout(url: Option<&str>, cfg: &Config, json: bool) -> Result<(), String> {
    let base = resolve_base_url(url, cfg);
    // Grab the token first so we can revoke it before deleting.
    let existing = crate::cli::credentials::load(&base).map_err(|error| error.to_string())?;
    if let Some(token) = &existing
        && let Ok(flow) = HttpDeviceFlow::new(&base)
    {
        // Best-effort; ignore revoke failures (server may be down).
        let _ = flow.revoke(token);
    }
    let removed = crate::cli::credentials::delete(&base).map_err(|error| error.to_string())?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "url": base,
                "removed": removed,
            }))
            .unwrap_or_default()
        );
    } else if removed {
        crate::cli::ui::step(format!("Signed out of {base}."));
    } else {
        crate::cli::ui::info(format!("No stored credential for {base}."));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    // ── pure arithmetic ──────────────────────────────────────

    #[test]
    fn backoff_adds_5s_per_slow_down() {
        assert_eq!(poll_backoff(5, 0), 5);
        assert_eq!(poll_backoff(5, 1), 10);
        assert_eq!(poll_backoff(5, 3), 20);
        assert_eq!(poll_backoff(2, 2), 12);
    }

    #[test]
    fn classify_maps_rfc8628_errors() {
        assert_eq!(
            classify_poll_error("authorization_pending"),
            PollSignal::Pending
        );
        assert_eq!(classify_poll_error("slow_down"), PollSignal::SlowDown);
        assert_eq!(
            classify_poll_error("access_denied"),
            PollSignal::Terminal(PollOutcome::Denied)
        );
        assert_eq!(
            classify_poll_error("expired_token"),
            PollSignal::Terminal(PollOutcome::Expired)
        );
        // Unknown → treated as terminal (expired) so the loop stops.
        assert_eq!(
            classify_poll_error("invalid_grant"),
            PollSignal::Terminal(PollOutcome::Expired)
        );
    }

    #[test]
    fn resolve_base_url_precedence() {
        let mut cfg = Config::default();
        cfg.server.port = 3999;
        // Explicit --url wins, trailing slash trimmed.
        assert_eq!(resolve_base_url(Some("http://h:1/"), &cfg), "http://h:1");
        // Else public_url.
        cfg.server.public_url = Some("https://lific.example/".into());
        assert_eq!(resolve_base_url(None, &cfg), "https://lific.example");
        // Else loopback:port.
        cfg.server.public_url = None;
        assert_eq!(resolve_base_url(None, &cfg), "http://127.0.0.1:3999");
    }

    #[test]
    fn non_interactive_json_shape() {
        let resp = DeviceAuthResponse {
            client_id: None,
            device_code: "DEV123".into(),
            user_code: "BCDF-GHJK".into(),
            verification_uri: "http://h/oauth/device".into(),
            verification_uri_complete: Some("http://h/oauth/device?user_code=BCDF-GHJK".into()),
            expires_in: 900,
            interval: 5,
        };
        let v = non_interactive_json(&resp, "http://h");
        assert_eq!(v["user_code"], "BCDF-GHJK");
        assert_eq!(v["device_code"], "DEV123");
        assert_eq!(v["interval"], 5);
        assert_eq!(v["expires_in"], 900);
        assert_eq!(
            v["verification_uri_complete"],
            "http://h/oauth/device?user_code=BCDF-GHJK"
        );
        assert_eq!(
            v["next_step"],
            "lific login --complete DEV123 --url http://h"
        );
        assert!(v.get("client_id").is_none());
        assert!(v.get("next_step_shell").is_none());

        let mut registered = resp;
        registered.client_id = Some("grant-client".into());
        let v = non_interactive_json(&registered, "http://h");
        assert_eq!(v["client_id"], "grant-client");
        assert!(v.get("next_step_shell").is_none());
        assert_eq!(
            v["next_step"],
            "lific login --complete DEV123 --url http://h --client-id=grant-client"
        );

        registered.client_id = Some("client with 'quotes'".into());
        let v = non_interactive_json(&registered, "http://h");
        assert_eq!(v["next_step_shell"], "posix");
        assert_eq!(
            shlex::split(v["next_step"].as_str().unwrap())
                .unwrap()
                .last()
                .unwrap(),
            "--client-id=client with 'quotes'"
        );
    }

    #[test]
    fn completion_command_roundtrips_arbitrary_client_ids() {
        use clap::Parser;

        let mut resp = FakeFlow::new(vec![]).device;
        for client in [
            "",
            "client with 'quotes'",
            "$(echo dangerous); & |",
            "\"double\" \\ slash",
            "line\nnext\tcolumn",
            "client-€-日本語",
            "-leading",
            "client@host",
        ] {
            resp.client_id = Some(client.into());
            let payload = non_interactive_json(&resp, "http://h");
            assert_eq!(payload["client_id"], client);
            let command = payload["next_step"].as_str().unwrap();
            let words = shlex::split(command).unwrap();
            let parsed = super::super::Cli::try_parse_from(words).unwrap();
            let super::super::Command::Login { client_id, .. } = parsed.command else {
                panic!("expected login command");
            };
            assert_eq!(client_id.as_deref(), Some(client));
        }
    }

    #[test]
    fn completion_command_quotes_device_code_and_url_arguments() {
        let mut resp = FakeFlow::new(vec![]).device;
        resp.device_code = "code with $(expansion)".into();
        let url = "https://example.com/path?x=1&y=2";
        let payload = non_interactive_json(&resp, url);
        assert_eq!(payload["next_step_shell"], "posix");
        let words = shlex::split(payload["next_step"].as_str().unwrap()).unwrap();
        assert_eq!(words[3], resp.device_code);
        assert_eq!(words[5], url);
        assert_eq!(words[6], "--client-id=grant-client");
    }

    #[test]
    fn completion_command_rejects_nul_without_losing_structured_arguments() {
        for location in ["client", "code", "url"] {
            let mut resp = FakeFlow::new(vec![]).device;
            let mut url = "http://h";
            match location {
                "client" => resp.client_id = Some("client\0suffix".into()),
                "code" => resp.device_code = "code\0suffix".into(),
                _ => url = "http://h/\0suffix",
            }
            let payload = non_interactive_json(&resp, url);
            assert!(payload["next_step"].is_null());
            assert!(payload["next_step_error"].as_str().unwrap().contains("NUL"));
            assert!(payload.get("next_step_shell").is_none());
            assert_eq!(payload["client_id"], resp.client_id.as_deref().unwrap());
            assert_eq!(payload["device_code"], resp.device_code);
        }
    }

    #[cfg(windows)]
    #[test]
    fn completion_client_id_survives_cmd_parsing() {
        let resp = FakeFlow::new(vec![]).device;
        let next_step = non_interactive_json(&resp, "http://h")["next_step"]
            .as_str()
            .unwrap()
            .to_owned();
        // Echo the generated arguments through the native shell so literal
        // apostrophes cannot hide behind a string-only expectation.
        let command = format!("echo {}", next_step.strip_prefix("lific ").unwrap());
        let output = std::process::Command::new("cmd.exe")
            .args(["/D", "/C", &command])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            "login --complete DEV --url http://h --client-id=grant-client"
        );
    }

    #[test]
    fn completion_command_preserves_a_leading_hyphen_client_id() {
        use clap::Parser;

        let mut resp = FakeFlow::new(vec![]).device;
        resp.client_id = Some("-registered-client".into());
        let payload = non_interactive_json(&resp, "http://h");
        let command = payload["next_step"].as_str().unwrap();
        let parsed = super::super::Cli::try_parse_from(command.split_whitespace()).unwrap();
        let super::super::Command::Login { client_id, .. } = parsed.command else {
            panic!("expected login command");
        };
        assert_eq!(client_id.as_deref(), Some("-registered-client"));
    }

    // ── scripted fake for the polling loop ───────────────────

    /// A fake that returns a scripted sequence of poll signals, one per call.
    struct FakeFlow {
        signals: RefCell<Vec<PollSignal>>,
        polls: RefCell<u32>,
        poll_clients: RefCell<Vec<Option<String>>>,
        device: DeviceAuthResponse,
    }

    impl FakeFlow {
        fn new(signals: Vec<PollSignal>) -> Self {
            Self {
                signals: RefCell::new(signals),
                polls: RefCell::new(0),
                poll_clients: RefCell::new(Vec::new()),
                device: DeviceAuthResponse {
                    client_id: Some("grant-client".into()),
                    device_code: "DEV".into(),
                    user_code: "BCDF-GHJK".into(),
                    verification_uri: "http://h/oauth/device".into(),
                    verification_uri_complete: Some(
                        "http://h/oauth/device?user_code=BCDF-GHJK".into(),
                    ),
                    expires_in: 900,
                    interval: 5,
                },
            }
        }
    }

    impl DeviceFlow for FakeFlow {
        fn request_device_code(&self, _label: Option<&str>) -> Result<DeviceAuthResponse, String> {
            Ok(self.device.clone())
        }
        fn poll_token(
            &self,
            _device_code: &str,
            client_id: Option<&str>,
        ) -> Result<PollSignal, String> {
            *self.polls.borrow_mut() += 1;
            self.poll_clients
                .borrow_mut()
                .push(client_id.map(str::to_string));
            let mut sigs = self.signals.borrow_mut();
            if sigs.is_empty() {
                Ok(PollSignal::Terminal(PollOutcome::Expired))
            } else {
                Ok(sigs.remove(0))
            }
        }
        fn revoke(&self, _token: &str) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn poll_loop_pending_then_approved() {
        let flow = FakeFlow::new(vec![
            PollSignal::Pending,
            PollSignal::Pending,
            PollSignal::Terminal(PollOutcome::Approved("lific_at_xyz".into())),
        ]);
        let sleeps = RefCell::new(0u32);
        let outcome = poll_loop(&flow, "DEV", None, 5, 900, |_| {
            *sleeps.borrow_mut() += 1;
        })
        .unwrap();
        assert_eq!(outcome, PollOutcome::Approved("lific_at_xyz".into()));
        // Two pending signals → two sleeps before the terminal poll.
        assert_eq!(*sleeps.borrow(), 2);
        assert_eq!(*flow.polls.borrow(), 3);
    }

    #[test]
    fn poll_loop_slow_down_increases_backoff() {
        let flow = FakeFlow::new(vec![
            PollSignal::SlowDown,
            PollSignal::SlowDown,
            PollSignal::Terminal(PollOutcome::Approved("t".into())),
        ]);
        let durations = RefCell::new(Vec::<u64>::new());
        let outcome = poll_loop(&flow, "DEV", None, 5, 900, |d| {
            durations.borrow_mut().push(d.as_secs());
        })
        .unwrap();
        assert_eq!(outcome, PollOutcome::Approved("t".into()));
        // First slow_down → +5 (10s), second → +10 (15s).
        assert_eq!(*durations.borrow(), vec![10, 15]);
    }

    #[test]
    fn poll_loop_denied_is_terminal() {
        let flow = FakeFlow::new(vec![
            PollSignal::Pending,
            PollSignal::Terminal(PollOutcome::Denied),
        ]);
        let outcome = poll_loop(&flow, "DEV", None, 5, 900, |_| {}).unwrap();
        assert_eq!(outcome, PollOutcome::Denied);
    }

    #[test]
    fn poll_loop_honors_deadline() {
        // All-pending script with a zero-second deadline → immediate Expired.
        let flow = FakeFlow::new(vec![PollSignal::Pending; 3]);
        let outcome = poll_loop(&flow, "DEV", None, 5, 0, |_| {}).unwrap();
        assert_eq!(outcome, PollOutcome::Expired);
    }

    #[test]
    fn poll_loop_keeps_the_grants_client_across_retries() {
        let flow = FakeFlow::new(vec![
            PollSignal::Pending,
            PollSignal::Terminal(PollOutcome::Approved("token".into())),
        ]);
        poll_loop(&flow, "DEV", Some("grant-client"), 5, 900, |_| {}).unwrap();
        assert_eq!(
            *flow.poll_clients.borrow(),
            vec![Some("grant-client".into()), Some("grant-client".into())]
        );
    }

    #[test]
    fn completion_uses_the_client_from_its_next_step() {
        let flow = FakeFlow::new(vec![PollSignal::Terminal(PollOutcome::Approved(
            "token".into(),
        ))]);
        let args = LoginArgs {
            url: None,
            non_interactive: false,
            complete: Some("DEV".into()),
            client_id: Some("original-client".into()),
            label: None,
            no_store: true,
        };
        run_login_with_flow(&args, "http://h", &flow, true, false).unwrap();
        assert_eq!(
            *flow.poll_clients.borrow(),
            vec![Some("original-client".into())]
        );
    }

    #[tokio::test]
    async fn http_device_poll_sends_the_requested_client_and_preserves_legacy_omission() {
        use axum::{Form, Json, Router, routing::post};
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};

        let captured = Arc::new(Mutex::new(Vec::<HashMap<String, String>>::new()));
        let requests = captured.clone();
        let app = Router::new()
            .route(
                "/oauth/device_authorization",
                post(|| async {
                    Json(serde_json::json!({
                        "device_code": "DEV",
                        "user_code": "BCDF-GHJK",
                        "verification_uri": "http://localhost/oauth/device",
                        "expires_in": 900,
                        "interval": 5,
                        // A server-supplied field cannot override the local client.
                        "client_id": "unrelated-server-value"
                    }))
                }),
            )
            .route(
                "/oauth/token",
                post(move |Form(form): Form<HashMap<String, String>>| {
                    let requests = requests.clone();
                    async move {
                        requests.lock().unwrap().push(form);
                        Json(serde_json::json!({"access_token": "token"}))
                    }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        tokio::task::spawn_blocking(move || {
            let flow = HttpDeviceFlow::new(&base).unwrap();
            for client_id in [Some("original-client"), None] {
                let mut form = vec![("scope", "mcp")];
                if let Some(client_id) = client_id {
                    form.push(("client_id", client_id));
                }
                let grant = flow
                    .device_authorization(&form)
                    .unwrap_or_else(|_| panic!("device authorization failed"));
                assert_eq!(grant.client_id.as_deref(), client_id);
                assert_eq!(
                    flow.poll_token(&grant.device_code, grant.client_id.as_deref())
                        .unwrap(),
                    PollSignal::Terminal(PollOutcome::Approved("token".into()))
                );
            }
        })
        .await
        .unwrap();
        server.abort();
        let requests = captured.lock().unwrap();
        assert_eq!(
            requests[0].get("client_id").map(String::as_str),
            Some("original-client")
        );
        assert_eq!(
            requests[0].get("device_code").map(String::as_str),
            Some("DEV")
        );
        assert!(!requests[1].contains_key("client_id"));
    }

    #[test]
    fn non_interactive_prints_json_and_exits_without_polling() {
        let flow = FakeFlow::new(vec![]); // must NOT be polled
        let args = LoginArgs {
            url: None,
            non_interactive: true,
            complete: None,
            client_id: None,
            label: None,
            no_store: true,
        };
        // stdin_tty=true but --non-interactive set → still non-interactive.
        run_login_with_flow(&args, "http://h", &flow, true, true).unwrap();
        assert_eq!(
            *flow.polls.borrow(),
            0,
            "must not poll in non-interactive mode"
        );
    }

    #[test]
    fn no_tty_forces_non_interactive() {
        let flow = FakeFlow::new(vec![]);
        let args = LoginArgs {
            url: None,
            non_interactive: false,
            complete: None,
            client_id: None,
            label: None,
            no_store: true,
        };
        // stdin_tty=false → non-interactive even without the flag.
        run_login_with_flow(&args, "http://h", &flow, true, false).unwrap();
        assert_eq!(*flow.polls.borrow(), 0);
    }
}
