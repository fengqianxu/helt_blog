use std::net::IpAddr;

use axum::http::HeaderMap;
use hmac::{Hmac, Mac};
use sha2::Sha256;

pub fn address(headers: &HeaderMap) -> String {
    headers
        .get("x-real-ip")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .and_then(|value| value.parse::<IpAddr>().ok())
        .map(|address| address.to_string())
        .unwrap_or_else(|| "unknown".to_owned())
}

pub fn fingerprint(headers: &HeaderMap, secret: &str, scope: &str) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(scope.as_bytes());
    mac.update(&[0]);
    mac.update(address(headers).as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};

    use super::{address, fingerprint};

    #[test]
    fn only_a_canonical_gateway_address_is_accepted() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("198.51.100.9"));
        assert_eq!(address(&headers), "unknown");
        headers.insert("x-real-ip", HeaderValue::from_static("2001:db8::1"));
        assert_eq!(address(&headers), "2001:db8::1");
        headers.insert("x-real-ip", HeaderValue::from_static("not-an-ip"));
        assert_eq!(address(&headers), "unknown");
    }

    #[test]
    fn fingerprints_are_scoped_and_keyed() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", HeaderValue::from_static("203.0.113.7"));
        assert_ne!(
            fingerprint(&headers, "secret-a", "likes"),
            fingerprint(&headers, "secret-a", "visits")
        );
        assert_ne!(
            fingerprint(&headers, "secret-a", "likes"),
            fingerprint(&headers, "secret-b", "likes")
        );
    }
}
