//! A minimal recording HTTP server on `127.0.0.1:0`.

use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Clone)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub authorization: Option<String>,
    pub body: String,
}

impl Recorded {
    /// A form field of a urlencoded body.
    pub fn field(&self, name: &str) -> Option<String> {
        let url = reqwest::Url::parse(&format!("http://x/?{}", self.body)).ok()?;
        url.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
    }
}

pub type Handler = Arc<dyn Fn(&Recorded) -> (u16, String) + Send + Sync>;

pub struct FakeServer {
    pub base: String,
    requests: Arc<Mutex<Vec<Recorded>>>,
}

impl FakeServer {
    pub async fn start(handler: Handler) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let base = format!("http://{}", listener.local_addr()?);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&requests);
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(serve(stream, Arc::clone(&handler), Arc::clone(&log)));
            }
        });
        Ok(Self { base, requests })
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.requests
            .lock()
            .map(|log| log.clone())
            .unwrap_or_default()
    }

    pub fn count(&self, path: &str) -> usize {
        self.requests()
            .iter()
            .filter(|request| request.path == path)
            .count()
    }
}

async fn serve(mut stream: TcpStream, handler: Handler, log: Arc<Mutex<Vec<Recorded>>>) {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 2048];
    let (head_end, length) = loop {
        let Ok(count) = stream.read(&mut chunk).await else {
            return;
        };
        if count == 0 {
            return;
        }
        buffer.extend_from_slice(&chunk[..count]);
        if let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buffer[..end]).to_lowercase();
            let length = head
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            break (end + 4, length);
        }
    };
    while buffer.len() < head_end + length {
        let Ok(count) = stream.read(&mut chunk).await else {
            return;
        };
        if count == 0 {
            return;
        }
        buffer.extend_from_slice(&chunk[..count]);
    }
    let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
    let mut first = head.lines().next().unwrap_or_default().split(' ');
    let recorded = Recorded {
        method: first.next().unwrap_or_default().to_owned(),
        path: first.next().unwrap_or_default().to_owned(),
        authorization: head.lines().find_map(|line| {
            line.to_lowercase()
                .starts_with("authorization:")
                .then(|| line[14..].trim().to_owned())
        }),
        body: String::from_utf8_lossy(&buffer[head_end..head_end + length]).into_owned(),
    };
    let (status, body) = handler(&recorded);
    if let Ok(mut log) = log.lock() {
        log.push(recorded);
    }
    let response = format!(
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _written = stream.write_all(response.as_bytes()).await;
    let _closed = stream.shutdown().await;
}
