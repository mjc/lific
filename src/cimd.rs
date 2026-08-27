//! Client ID Metadata Document loading for MCP OAuth.
//!
//! The fetcher is deliberately separate from the OAuth handlers. That keeps
//! the untrusted-network policy testable with an injected implementation and
//! makes it difficult for a future authorization path to skip URL validation.

use std::{
    collections::HashMap,
    future::Future,
    net::{IpAddr, SocketAddr},
    pin::Pin,
    sync::Mutex,
    time::{Duration, Instant},
};

use futures_util::StreamExt;
use reqwest::{Url, header};
use serde::Deserialize;

const MAX_DOCUMENT_BYTES: usize = 64 * 1024;
const MAX_CACHE_ENTRIES: usize = 128;
const MAX_REDIRECTS: usize = 3;
const MAX_CACHE_TTL: Duration = Duration::from_secs(60 * 60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClientMetadata {
    pub client_id: String,
    pub client_name: String,
    pub redirect_uris: Vec<String>,
}

pub(crate) type FetchFuture = Pin<Box<dyn Future<Output = Result<ClientMetadata, String>> + Send>>;

/// Injectable boundary for CIMD retrieval. Tests can return a document without
/// opening a socket; production uses [`HttpFetcher`], which performs all
/// network validation before accepting a document.
pub(crate) trait Fetcher: Send + Sync {
    fn fetch(&self, issuer: &str, client_id: &str) -> FetchFuture;
}

#[derive(Clone)]
pub(crate) struct HttpFetcher {
    cache: std::sync::Arc<Mutex<HashMap<String, CachedDocument>>>,
}

#[derive(Clone)]
struct CachedDocument {
    metadata: ClientMetadata,
    expires_at: Instant,
    etag: Option<String>,
    last_modified: Option<String>,
}

impl HttpFetcher {
    pub(crate) fn new() -> Self {
        Self {
            cache: std::sync::Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn fetch_document(
        &self,
        issuer: &str,
        client_id: &str,
    ) -> Result<ClientMetadata, String> {
        let original_url = validate_client_id_url(client_id)?;
        let cache_key = format!("{issuer}\0{client_id}");
        let cached = self
            .cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&cache_key)
            .cloned();
        if let Some(entry) = cached
            .as_ref()
            .filter(|entry| entry.expires_at > Instant::now())
        {
            return Ok(entry.metadata.clone());
        }

        let mut url = original_url;
        for redirect in 0..=MAX_REDIRECTS {
            let resolved = resolve_public_host(&url).await?;
            let client = reqwest::Client::builder()
                .connect_timeout(CONNECT_TIMEOUT)
                .timeout(REQUEST_TIMEOUT)
                .redirect(reqwest::redirect::Policy::none())
                .resolve(url.host_str().ok_or("CIMD URL has no host")?, resolved)
                .build()
                .map_err(|_| "CIMD HTTP client could not be configured".to_string())?;

            let mut request = client.get(url.clone());
            if redirect == 0 {
                if let Some(etag) = cached.as_ref().and_then(|entry| entry.etag.as_deref()) {
                    request = request.header(header::IF_NONE_MATCH, etag);
                }
                if let Some(last_modified) = cached
                    .as_ref()
                    .and_then(|entry| entry.last_modified.as_deref())
                {
                    request = request.header(header::IF_MODIFIED_SINCE, last_modified);
                }
            }

            let response = request
                .send()
                .await
                .map_err(|_| "CIMD metadata could not be fetched".to_string())?;
            if response.status().is_redirection() {
                if redirect == MAX_REDIRECTS {
                    return Err("CIMD metadata redirected too many times".into());
                }
                let location = response
                    .headers()
                    .get(header::LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .ok_or("CIMD redirect did not include a valid Location")?;
                url = url
                    .join(location)
                    .map_err(|_| "CIMD redirect URL is invalid".to_string())?;
                validate_metadata_url(&url)?;
                continue;
            }

            if response.status() == reqwest::StatusCode::NOT_MODIFIED {
                let Some(entry) = cached.as_ref() else {
                    return Err("CIMD returned 304 without a cached document".into());
                };
                let ttl = cache_ttl(response.headers());
                if ttl.is_zero() {
                    return Err("CIMD cache revalidation did not provide a usable lifetime".into());
                }
                self.store(
                    cache_key,
                    CachedDocument {
                        metadata: entry.metadata.clone(),
                        expires_at: Instant::now() + ttl,
                        etag: header_value(response.headers(), header::ETAG)
                            .or_else(|| entry.etag.clone()),
                        last_modified: header_value(response.headers(), header::LAST_MODIFIED)
                            .or_else(|| entry.last_modified.clone()),
                    },
                );
                return Ok(entry.metadata.clone());
            }

            if response.status() != reqwest::StatusCode::OK {
                return Err("CIMD metadata returned an unexpected status".into());
            }
            let response_headers = response.headers().clone();
            let content_type = response_headers
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default();
            if !is_json_content_type(content_type) {
                return Err("CIMD metadata did not have a JSON content type".into());
            }
            if response
                .content_length()
                .is_some_and(|length| length > MAX_DOCUMENT_BYTES as u64)
            {
                return Err("CIMD metadata is too large".into());
            }
            let mut body = Vec::new();
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|_| "CIMD metadata could not be read".to_string())?;
                if body.len().saturating_add(chunk.len()) > MAX_DOCUMENT_BYTES {
                    return Err("CIMD metadata is too large".into());
                }
                body.extend_from_slice(&chunk);
            }
            let metadata = parse_metadata(&body, client_id)?;
            let ttl = cache_ttl(&response_headers);
            if !ttl.is_zero() {
                self.store(
                    cache_key,
                    CachedDocument {
                        metadata: metadata.clone(),
                        expires_at: Instant::now() + ttl,
                        etag: header_value(&response_headers, header::ETAG),
                        last_modified: header_value(&response_headers, header::LAST_MODIFIED),
                    },
                );
            }
            return Ok(metadata);
        }
        Err("CIMD metadata redirected too many times".into())
    }

    fn store(&self, key: String, value: CachedDocument) {
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if cache.len() >= MAX_CACHE_ENTRIES
            && !cache.contains_key(&key)
            && let Some(oldest) = cache.keys().next().cloned()
        {
            cache.remove(&oldest);
        }
        cache.insert(key, value);
    }
}

impl Fetcher for HttpFetcher {
    fn fetch(&self, issuer: &str, client_id: &str) -> FetchFuture {
        let this = self.clone();
        let issuer = issuer.to_string();
        let client_id = client_id.to_string();
        Box::pin(async move { this.fetch_document(&issuer, &client_id).await })
    }
}

#[derive(Deserialize)]
struct RawMetadata {
    client_id: String,
    client_name: String,
    redirect_uris: Vec<String>,
}

#[cfg(test)]
fn is_client_id_url(client_id: &str) -> bool {
    validate_client_id_url(client_id).is_ok()
}

pub(crate) fn is_client_id_candidate(client_id: &str) -> bool {
    Url::parse(client_id)
        .ok()
        .is_some_and(|url| url.scheme().eq_ignore_ascii_case("https"))
}

fn validate_client_id_url(client_id: &str) -> Result<Url, String> {
    let url = Url::parse(client_id).map_err(|_| "CIMD client_id is not a valid URL".to_string())?;
    validate_metadata_url(&url)?;
    Ok(url)
}

fn validate_metadata_url(url: &Url) -> Result<(), String> {
    if url.scheme() != "https"
        || url.host_str().is_none()
        || url.path().is_empty()
        || url.path() == "/"
        || url.username() != ""
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "CIMD client_id must be an HTTPS URL with a path and no credentials or fragment".into(),
        );
    }
    Ok(())
}

fn parse_metadata(body: &[u8], client_id: &str) -> Result<ClientMetadata, String> {
    let raw: RawMetadata =
        serde_json::from_slice(body).map_err(|_| "CIMD metadata is not valid JSON".to_string())?;
    if raw.client_id != client_id
        || raw.client_name.trim().is_empty()
        || raw.redirect_uris.is_empty()
    {
        return Err("CIMD metadata is missing required or matching fields".into());
    }
    for redirect_uri in &raw.redirect_uris {
        crate::oauth::validate_redirect_uri(redirect_uri)
            .map_err(|_| "CIMD metadata contains an invalid redirect URI".to_string())?;
    }
    Ok(ClientMetadata {
        client_id: raw.client_id,
        client_name: raw.client_name,
        redirect_uris: raw.redirect_uris,
    })
}

fn is_json_content_type(content_type: &str) -> bool {
    let media_type = content_type.split(';').next().unwrap_or_default().trim();
    media_type.eq_ignore_ascii_case("application/json") || media_type.ends_with("+json")
}

fn cache_ttl(headers: &reqwest::header::HeaderMap) -> Duration {
    let Some(cache_control) = headers
        .get(header::CACHE_CONTROL)
        .and_then(|v| v.to_str().ok())
    else {
        return Duration::ZERO;
    };
    for directive in cache_control.split(',').map(str::trim) {
        if directive.eq_ignore_ascii_case("no-store") || directive.eq_ignore_ascii_case("no-cache")
        {
            return Duration::ZERO;
        }
        if let Some((name, value)) = directive.split_once('=')
            && name.eq_ignore_ascii_case("max-age")
            && let Ok(seconds) = value.trim_matches('"').parse::<u64>()
        {
            return Duration::from_secs(seconds).min(MAX_CACHE_TTL);
        }
    }
    Duration::ZERO
}

fn header_value(headers: &reqwest::header::HeaderMap, name: header::HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

async fn resolve_public_host(url: &Url) -> Result<SocketAddr, String> {
    let host = url.host_str().ok_or("CIMD URL has no host")?;
    let port = url.port_or_known_default().ok_or("CIMD URL has no port")?;
    let addresses: Vec<_> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| "CIMD host could not be resolved".to_string())?
        .collect();
    if !addresses_are_public(&addresses) {
        return Err("CIMD host did not resolve only to public addresses".into());
    }
    Ok(addresses[0])
}

fn addresses_are_public(addresses: &[SocketAddr]) -> bool {
    !addresses.is_empty()
        && addresses
            .iter()
            .all(|address| is_public_address(address.ip()))
}

fn is_public_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && !ip.is_broadcast()
        }
        IpAddr::V6(ip) => {
            if ip.is_loopback()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.is_unspecified()
                || ip.is_multicast()
            {
                return false;
            }
            if let Some(v4) = ip.to_ipv4() {
                return is_public_address(IpAddr::V4(v4));
            }
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cimd_urls_require_https_and_a_path() {
        assert!(is_client_id_url("https://client.example/metadata.json"));
        assert!(!is_client_id_url("https://client.example"));
        assert!(!is_client_id_url("http://client.example/metadata.json"));
        assert!(!is_client_id_url(
            "https://client.example/metadata.json#fragment"
        ));
        assert!(!is_client_id_url(
            "https://user:pass@client.example/metadata.json"
        ));
    }

    #[test]
    fn metadata_requires_exact_client_id_name_and_redirects() {
        let document = serde_json::json!({
            "client_id": "https://client.example/metadata.json",
            "client_name": "Example",
            "redirect_uris": ["http://127.0.0.1/callback"]
        });
        let metadata = parse_metadata(
            &serde_json::to_vec(&document).unwrap(),
            "https://client.example/metadata.json",
        )
        .unwrap();
        assert_eq!(metadata.client_name, "Example");
        assert!(parse_metadata(b"{}", "https://client.example/metadata.json").is_err());
    }

    #[test]
    fn cache_lifetime_is_bounded_and_fail_closed_without_cache_control() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(header::CACHE_CONTROL, "max-age=999999".parse().unwrap());
        assert_eq!(cache_ttl(&headers), MAX_CACHE_TTL);
        headers.insert(header::CACHE_CONTROL, "Max-Age=30".parse().unwrap());
        assert_eq!(cache_ttl(&headers), Duration::from_secs(30));
        headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
        assert_eq!(cache_ttl(&headers), Duration::ZERO);
        headers.remove(header::CACHE_CONTROL);
        assert_eq!(cache_ttl(&headers), Duration::ZERO);
    }

    #[test]
    fn private_and_special_addresses_are_not_public() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.1.1",
            "::1",
            "fc00::1",
            "fe80::1",
        ] {
            assert!(
                !is_public_address(ip.parse().unwrap()),
                "{ip} must be rejected"
            );
        }
        assert!(is_public_address("8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn mixed_dns_answers_fail_closed() {
        let mixed = [
            SocketAddr::from(([8, 8, 8, 8], 443)),
            SocketAddr::from(([127, 0, 0, 1], 443)),
        ];
        assert!(!addresses_are_public(&mixed));
        assert!(addresses_are_public(&[SocketAddr::from((
            [8, 8, 8, 8],
            443
        ))]));
    }
}
