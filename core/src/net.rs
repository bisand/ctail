//! The one HTTP shape the engine needs: a blocking request with a deadline
//! whose answer is a status and a body, whatever the status was.
//!
//! Callers read the body of a failed request too — an API's error message is
//! in there — so a 4xx/5xx is an answer here, not an error. Only not getting
//! an answer at all (no route, a refused connection, the deadline) is one.

use std::time::Duration;

/// What came back.
pub(crate) struct Reply {
    pub status: u16,
    pub body: String,
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .http_status_as_error(false)
            .user_agent(concat!("ctail/", env!("CARGO_PKG_VERSION")))
            .build(),
    )
}

fn read(result: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<Reply, String> {
    let mut response = result.map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    Ok(Reply { status, body })
}

pub(crate) fn get(url: &str, headers: &[(&str, &str)], timeout: Duration) -> Result<Reply, String> {
    let mut request = agent(timeout).get(url);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    read(request.call())
}

pub(crate) fn post_json(
    url: &str,
    headers: &[(&str, &str)],
    json: &str,
    timeout: Duration,
) -> Result<Reply, String> {
    let mut request = agent(timeout)
        .post(url)
        .header("Content-Type", "application/json");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    read(request.send(json))
}

#[cfg(test)]
pub(crate) mod testing {
    //! A one-shot HTTP server on a loopback port, for tests that want the
    //! whole path exercised without a network.

    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Serves `body` with `status` to the first request, and hands that
    /// request's text (start line, headers and body) back through `seen`.
    pub(crate) fn serve_once(
        status: u16,
        body: &'static str,
    ) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener");
        let url = format!("http://{}/", listener.local_addr().expect("addr"));
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut request = Vec::new();
            let mut buf = [0u8; 4096];
            // Read until the headers are complete, then whatever body the
            // Content-Length promises.
            let body_len = loop {
                let n = stream.read(&mut buf).expect("read");
                request.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&request);
                if let Some(end) = text.find("\r\n\r\n") {
                    let len = text[..end]
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                        })
                        .unwrap_or(0);
                    if request.len() >= end + 4 + len {
                        break len;
                    }
                }
                if n == 0 {
                    break 0;
                }
            };
            let _ = body_len;
            let response = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).expect("write");
            let _ = stream.flush();
            String::from_utf8_lossy(&request).into_owned()
        });
        (url, handle)
    }
}
