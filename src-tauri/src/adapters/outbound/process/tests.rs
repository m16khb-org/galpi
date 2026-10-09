use super::{
    MAX_LINE_BYTES, ProcessResult, ProcessSpec, handle_stdout, read_bounded_line, run_process,
};
use crate::application::error::AppError;
use crate::application::ports::JobEvents;
use crate::domain::worker::WorkerEvent;
use tokio::io::BufReader;
use uuid::Uuid;

struct IgnoredEvents;

impl JobEvents for IgnoredEvents {
    fn emit(&self, _job_id: Uuid, _event: WorkerEvent) -> Result<(), AppError> {
        Ok(())
    }
}

#[tokio::test]
async fn rejects_oversized_line_before_unbounded_growth() {
    let bytes = vec![b'x'; MAX_LINE_BYTES + 1];
    let mut reader = BufReader::new(bytes.as_slice());
    let mut buffer = Vec::new();
    let result = read_bounded_line(&mut reader, &mut buffer).await;

    assert!(result.is_err());
    assert!(buffer.len() <= MAX_LINE_BYTES);
}

#[test]
fn captures_refined_event_as_process_result() -> Result<(), AppError> {
    // Given
    let mut result = ProcessResult::default();
    let line = r#"{"v":1,"seq":3,"type":"refined","minutes":"/tmp/notes.md"}"#;

    // When
    handle_stdout(&IgnoredEvents, Uuid::nil(), line, true, &mut result)?;

    // Then
    assert_eq!(
        result.completed,
        Some(WorkerEvent::Refined {
            minutes: "/tmp/notes.md".to_owned(),
        })
    );
    Ok(())
}

#[tokio::test]
async fn cancelling_stops_a_running_child_and_reaps_it() {
    // Given: a child that would outlive the test if nobody killed it
    let (sender, mut cancel) = tokio::sync::oneshot::channel();
    let spec = ProcessSpec {
        program: std::path::PathBuf::from("/bin/sleep"),
        current_dir: std::env::temp_dir(),
        args: vec!["30".into()],
        env: std::collections::HashMap::new(),
        worker_protocol: false,
    };

    // When: the job is cancelled while the child is still sleeping
    let job = tokio::spawn(async move {
        run_process(&IgnoredEvents, Uuid::now_v7(), spec, &mut cancel).await
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let _requested = sender.send(());

    // Then: the call returns promptly with CANCELLED rather than after 30s
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(5), job).await;
    let code = match outcome {
        Ok(Ok(Err(error))) => error.code,
        Ok(Ok(Ok(_))) => "COMPLETED".to_owned(),
        Ok(Err(_)) => "TASK_PANICKED".to_owned(),
        Err(_) => "TIMED_OUT".to_owned(),
    };
    assert_eq!(code, "CANCELLED");
}

#[tokio::test]
async fn a_failing_child_reports_its_last_stderr_line() {
    // Given: a child that writes to stderr and exits non-zero
    let (_sender, mut cancel) = tokio::sync::oneshot::channel();
    let spec = ProcessSpec {
        program: std::path::PathBuf::from("/bin/sh"),
        current_dir: std::env::temp_dir(),
        args: vec!["-c".into(), "echo first >&2; echo last >&2; exit 3".into()],
        env: std::collections::HashMap::new(),
        worker_protocol: false,
    };

    // When
    let result = run_process(&IgnoredEvents, Uuid::now_v7(), spec, &mut cancel).await;

    // Then: the tail carries the most recent line, not the first
    let reported = result.err().map(|error| (error.code, error.message));
    assert_eq!(
        reported,
        Some(("PROCESS_FAILED".to_owned(), "last".to_owned()))
    );
}

fn worker_spec(script: &str) -> ProcessSpec {
    ProcessSpec {
        program: std::path::PathBuf::from("/bin/sh"),
        current_dir: std::env::temp_dir(),
        args: vec!["-c".into(), script.into()],
        env: std::collections::HashMap::new(),
        worker_protocol: true,
    }
}

#[tokio::test]
async fn a_listed_worker_error_code_becomes_the_failure() {
    // Given: a worker that reports a stable assistant code, then exits non-zero
    let (_sender, mut cancel) = tokio::sync::oneshot::channel();
    let spec = worker_spec(
        r#"printf '%s\n' '{"v":1,"seq":1,"type":"error","code":"CHATGPT_USAGE_LIMIT_EXCEEDED","message":"사용량 한도에 도달했습니다."}'; exit 1"#,
    );

    // When
    let result = run_process(&IgnoredEvents, Uuid::now_v7(), spec, &mut cancel).await;

    // Then: the caller sees that code and message, not a generic failure
    let reported = result.err().map(|error| (error.code, error.message));
    assert_eq!(
        reported,
        Some((
            "CHATGPT_USAGE_LIMIT_EXCEEDED".to_owned(),
            "사용량 한도에 도달했습니다.".to_owned()
        ))
    );
}

#[tokio::test]
async fn an_unlisted_worker_error_code_stays_a_generic_process_failure() {
    // Given: a worker error whose code is not one of the stable assistant codes
    let (_sender, mut cancel) = tokio::sync::oneshot::channel();
    let spec = worker_spec(
        r#"printf '%s\n' '{"v":1,"seq":1,"type":"error","code":"SOMETHING_ELSE","message":"boom"}'; echo last >&2; exit 1"#,
    );

    // When
    let result = run_process(&IgnoredEvents, Uuid::now_v7(), spec, &mut cancel).await;

    // Then
    let reported = result.err().map(|error| (error.code, error.message));
    assert_eq!(
        reported,
        Some(("PROCESS_FAILED".to_owned(), "last".to_owned()))
    );
}

#[tokio::test]
async fn a_listed_code_does_not_fail_a_worker_that_exits_cleanly() -> Result<(), AppError> {
    // Given: an error line followed by a clean exit
    let (_sender, mut cancel) = tokio::sync::oneshot::channel();
    let spec = worker_spec(
        r#"printf '%s\n' '{"v":1,"seq":1,"type":"error","code":"CHATGPT_USAGE_LIMIT_EXCEEDED","message":"x"}'; exit 0"#,
    );

    // When / Then: only a non-zero exit turns the remembered error into a failure
    run_process(&IgnoredEvents, Uuid::now_v7(), spec, &mut cancel)
        .await
        .map(|_| ())
}
