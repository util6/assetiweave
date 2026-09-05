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
                return Err(AppError::Canceled("download cancelled".into()));
            }
            let read =
                std::io::Read::read(&mut response, &mut buffer).map_err(AppError::external)?;
            if cancelled() {
                return Err(AppError::Canceled("download cancelled".into()));
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
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;

    #[tokio::test]
    async fn client_can_be_built_used_and_dropped_in_blocking_worker() {
        tokio::task::spawn_blocking(|| {
            let client = build_http_client().unwrap();
            let second = client.clone();
            drop(second);
            drop(client);
        })
        .await
        .unwrap();
    }

    #[test]
    fn shared_http_client_returns_cloned_instance() {
        let client1 = shared_http_client().unwrap();
        let client2 = shared_http_client().unwrap();
        // Client clone shares connection pool
        assert_eq!(
            std::mem::size_of_val(&client1),
            std::mem::size_of_val(&client2)
        );
    }

    #[test]
    fn get_with_redirects_handles_200_and_gzip() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut reader = BufReader::new(&mut stream);
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap() > 0 {
                    if line == "\r\n" || line == "\n" {
                        break;
                    }
                    line.clear();
                }

                let body = b"hello compressed world";
                let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
                encoder.write_all(body).unwrap();
                let compressed = encoder.finish().unwrap();

                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    compressed.len()
                );
                stream.write_all(resp.as_bytes()).unwrap();
                stream.write_all(&compressed).unwrap();
                stream.flush().unwrap();
            }
        });

        let client = build_http_client().unwrap();
        let url = format!("http://{addr}/data");
        let resp = get_with_redirects(
            &client,
            &url,
            reqwest::header::HeaderMap::new(),
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let text = resp.text().unwrap();
        assert_eq!(text, "hello compressed world");

        handle.join().unwrap();
    }

    #[test]
    fn get_with_redirects_returns_304_as_is() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut reader = BufReader::new(&mut stream);
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap() > 0 {
                    if line == "\r\n" || line == "\n" {
                        break;
                    }
                    line.clear();
                }
                let resp =
                    "HTTP/1.1 304 Not Modified\r\nETag: \"12345\"\r\nConnection: close\r\n\r\n";
                stream.write_all(resp.as_bytes()).unwrap();
                stream.flush().unwrap();
            }
        });

        let client = build_http_client().unwrap();
        let url = format!("http://{addr}/resource");
        let resp = get_with_redirects(
            &client,
            &url,
            reqwest::header::HeaderMap::new(),
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::NOT_MODIFIED);
        assert_eq!(
            resp.headers().get(reqwest::header::ETAG).unwrap(),
            "\"12345\""
        );

        handle.join().unwrap();
    }

    #[test]
    fn get_with_redirects_follows_relative_redirect_and_strips_authorization() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let received_auth_on_hop2 = Arc::new(AtomicUsize::new(0));
        let flag = received_auth_on_hop2.clone();

        let handle = thread::spawn(move || {
            // First hop: 302 Found redirect to /target
            let (mut stream1, _) = listener.accept().unwrap();
            let mut reader1 = BufReader::new(&mut stream1);
            let mut line = String::new();
            while reader1.read_line(&mut line).unwrap() > 0 {
                if line == "\r\n" || line == "\n" {
                    break;
                }
                line.clear();
            }
            let resp1 = "HTTP/1.1 302 Found\r\nLocation: /target\r\nConnection: close\r\n\r\n";
            stream1.write_all(resp1.as_bytes()).unwrap();
            stream1.flush().unwrap();

            // Second hop: target
            let (mut stream2, _) = listener.accept().unwrap();
            let mut reader2 = BufReader::new(&mut stream2);
            line.clear();
            let mut has_auth = false;
            while reader2.read_line(&mut line).unwrap() > 0 {
                if line.to_ascii_lowercase().starts_with("authorization:") {
                    has_auth = true;
                }
                if line == "\r\n" || line == "\n" {
                    break;
                }
                line.clear();
            }
            if has_auth {
                flag.store(1, Ordering::SeqCst);
            } else {
                flag.store(2, Ordering::SeqCst);
            }
            let resp2 = "HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nsuccess";
            stream2.write_all(resp2.as_bytes()).unwrap();
            stream2.flush().unwrap();
        });

        let client = build_http_client().unwrap();
        let url = format!("http://{addr}/initial");
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_static("Bearer secret-token"),
        );
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/json"),
        );

        let resp = get_with_redirects(&client, &url, headers, Duration::from_secs(5)).unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        assert_eq!(resp.text().unwrap(), "success");
        assert_eq!(
            received_auth_on_hop2.load(Ordering::SeqCst),
            2,
            "authorization header must be stripped upon redirect"
        );

        handle.join().unwrap();
    }

    #[test]
    fn get_with_redirects_fails_after_exceeding_5_redirects() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = thread::spawn(move || {
            // Serve 6 redirects
            for i in 1..=6 {
                if let Ok((mut stream, _)) = listener.accept() {
                    let mut reader = BufReader::new(&mut stream);
                    let mut line = String::new();
                    while reader.read_line(&mut line).unwrap() > 0 {
                        if line == "\r\n" || line == "\n" {
                            break;
                        }
                        line.clear();
                    }
                    let resp = format!(
                        "HTTP/1.1 302 Found\r\nLocation: /step{}\r\nConnection: close\r\n\r\n",
                        i + 1
                    );
                    let _ = stream.write_all(resp.as_bytes());
                    let _ = stream.flush();
                }
            }
        });

        let client = build_http_client().unwrap();
        let url = format!("http://{addr}/step1");
        let res = get_with_redirects(
            &client,
            &url,
            reqwest::header::HeaderMap::new(),
            Duration::from_secs(5),
        );
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(
            err.to_string().contains("too many redirects"),
            "error was: {err}"
        );

        handle.join().unwrap();
    }

    #[test]
    fn get_with_redirects_fails_on_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = thread::spawn(move || {
            if let Ok((_stream, _)) = listener.accept() {
                // Sleep longer than client timeout
                thread::sleep(Duration::from_millis(500));
            }
        });

        let client = build_http_client().unwrap();
        let url = format!("http://{addr}/slow");
        let res = get_with_redirects(
            &client,
            &url,
            reqwest::header::HeaderMap::new(),
            Duration::from_millis(50),
        );
        assert!(res.is_err());

        handle.join().unwrap();
    }

    #[test]
    fn download_to_file_loopback_and_cleanup_on_error() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let body = b"1234";
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.write_all(body);
                let _ = stream.flush();
            }
        });

        let client = build_http_client().unwrap();
        let temp_dir =
            std::env::temp_dir().join(format!("assetiweave-dl-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let part_path = temp_dir.join("test.part");

        let spec = DownloadSpec {
            url: &format!("http://{addr}/test"),
            path: &part_path,
            max_bytes: 3,
            expected_size: None,
            timeout: Duration::from_secs(5),
        };
        let res = download_to_file(&client, spec, &|| false);
        assert!(res.is_err());
        assert!(
            !part_path.exists(),
            "partial file must be cleaned up on error"
        );
        let _ = std::fs::remove_dir_all(&temp_dir);

        handle.join().unwrap();
    }

    #[test]
    fn artifact_production_paths_no_longer_use_ureq_or_buffered_extract() {
        for source in [
            include_str!("agent_market/lifecycle/install.rs"),
            include_str!("application/conversation_adapter_installer.rs"),
        ] {
            assert!(!source.contains(concat!("ur", "eq::")));
        }
        let source = include_str!("agent_market/installers/binary.rs");
        assert!(source.contains("materialize_file"));
    }

    #[test]
    fn read_response_text_with_limit_enforces_size_limit() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let body = "abcdefghijklmnopqrstuvwxyz";
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
            }
        });

        let client = build_http_client().unwrap();
        let resp = client
            .get(format!("http://{addr}/oversize"))
            .send()
            .unwrap();
        let err = read_response_text_with_limit(resp, 10).unwrap_err();
        assert!(
            err.to_string().contains("exceeds maximum limit")
                || err.to_string().contains("exceeds"),
            "Expected size error, got: {err}"
        );
        handle.join().unwrap();
    }

    #[test]
    fn read_response_text_with_limit_reads_valid_content() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let body = "hello world text";
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
            }
        });

        let client = build_http_client().unwrap();
        let resp = client.get(format!("http://{addr}/valid")).send().unwrap();
        let text = read_response_text_with_limit(resp, 1024).unwrap();
        assert_eq!(text, "hello world text");
        handle.join().unwrap();
    }
}
