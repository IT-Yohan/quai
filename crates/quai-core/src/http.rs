use anyhow::Result;
use std::time::Duration;

/// Timeout for API calls and header range requests. Full downloads use their own.
pub const API_TIMEOUT: Duration = Duration::from_secs(30);

/// Shared HTTP client: system certificate store, `HTTPS_PROXY` honoured, redirects followed.
pub fn client() -> Result<reqwest::Client> {
    let ua = format!("quai/{} (+https://github.com/quai-rpm/quai)", crate::VERSION);
    Ok(reqwest::Client::builder()
        .user_agent(ua)
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?)
}
