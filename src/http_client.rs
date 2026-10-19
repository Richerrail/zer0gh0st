use once_cell::sync::Lazy;
use reqwest::header::RETRY_AFTER;
use std::time::Duration;

/// Shared HTTP client with connection pooling, keep-alive, and reasonable timeouts.
pub static HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .pool_max_idle_per_host(10)
        .pool_idle_timeout(Duration::from_secs(90))
        .timeout(Duration::from_secs(180))
        .build()
        .expect("failed to build shared reqwest client")
});

/// Retry a request on 429 (rate limit) with exponential backoff + Retry-After header.
/// Helper disponible pour les appelants externes (pas encore utilisé en interne).
#[allow(dead_code)]
pub async fn send_with_retry(
    mut req_builder: impl FnMut() -> reqwest::RequestBuilder,
    max_retries: u32,
) -> Result<reqwest::Response, reqwest::Error> {
    let mut retries = max_retries;
    loop {
        let req = req_builder();
        let resp = req.send().await?;
        
        if resp.status() == 429 {
            if retries == 0 {
                return Ok(resp);
            }
            // Honor Retry-After header if present, else exponential backoff
            let wait = resp.headers()
                .get(RETRY_AFTER)
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(2u64.pow(max_retries - retries + 1));
            tokio::time::sleep(Duration::from_secs(wait)).await;
            retries -= 1;
            continue;
        }
        
        return Ok(resp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn client_is_shared() {
        // Just verify the lazy static compiles and can be accessed
        let _ = &*HTTP_CLIENT;
    }
}