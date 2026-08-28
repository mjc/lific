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
    sync::{Arc, Mutex},
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

type TransportFuture = Pin<Box<dyn Future<Output = Result<TransportResponse, String>> + Send>>;

trait Transport: Send + Sync {
    fn get(&self, request: TransportRequest) -> TransportFuture;
}

#[derive(Clone)]
struct TransportRequest {
    url: Url,
    etag: Option<String>,
    last_modified: Option<String>,
}

struct TransportResponse {
    status: reqwest::StatusCode,
    headers: reqwest::header::HeaderMap,
    body: Vec<u8>,
}

#[derive(Clone, Copy)]
struct ReqwestTransport;

#[derive(Clone)]
pub(crate) struct HttpFetcher {
    transport: Arc<dyn Transport>,
    cache: Arc<Mutex<HashMap<String, CachedDocument>>>,
    inflight: Arc<tokio::sync::Semaphore>,
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
        Self::with_transport(Arc::new(ReqwestTransport))
    }

    fn with_transport(transport: Arc<dyn Transport>) -> Self {
        Self {
            transport,
            cache: Arc::new(Mutex::new(HashMap::new())),
            inflight: Arc::new(tokio::sync::Semaphore::new(16)),
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

        let _permit = self
            .inflight
            .acquire()
            .await
            .map_err(|_| "CIMD metadata lookup is unavailable".to_string())?;

        // Another request may have populated the cache while this lookup was
        // waiting for a network slot. Recheck before opening a connection.
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
            let (etag, last_modified) = if redirect == 0 {
                (
                    cached.as_ref().and_then(|entry| entry.etag.clone()),
                    cached
                        .as_ref()
                        .and_then(|entry| entry.last_modified.clone()),
                )
            } else {
                (None, None)
            };
            let is_conditional = etag.is_some() || last_modified.is_some();
            let response = self
                .transport
                .get(TransportRequest {
                    url: url.clone(),
                    etag,
                    last_modified,
                })
                .await?;
            if response.status == reqwest::StatusCode::NOT_MODIFIED {
                if redirect != 0 || !is_conditional {
                    return Err("CIMD returned 304 without a matching conditional request".into());
                }
                let Some(entry) = cached.as_ref() else {
                    return Err("CIMD returned 304 without a cached document".into());
                };
                let ttl = cache_ttl(&response.headers);
                self.store(
                    cache_key,
                    CachedDocument {
                        metadata: entry.metadata.clone(),
                        expires_at: Instant::now() + ttl,
                        etag: header_value(&response.headers, header::ETAG)
                            .or_else(|| entry.etag.clone()),
                        last_modified: header_value(&response.headers, header::LAST_MODIFIED)
                            .or_else(|| entry.last_modified.clone()),
                    },
                );
                return Ok(entry.metadata.clone());
            }
            if response.status.is_redirection() {
                if redirect == MAX_REDIRECTS {
                    return Err("CIMD metadata redirected too many times".into());
                }
                let location = response
                    .headers
                    .get(header::LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .ok_or("CIMD redirect did not include a valid Location")?;
                url = url
                    .join(location)
                    .map_err(|_| "CIMD redirect URL is invalid".to_string())?;
                validate_metadata_url(&url)?;
                continue;
            }

            if response.status != reqwest::StatusCode::OK {
                return Err("CIMD metadata returned an unexpected status".into());
            }
            let content_type = response
                .headers
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default();
            if !is_json_content_type(content_type) {
                return Err("CIMD metadata did not have a JSON content type".into());
            }
            if response.body.len() > MAX_DOCUMENT_BYTES {
                return Err("CIMD metadata is too large".into());
            }
            let metadata = parse_metadata(&response.body, client_id)?;
            let ttl = cache_ttl(&response.headers);
            if !ttl.is_zero() {
                self.store(
                    cache_key,
                    CachedDocument {
                        metadata: metadata.clone(),
                        expires_at: Instant::now() + ttl,
                        etag: header_value(&response.headers, header::ETAG),
                        last_modified: header_value(&response.headers, header::LAST_MODIFIED),
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

impl Transport for ReqwestTransport {
    fn get(&self, request: TransportRequest) -> TransportFuture {
        Box::pin(async move {
            let resolved = resolve_public_host(&request.url).await?;
            let client = reqwest::Client::builder()
                .connect_timeout(CONNECT_TIMEOUT)
                .timeout(REQUEST_TIMEOUT)
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .resolve(
                    request.url.host_str().ok_or("CIMD URL has no host")?,
                    resolved,
                )
                .build()
                .map_err(|_| "CIMD HTTP client could not be configured".to_string())?;

            let mut outgoing = client.get(request.url);
            if let Some(etag) = request.etag {
                outgoing = outgoing.header(header::IF_NONE_MATCH, etag);
            }
            if let Some(last_modified) = request.last_modified {
                outgoing = outgoing.header(header::IF_MODIFIED_SINCE, last_modified);
            }

            let response = outgoing
                .send()
                .await
                .map_err(|_| "CIMD metadata could not be fetched".to_string())?;
            if response
                .content_length()
                .is_some_and(|length| length > MAX_DOCUMENT_BYTES as u64)
            {
                return Err("CIMD metadata is too large".into());
            }
            let status = response.status();
            let headers = response.headers().clone();
            let mut body = Vec::new();
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|_| "CIMD metadata could not be read".to_string())?;
                if body.len().saturating_add(chunk.len()) > MAX_DOCUMENT_BYTES {
                    return Err("CIMD metadata is too large".into());
                }
                body.extend_from_slice(&chunk);
            }
            Ok(TransportResponse {
                status,
                headers,
                body,
            })
        })
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
    if contains_dot_segment(client_id) {
        return Err("CIMD client_id must not contain dot segments".into());
    }
    let url = Url::parse(client_id).map_err(|_| "CIMD client_id is not a valid URL".to_string())?;
    validate_metadata_url(&url)?;
    Ok(url)
}

fn contains_dot_segment(url: &str) -> bool {
    let Some(scheme_end) = url.find("://") else {
        return false;
    };
    let authority = &url[scheme_end + 3..];
    let Some(path_offset) = authority.find('/') else {
        return false;
    };
    let path_start = scheme_end + 3 + path_offset;
    let path = &url[path_start..]
        [..url[path_start..].find(['?', '#']).unwrap_or(url.len() - path_start)];
    path.split('/').any(|segment| {
        let mut decoded = Vec::with_capacity(segment.len());
        let bytes = segment.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'%' && index + 2 < bytes.len()
                && let (Some(high), Some(low)) = (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                decoded.push(high << 4 | low);
                index += 3;
            } else {
                decoded.push(bytes[index]);
                index += 1;
            }
        }
        decoded == b"." || decoded == b".."
    })
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
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
    media_type.eq_ignore_ascii_case("application/json")
        || media_type.to_ascii_lowercase().ends_with("+json")
}

fn cache_ttl(headers: &reqwest::header::HeaderMap) -> Duration {
    let Some(cache_control) = headers
        .get(header::CACHE_CONTROL)
        .and_then(|v| v.to_str().ok())
    else {
        return Duration::ZERO;
    };
    let directives: Vec<_> = cache_control.split(',').map(str::trim).collect();
    if directives.iter().any(|directive| {
        directive.eq_ignore_ascii_case("no-store") || directive.eq_ignore_ascii_case("no-cache")
    }) {
        return Duration::ZERO;
    }
    for directive in directives {
        if let Some((name, value)) = directive.split_once('=')
            && name.eq_ignore_ascii_case("max-age")
            && let Ok(seconds) = value.trim().trim_matches('"').parse::<u64>()
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
        IpAddr::V4(ip) => is_public_ipv4(ip),
        IpAddr::V6(ip) => is_public_ipv6(ip),
    }
}

fn is_public_ipv4(ip: std::net::Ipv4Addr) -> bool {
    let value = u32::from(ip);
    ![
        (0x0000_0000, 0xff00_0000), // 0.0.0.0/8
        (0x0a00_0000, 0xff00_0000), // 10.0.0.0/8
        (0x6440_0000, 0xffc0_0000), // 100.64.0.0/10, shared address space
        (0x7f00_0000, 0xff00_0000), // 127.0.0.0/8
        (0xa9fe_0000, 0xffff_0000), // 169.254.0.0/16
        (0xac10_0000, 0xfff0_0000), // 172.16.0.0/12
        (0xc000_0000, 0xffffff00),  // 192.0.0.0/24
        (0xc000_0200, 0xffffff00),  // 192.0.2.0/24
        (0xc058_6300, 0xffffff00),  // 192.88.99.0/24
        (0xc0a8_0000, 0xffff_0000), // 192.168.0.0/16
        (0xc612_0000, 0xfffe_0000), // 198.18.0.0/15
        (0xc633_6400, 0xffffff00),  // 198.51.100.0/24
        (0xcb00_7100, 0xffffff00),  // 203.0.113.0/24
        (0xe000_0000, 0xf000_0000), // 224.0.0.0/4, multicast
        (0xf000_0000, 0xf000_0000), // 240.0.0.0/4, reserved
    ]
    .into_iter()
    .any(|(network, mask)| value & mask == network)
}

fn is_public_ipv6(ip: std::net::Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4() {
        return is_public_ipv4(v4);
    }
    if ip.is_unique_local()
        || ip.is_unicast_link_local()
        || ip.is_unspecified()
        || ip.is_multicast()
    {
        return false;
    }
    [
        (std::net::Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0), 128),
        (std::net::Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1), 128),
        (std::net::Ipv6Addr::new(0x2001, 2, 0, 0, 0, 0, 0, 0), 48),
        (std::net::Ipv6Addr::new(0x2001, 0x10, 0, 0, 0, 0, 0, 0), 28),
        (std::net::Ipv6Addr::new(0x2001, 0x20, 0, 0, 0, 0, 0, 0), 28),
        (std::net::Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0), 32),
        (std::net::Ipv6Addr::new(0x3fff, 0, 0, 0, 0, 0, 0, 0), 20),
        (std::net::Ipv6Addr::new(0x5f00, 0, 0, 0, 0, 0, 0, 0), 16),
    ]
    .into_iter()
    .all(|(network, prefix)| !ipv6_in_prefix(ip, network, prefix))
}

fn ipv6_in_prefix(ip: std::net::Ipv6Addr, network: std::net::Ipv6Addr, prefix: u8) -> bool {
    let mask = if prefix == 0 {
        0
    } else {
        u128::MAX << (128 - prefix)
    };
    u128::from(ip) & mask == u128::from(network) & mask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct ScriptedTransport {
        responses:
            std::sync::Arc<Mutex<std::collections::VecDeque<Result<TransportResponse, String>>>>,
        requests: std::sync::Arc<Mutex<Vec<TransportRequest>>>,
    }

    impl ScriptedTransport {
        fn new(responses: impl IntoIterator<Item = TransportResponse>) -> Self {
            Self::with_results(responses.into_iter().map(Ok))
        }

        fn with_results(
            responses: impl IntoIterator<Item = Result<TransportResponse, String>>,
        ) -> Self {
            Self {
                responses: std::sync::Arc::new(Mutex::new(responses.into_iter().collect())),
                requests: std::sync::Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl Transport for ScriptedTransport {
        fn get(&self, request: TransportRequest) -> TransportFuture {
            self.requests.lock().unwrap().push(request);
            let response = self
                .responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Err("unexpected request".to_string()));
            Box::pin(async move { response })
        }
    }

    fn response(
        status: reqwest::StatusCode,
        headers: &[(&'static str, &'static str)],
        body: &[u8],
    ) -> TransportResponse {
        let mut response_headers = reqwest::header::HeaderMap::new();
        for (name, value) in headers {
            response_headers.insert(*name, reqwest::header::HeaderValue::from_static(value));
        }
        TransportResponse {
            status,
            headers: response_headers,
            body: body.to_vec(),
        }
    }

    fn metadata_body(client_id: &str, client_name: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "client_id": client_id,
            "client_name": client_name,
            "redirect_uris": ["http://127.0.0.1/callback"]
        }))
        .unwrap()
    }

    fn store_stale(fetcher: &HttpFetcher, issuer: &str, client_id: &str) {
        fetcher.store(
            format!("{issuer}\0{client_id}"),
            CachedDocument {
                metadata: ClientMetadata {
                    client_id: client_id.into(),
                    client_name: "Stale".into(),
                    redirect_uris: vec!["http://127.0.0.1/callback".into()],
                },
                expires_at: Instant::now() - Duration::from_secs(1),
                etag: Some("stale".into()),
                last_modified: Some("Wed, 21 Oct 2015 07:28:00 GMT".into()),
            },
        );
    }

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
        assert!(!is_client_id_url("https://client.example/./metadata.json"));
        assert!(!is_client_id_url("https://client.example/a/../metadata.json"));
        assert!(!is_client_id_url("https://client.example/%2e%2e/metadata.json"));
        assert!(!is_client_id_url("https://client.example/%2E./metadata.json"));
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
    fn cimd_redirects_allow_local_http_but_not_remote_http() {
        let base = serde_json::json!({
            "client_id": "https://client.example/metadata.json",
            "client_name": "Example"
        });
        for redirect_uri in [
            "http://localhost/callback",
            "http://127.0.0.1:4312/callback",
            "http://[::1]/callback",
            "https://app.example/callback",
        ] {
            let mut document = base.clone();
            document["redirect_uris"] = serde_json::json!([redirect_uri]);
            assert!(
                parse_metadata(
                    &serde_json::to_vec(&document).unwrap(),
                    "https://client.example/metadata.json"
                )
                .is_ok(),
                "redirect should be accepted: {redirect_uri}"
            );
        }
        let mut document = base;
        document["redirect_uris"] = serde_json::json!(["http://app.example/callback"]);
        assert!(
            parse_metadata(
                &serde_json::to_vec(&document).unwrap(),
                "https://client.example/metadata.json"
            )
            .is_err()
        );
    }

    #[test]
    fn cache_lifetime_is_bounded_and_fail_closed_without_cache_control() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(header::CACHE_CONTROL, "max-age=999999".parse().unwrap());
        assert_eq!(cache_ttl(&headers), MAX_CACHE_TTL);
        headers.insert(header::CACHE_CONTROL, "Max-Age=30".parse().unwrap());
        assert_eq!(cache_ttl(&headers), Duration::from_secs(30));
        headers.insert(
            header::CACHE_CONTROL,
            "max-age=30, no-cache".parse().unwrap(),
        );
        assert_eq!(cache_ttl(&headers), Duration::ZERO);
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
    fn non_global_special_addresses_are_not_public() {
        for ip in [
            "0.0.0.1",
            "100.64.0.1",
            "192.0.0.1",
            "192.0.2.1",
            "198.18.0.1",
            "198.51.100.1",
            "203.0.113.1",
            "2001:db8::1",
        ] {
            assert!(
                !is_public_address(ip.parse().unwrap()),
                "{ip} must be rejected"
            );
        }
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

    #[tokio::test]
    async fn redirected_revalidation_cannot_revive_stale_metadata() {
        let client_id = "https://client.example/metadata.json";
        let transport = ScriptedTransport::new([
            response(
                reqwest::StatusCode::FOUND,
                &[("location", "https://redirect.example/metadata.json")],
                b"",
            ),
            response(
                reqwest::StatusCode::NOT_MODIFIED,
                &[("cache-control", "max-age=60")],
                b"",
            ),
        ]);
        let fetcher = HttpFetcher::with_transport(std::sync::Arc::new(transport));
        store_stale(&fetcher, "https://issuer.example", client_id);

        let error = fetcher
            .fetch_document("https://issuer.example", client_id)
            .await
            .expect_err("a redirect target cannot validate another URL's cached document");
        assert!(error.contains("304"), "unexpected error: {error}");
    }

    #[tokio::test]
    async fn direct_304_revalidates_cached_metadata_and_validators() {
        let issuer = "https://issuer.example";
        let client_id = "https://client.example/metadata.json";
        let transport = ScriptedTransport::new([response(
            reqwest::StatusCode::NOT_MODIFIED,
            &[("cache-control", "max-age=60"), ("etag", "fresh")],
            b"",
        )]);
        let requests = transport.requests.clone();
        let fetcher = HttpFetcher::with_transport(Arc::new(transport));
        store_stale(&fetcher, issuer, client_id);

        let metadata = fetcher.fetch_document(issuer, client_id).await.unwrap();
        assert_eq!(metadata.client_name, "Stale");
        assert_eq!(
            fetcher
                .fetch_document(issuer, client_id)
                .await
                .unwrap()
                .client_name,
            "Stale",
            "the refreshed cache should avoid another transport request"
        );
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].etag.as_deref(), Some("stale"));
        assert_eq!(
            requests[0].last_modified.as_deref(),
            Some("Wed, 21 Oct 2015 07:28:00 GMT")
        );
    }

    #[tokio::test]
    async fn direct_304_without_lifetime_returns_cached_metadata_once() {
        let issuer = "https://issuer.example";
        let client_id = "https://client.example/metadata.json";
        let transport = ScriptedTransport::new([response(
            reqwest::StatusCode::NOT_MODIFIED,
            &[],
            b"",
        )]);
        let fetcher = HttpFetcher::with_transport(Arc::new(transport));
        store_stale(&fetcher, issuer, client_id);

        let metadata = fetcher.fetch_document(issuer, client_id).await.unwrap();
        assert_eq!(metadata.client_name, "Stale");
        let cached = fetcher
            .cache
            .lock()
            .unwrap()
            .get(&format!("{issuer}\0{client_id}"))
            .cloned()
            .unwrap();
        assert!(cached.expires_at <= Instant::now());
    }

    #[tokio::test]
    async fn unsolicited_304_without_cache_validators_is_rejected() {
        let issuer = "https://issuer.example";
        let client_id = "https://client.example/metadata.json";
        let transport = ScriptedTransport::new([response(
            reqwest::StatusCode::NOT_MODIFIED,
            &[("cache-control", "max-age=60")],
            b"",
        )]);
        let fetcher = HttpFetcher::with_transport(Arc::new(transport));
        store_stale(&fetcher, issuer, client_id);
        {
            let mut cache = fetcher.cache.lock().unwrap();
            let entry = cache.get_mut(&format!("{issuer}\0{client_id}")).unwrap();
            entry.etag = None;
            entry.last_modified = None;
        }

        let error = fetcher
            .fetch_document(issuer, client_id)
            .await
            .expect_err("304 is only valid after a conditional request");
        assert!(error.contains("304"), "unexpected error: {error}");
    }

    #[tokio::test]
    async fn changed_metadata_replaces_an_expired_cached_document() {
        let issuer = "https://issuer.example";
        let client_id = "https://client.example/metadata.json";
        let body = metadata_body(client_id, "Fresh");
        let transport = ScriptedTransport::new([response(
            reqwest::StatusCode::OK,
            &[
                ("content-type", "application/json"),
                ("cache-control", "max-age=60"),
                ("etag", "fresh"),
            ],
            &body,
        )]);
        let fetcher = HttpFetcher::with_transport(Arc::new(transport));
        store_stale(&fetcher, issuer, client_id);

        let metadata = fetcher.fetch_document(issuer, client_id).await.unwrap();
        assert_eq!(metadata.client_name, "Fresh");
        assert_eq!(
            fetcher
                .fetch_document(issuer, client_id)
                .await
                .unwrap()
                .client_name,
            "Fresh"
        );
    }

    #[tokio::test]
    async fn removed_metadata_fails_closed_instead_of_serving_stale_cache() {
        let issuer = "https://issuer.example";
        let client_id = "https://client.example/metadata.json";
        let transport =
            ScriptedTransport::new([response(reqwest::StatusCode::NOT_FOUND, &[], b"")]);
        let fetcher = HttpFetcher::with_transport(Arc::new(transport));
        store_stale(&fetcher, issuer, client_id);

        let error = fetcher
            .fetch_document(issuer, client_id)
            .await
            .expect_err("removed metadata must not fall back to stale cache");
        assert!(
            error.contains("unexpected status"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn transport_failure_during_revalidation_fails_closed() {
        let issuer = "https://issuer.example";
        let client_id = "https://client.example/metadata.json";
        let transport =
            ScriptedTransport::with_results([
                Err("CIMD metadata could not be fetched".to_string()),
            ]);
        let fetcher = HttpFetcher::with_transport(Arc::new(transport));
        store_stale(&fetcher, issuer, client_id);

        let error = fetcher
            .fetch_document(issuer, client_id)
            .await
            .expect_err("a timeout must not revive stale metadata");
        assert_eq!(error, "CIMD metadata could not be fetched");
    }

    #[tokio::test]
    async fn oversized_streamed_metadata_is_rejected_without_network_access() {
        let client_id = "https://client.example/metadata.json";
        let transport = ScriptedTransport::new([response(
            reqwest::StatusCode::OK,
            &[("content-type", "application/json")],
            &vec![b' '; MAX_DOCUMENT_BYTES + 1],
        )]);
        let fetcher = HttpFetcher::with_transport(Arc::new(transport));

        let error = fetcher
            .fetch_document("https://issuer.example", client_id)
            .await
            .expect_err("oversized metadata must be rejected");
        assert!(error.contains("too large"), "unexpected error: {error}");
    }

    #[tokio::test]
    async fn every_redirect_reenters_the_transport_and_drops_original_validators() {
        let issuer = "https://issuer.example";
        let client_id = "https://client.example/metadata.json";
        let transport = ScriptedTransport::new([
            response(
                reqwest::StatusCode::FOUND,
                &[("location", "https://one.example/metadata.json")],
                b"",
            ),
            response(
                reqwest::StatusCode::FOUND,
                &[("location", "https://two.example/metadata.json")],
                b"",
            ),
            response(
                reqwest::StatusCode::FOUND,
                &[("location", "https://three.example/metadata.json")],
                b"",
            ),
            response(
                reqwest::StatusCode::FOUND,
                &[("location", "https://four.example/metadata.json")],
                b"",
            ),
        ]);
        let requests = transport.requests.clone();
        let fetcher = HttpFetcher::with_transport(Arc::new(transport));
        store_stale(&fetcher, issuer, client_id);

        let error = fetcher
            .fetch_document(issuer, client_id)
            .await
            .expect_err("redirect chains are bounded");
        assert!(
            error.contains("too many times"),
            "unexpected error: {error}"
        );
        let requests = requests.lock().unwrap();
        assert_eq!(
            requests
                .iter()
                .map(|request| request.url.as_str())
                .collect::<Vec<_>>(),
            [
                "https://client.example/metadata.json",
                "https://one.example/metadata.json",
                "https://two.example/metadata.json",
                "https://three.example/metadata.json",
            ]
        );
        assert_eq!(requests[0].etag.as_deref(), Some("stale"));
        assert!(
            requests[1..]
                .iter()
                .all(|request| request.etag.is_none() && request.last_modified.is_none())
        );
    }

    #[tokio::test]
    async fn dns_policy_failure_after_a_redirect_is_propagated() {
        let client_id = "https://client.example/metadata.json";
        let transport = ScriptedTransport::with_results([
            Ok(response(
                reqwest::StatusCode::FOUND,
                &[("location", "https://rebound.example/metadata.json")],
                b"",
            )),
            Err("CIMD host did not resolve only to public addresses".to_string()),
        ]);
        let requests = transport.requests.clone();
        let fetcher = HttpFetcher::with_transport(Arc::new(transport));

        let error = fetcher
            .fetch_document("https://issuer.example", client_id)
            .await
            .expect_err("every redirect target must pass the transport's DNS policy");
        assert!(
            error.contains("public addresses"),
            "unexpected error: {error}"
        );
        assert_eq!(requests.lock().unwrap().len(), 2);
    }
}
