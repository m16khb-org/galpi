//! The loopback redirect listener for the browser sign-in.
//!
//! Only `GET /auth/callback` with a `state` is accepted. Every other request is
//! answered and ignored, so a speculative browser connection, a favicon fetch,
//! or a silent socket never ends or stalls the wait.

use crate::application::error::AppError;
use reqwest::Url;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tokio::time::{sleep, timeout};

/// The port the official documentation uses as its example.
pub const PREFERRED_PORT: u16 = 1455;
/// How long a sign-in may wait for the browser.
pub const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const CALLBACK_PATH: &str = "/auth/callback";
const MAX_HEAD_BYTES: usize = 8 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(3);
const DONE_PAGE: &str = "<!doctype html><html lang=\"ko\"><head><meta charset=\"utf-8\">\
<title>Galpi</title></head><body style=\"font-family:-apple-system,sans-serif;\
text-align:center;margin-top:20vh\"><h1>로그인 응답을 받았습니다</h1>\
<p>이 창을 닫고 Galpi로 돌아가 주세요.</p></body></html>";

/// The query of an accepted callback. Holds the one-time code, so it has no `Debug`.
pub struct Callback(Vec<(String, String)>);

impl Callback {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

pub struct Loopback {
    listener: TcpListener,
    port: u16,
    read_timeout: Duration,
}

impl Loopback {
    /// Bind `127.0.0.1:port`, or an OS-assigned port when it is taken.
    pub async fn bind_preferring(port: u16) -> Result<Self, AppError> {
        let listener = match TcpListener::bind(("127.0.0.1", port)).await {
            Ok(listener) => listener,
            Err(_) => TcpListener::bind(("127.0.0.1", 0)).await.map_err(|error| {
                AppError::io("로그인 응답을 받을 로컬 포트를 열지 못했습니다", &error)
            })?,
        };
        let port = listener
            .local_addr()
            .map_err(|error| AppError::io("로컬 포트를 확인하지 못했습니다", &error))?
            .port();
        Ok(Self {
            listener,
            port,
            read_timeout: READ_TIMEOUT,
        })
    }

    pub fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}{CALLBACK_PATH}", self.port)
    }

    /// Wait for the callback, the user's cancellation, or `limit`.
    pub async fn wait(
        self,
        cancel: &mut oneshot::Receiver<()>,
        limit: Duration,
    ) -> Result<Callback, AppError> {
        let mut connections = JoinSet::new();
        let deadline = sleep(limit);
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                _ = &mut *cancel => {
                    return Err(AppError::new("CANCELLED", "사용자가 작업을 취소했습니다."));
                }
                () = &mut deadline => {
                    return Err(AppError::new(
                        "CHATGPT_SIGN_IN_TIMEOUT",
                        "브라우저에서 로그인이 완료되지 않아 대기를 마쳤습니다. 다시 시도해 주세요.",
                    ));
                }
                accepted = self.listener.accept() => {
                    let (stream, _) = accepted.map_err(|error| {
                        AppError::io("로그인 응답 연결을 받지 못했습니다", &error)
                    })?;
                    connections.spawn(serve(stream, self.port, self.read_timeout));
                }
                Some(finished) = connections.join_next(), if !connections.is_empty() => {
                    if let Ok(Some(callback)) = finished {
                        return Ok(callback);
                    }
                }
            }
        }
    }
}

enum Head {
    Complete(String),
    TooLarge,
    Unreadable,
}

async fn read_head(stream: &mut TcpStream) -> Head {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 1024];
    loop {
        if let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            buffer.truncate(end);
            return String::from_utf8(buffer).map_or(Head::Unreadable, Head::Complete);
        }
        if buffer.len() > MAX_HEAD_BYTES {
            return Head::TooLarge;
        }
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => return Head::Unreadable,
            Ok(count) => buffer.extend_from_slice(&chunk[..count]),
        }
    }
}

async fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\n\
Content-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\n\
Connection: close\r\n\r\n{body}",
        body.len()
    );
    // The peer may already be gone; there is nobody left to tell.
    let _written = stream.write_all(response.as_bytes()).await;
    let _closed = stream.shutdown().await;
}

/// Read and drop what the peer still sends (bounded), so closing does not
/// reset the connection before the peer has read our refusal.
async fn discard_rest(stream: &mut TcpStream, limit: Duration) {
    let mut sink = [0_u8; 4096];
    let mut remaining = 64 * 1024_usize;
    let _ended = timeout(limit, async {
        while remaining > 0 {
            match stream.read(&mut sink).await {
                Ok(0) | Err(_) => break,
                Ok(count) => remaining = remaining.saturating_sub(count),
            }
        }
    })
    .await;
}

/// Handle one connection; `Some` only for an accepted callback.
async fn serve(mut stream: TcpStream, port: u16, read_timeout: Duration) -> Option<Callback> {
    let head = match timeout(read_timeout, read_head(&mut stream)).await {
        Ok(Head::Complete(head)) => head,
        Ok(Head::TooLarge) => {
            respond(&mut stream, "431 Request Header Fields Too Large", "").await;
            discard_rest(&mut stream, read_timeout).await;
            return None;
        }
        Ok(Head::Unreadable) | Err(_) => return None,
    };
    let mut lines = head.split("\r\n");
    let mut request_line = lines.next().unwrap_or_default().split(' ');
    let (method, target) = (request_line.next(), request_line.next());
    let host = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("host"))
        .map(|(_, value)| value.trim());
    let (Some(method), Some(target)) = (method, target) else {
        respond(&mut stream, "400 Bad Request", "").await;
        return None;
    };
    if host != Some(format!("127.0.0.1:{port}").as_str()) {
        respond(&mut stream, "400 Bad Request", "").await;
        return None;
    }
    let Ok(url) = Url::parse(&format!("http://127.0.0.1{target}")) else {
        respond(&mut stream, "400 Bad Request", "").await;
        return None;
    };
    if url.path() != CALLBACK_PATH {
        respond(&mut stream, "404 Not Found", "").await;
        return None;
    }
    if method != "GET" {
        respond(&mut stream, "405 Method Not Allowed", "").await;
        return None;
    }
    let pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();
    if !pairs.iter().any(|(key, _)| key == "state") {
        respond(&mut stream, "400 Bad Request", "").await;
        return None;
    }
    respond(&mut stream, "200 OK", DONE_PAGE).await;
    Some(Callback(pairs))
}

#[cfg(test)]
mod tests;
