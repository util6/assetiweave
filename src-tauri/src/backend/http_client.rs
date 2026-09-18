use crate::backend::runtime::{AppError, AppResult};
use std::sync::Mutex;
use std::time::{Duration, Instant};

static HTTP_CLIENT: Mutex<Option<reqwest::blocking::Client>> = Mutex::new(None);

pub(crate) fn shared_http_client() -> AppResult<reqwest::blocking::Client> {
    let mut guard = HTTP_CLIENT
        .lock()
        .map_err(|_| AppError::external("failed to acquire shared http client lock"))?;
    if let Some(client) = guard.as_ref() {
        return Ok(client.clone());
    }
    let client = build_http_client()?;
    *guard = Some(client.clone());
    Ok(client)
}

fn build_http_client() -> AppResult<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(None)
        .user_agent(concat!("AssetIWeave/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(AppError::external)
}

pub(crate) fn get_with_redirects(
    client: &reqwest::blocking::Client,
    url: &str,
    headers: reqwest::header::HeaderMap,
    timeout: Duration,
) -> AppResult<reqwest::blocking::Response> {
    let deadline = Instant::now() + timeout;
    let mut current_url = url::Url::parse(url).map_err(AppError::external)?;
    let mut current_headers = headers;
    let mut redirects = 0;
    const MAX_REDIRECTS: usize = 5;

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(AppError::Timeout("HTTP request timed out".to_string()));
        }

        let response = client
            .get(current_url.as_str())
            .headers(current_headers.clone())
            .timeout(remaining)
            .send()
            .map_err(AppError::external)?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(response);
        }

        match status {
            reqwest::StatusCode::MOVED_PERMANENTLY
            | reqwest::StatusCode::FOUND
            | reqwest::StatusCode::SEE_OTHER
            | reqwest::StatusCode::TEMPORARY_REDIRECT
            | reqwest::StatusCode::PERMANENT_REDIRECT => {
                redirects += 1;
                if redirects > MAX_REDIRECTS {
                    return Err(AppError::external("too many redirects (exceeded 5)"));
                }
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .ok_or_else(|| AppError::external("redirect response missing Location header"))?
                    .to_str()
                    .map_err(AppError::external)?;
                current_url = current_url.join(location).map_err(AppError::external)?;
                current_headers.remove(reqwest::header::AUTHORIZATION);
            }
            _ => return Ok(response),
        }
    }
}

pub(crate) const DEFAULT_MAX_TEXT_RESPONSE_BYTES: u64 = 10 * 1024 * 1024;

pub(crate) fn read_response_text_with_limit(
    mut response: reqwest::blocking::Response,
    max_bytes: u64,
) -> AppResult<String> {
    use std::io::Read;
    if let Some(content_length) = response.content_length() {
        if content_length > max_bytes {
            return Err(AppError::External(format!(
                "HTTP response body size ({content_length} bytes) exceeds maximum limit of {max_bytes} bytes"
            )));
        }
    }
    let mut buffer = Vec::new();
    let mut reader = (&mut response).take(max_bytes + 1);
    reader
        .read_to_end(&mut buffer)
        .map_err(AppError::external)?;
    if buffer.len() > max_bytes as usize {
        return Err(AppError::External(format!(
            "HTTP response body exceeded maximum limit of {max_bytes} bytes"
        )));
    }
    String::from_utf8(buffer)
        .map_err(|error| AppError::External(format!("HTTP response was not valid text: {error}")))
}

pub(crate) struct DownloadSpec<'a> {
    pub(crate) url: &'a str,
    pub(crate) path: &'a std::path::Path,
    pub(crate) max_bytes: u64,
    pub(crate) expected_size: Option<u64>,
    pub(crate) timeout: Duration,
}

pub(crate) fn download_to_file(
    client: &reqwest::blocking::Client,
    spec: DownloadSpec<'_>,
    cancelled: &dyn Fn() -> bool,
) -> AppResult<u64> {
    if let Some(expected) = spec.expected_size {
        if expected > spec.max_bytes {
            return Err(AppError::Validation("artifact_size_invalid".into()));
        }
    }

    let download_closure = || -> AppResult<u64> {
        let mut response = get_with_redirects(
            client,
            spec.url,
            reqwest::header::HeaderMap::new(),
            spec.timeout,
        )?
        .error_for_status()
        .map_err(AppError::external)?;

        let mut file =
            std::fs::File::create(spec.path).map_err(|e| AppError::Storage(e.to_string()))?;
        let mut count = 0u64;
        let mut buffer = [0u8; 8192];
        loop {
            if cancelled() {
                return Err(AppError::Cancelled("download cancelled".into()));
            }
            let read =
                std::io::Read::read(&mut response, &mut buffer).map_err(AppError::external)?;
            if cancelled() {
                return Err(AppError::Cancelled("download cancelled".into()));
            }
            if read == 0 {
                break;
            }
            count += read as u64;
            if count > spec.max_bytes {
                return Err(AppError::Validation("artifact_size_invalid".into()));
            }
            std::io::Write::write_all(&mut file, &buffer[..read])
                .map_err(|e| AppError::Storage(e.to_string()))?;
        }

        if let Some(expected) = spec.expected_size {
            if count != expected {
                return Err(AppError::Validation("artifact_size_invalid".into()));
            }
        }
        std::io::Write::flush(&mut file).map_err(|e| AppError::Storage(e.to_string()))?;
        Ok(count)
    };

    let result = download_closure();
    if result.is_err() {
        let _ = std::fs::remove_file(spec.path);
    }
    result
}

#[cfg(test)]
#[path = "http_client_tests.rs"]
mod tests;
