use super::{Callback, Loopback};
use crate::application::error::AppError;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::timeout;

const LONG: Duration = Duration::from_secs(30);

type Waiting = (
    u16,
    oneshot::Sender<()>,
    JoinHandle<Result<Callback, AppError>>,
);

async fn start(limit: Duration) -> Result<Waiting, AppError> {
    let loopback = Loopback::bind_preferring(0).await?;
    let port = loopback.port;
    let (cancel, mut receiver) = oneshot::channel();
    let task = tokio::spawn(async move { loopback.wait(&mut receiver, limit).await });
    Ok((port, cancel, task))
}

async fn exchange(port: u16, request: &str) -> Result<String, std::io::Error> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await?;
    stream.write_all(request.as_bytes()).await?;
    let mut response = Vec::new();
    timeout(Duration::from_secs(10), stream.read_to_end(&mut response))
        .await
        .map_err(|_| std::io::Error::other("no response"))??;
    Ok(String::from_utf8_lossy(&response).into_owned())
}

fn get(port: u16, target: &str) -> String {
    format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n")
}

fn join_error(error: &tokio::task::JoinError) -> AppError {
    AppError::new("TEST_JOIN", error.to_string())
}

#[tokio::test]
async fn an_occupied_preferred_port_falls_back_to_an_assigned_one() -> Result<(), AppError> {
    let held = TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|error| AppError::io("bind", &error))?;
    let taken = held
        .local_addr()
        .map_err(|error| AppError::io("addr", &error))?
        .port();

    let loopback = Loopback::bind_preferring(taken).await?;

    assert_ne!(loopback.port, taken);
    assert_eq!(
        loopback.redirect_uri(),
        format!("http://127.0.0.1:{}/auth/callback", loopback.port)
    );
    Ok(())
}

#[tokio::test]
async fn the_callback_is_parsed_and_answered_with_a_no_store_page() -> Result<(), AppError> {
    let (port, _cancel, task) = start(LONG).await?;

    let response = exchange(
        port,
        &get(port, "/auth/callback?code=c%2B1&state=s&client_id=oaiapp_x"),
    )
    .await
    .map_err(|error| AppError::io("exchange", &error))?;
    let callback = task.await.map_err(|error| join_error(&error))??;

    assert!(response.starts_with("HTTP/1.1 200 OK"));
    assert!(response.contains("Cache-Control: no-store"));
    assert!(response.contains("Connection: close"));
    assert!(response.contains("돌아가 주세요"));
    assert_eq!(callback.get("code"), Some("c+1"));
    assert_eq!(callback.get("state"), Some("s"));
    assert_eq!(callback.get("client_id"), Some("oaiapp_x"));
    assert_eq!(callback.get("error"), None);
    Ok(())
}

#[tokio::test]
async fn other_paths_and_methods_are_refused_and_the_wait_continues() -> Result<(), AppError> {
    let (port, _cancel, task) = start(LONG).await?;
    let wrong_method =
        format!("POST /auth/callback?state=s HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n");

    let favicon = exchange(port, &get(port, "/favicon.ico")).await;
    let posted = exchange(port, &wrong_method).await;
    let stateless = exchange(port, &get(port, "/auth/callback?code=x")).await;
    let accepted = exchange(port, &get(port, "/auth/callback?state=s&code=ok")).await;

    let io = |error| AppError::io("exchange", &error);
    assert!(favicon.map_err(io)?.starts_with("HTTP/1.1 404"));
    assert!(posted.map_err(io)?.starts_with("HTTP/1.1 405"));
    assert!(stateless.map_err(io)?.starts_with("HTTP/1.1 400"));
    accepted.map_err(io)?;
    let callback = task.await.map_err(|error| join_error(&error))??;
    assert_eq!(callback.get("code"), Some("ok"));
    Ok(())
}

#[tokio::test]
async fn a_foreign_host_header_is_refused_and_the_wait_continues() -> Result<(), AppError> {
    let (port, _cancel, task) = start(LONG).await?;

    let rebound = exchange(
        port,
        "GET /auth/callback?state=s&code=evil HTTP/1.1\r\nHost: attacker.example\r\n\r\n",
    )
    .await
    .map_err(|error| AppError::io("exchange", &error))?;
    exchange(port, &get(port, "/auth/callback?state=s&code=ok"))
        .await
        .map_err(|error| AppError::io("exchange", &error))?;

    assert!(rebound.starts_with("HTTP/1.1 400"));
    let callback = task.await.map_err(|error| join_error(&error))??;
    assert_eq!(callback.get("code"), Some("ok"));
    Ok(())
}

#[tokio::test]
async fn an_oversized_head_is_refused_and_the_wait_continues() -> Result<(), AppError> {
    let (port, _cancel, task) = start(LONG).await?;
    let padding = "a".repeat(9 * 1024);

    let refused = exchange(
        port,
        &format!("GET /auth/callback?state=s HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-Pad: {padding}\r\n\r\n"),
    )
    .await
    .map_err(|error| AppError::io("exchange", &error))?;
    exchange(port, &get(port, "/auth/callback?state=s&code=ok"))
        .await
        .map_err(|error| AppError::io("exchange", &error))?;

    assert!(refused.starts_with("HTTP/1.1 431"));
    task.await.map_err(|error| join_error(&error))??;
    Ok(())
}

#[tokio::test]
async fn a_silent_connection_neither_blocks_nor_outlives_the_read_limit() -> Result<(), AppError> {
    let mut loopback = Loopback::bind_preferring(0).await?;
    loopback.read_timeout = Duration::from_millis(50);
    let port = loopback.port;
    let (_cancel, mut receiver) = oneshot::channel();
    let task = tokio::spawn(async move { loopback.wait(&mut receiver, LONG).await });
    let mut silent = TcpStream::connect(("127.0.0.1", port))
        .await
        .map_err(|error| AppError::io("connect", &error))?;

    exchange(port, &get(port, "/auth/callback?state=s&code=ok"))
        .await
        .map_err(|error| AppError::io("exchange", &error))?;

    task.await.map_err(|error| join_error(&error))??;
    let mut rest = Vec::new();
    let closed = timeout(Duration::from_secs(10), silent.read_to_end(&mut rest)).await;
    assert!(matches!(closed, Ok(Ok(0))));
    Ok(())
}

#[tokio::test]
async fn cancelling_ends_the_wait_with_cancelled() -> Result<(), AppError> {
    let (_port, cancel, task) = start(LONG).await?;

    let _sent = cancel.send(());

    let error = task.await.map_err(|error| join_error(&error))?.err();
    assert_eq!(error.map(|error| error.code), Some("CANCELLED".to_owned()));
    Ok(())
}

#[tokio::test]
async fn an_elapsed_deadline_ends_the_wait_with_a_timeout() -> Result<(), AppError> {
    let (_port, _cancel, task) = start(Duration::from_millis(30)).await?;

    let error = task.await.map_err(|error| join_error(&error))?.err();

    assert_eq!(
        error.map(|error| error.code),
        Some("CHATGPT_SIGN_IN_TIMEOUT".to_owned())
    );
    Ok(())
}
