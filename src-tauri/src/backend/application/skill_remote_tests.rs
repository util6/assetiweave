use super::*;

#[test]
fn skill_remote_uses_reqwest_not_ureq() {
    let source = include_str!("skill_remote.rs");
    assert!(!source.contains(concat!("ur", "eq::")));
}

#[test]
fn skill_remote_github_get_json_loopback() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0u8; 4096];
        let _ = stream.read(&mut request);
        let body = r#"{"name":"test-skill","tag_name":"v1.0.0"}"#;
        let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
        let _ = stream.write_all(response.as_bytes());
    });
    let result = github_get_json(&format!("http://{address}/repos/test"), "test context").unwrap();
    assert_eq!(result["name"], "test-skill");
    assert_eq!(result["tag_name"], "v1.0.0");
    server.join().unwrap();
}
