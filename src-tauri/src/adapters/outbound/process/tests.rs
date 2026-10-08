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

#[cfg(unix)]
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

#[cfg(unix)]
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

/// Collects log lines so a test can learn a grandchild's pid.
#[cfg(unix)]
#[derive(Default)]
struct CapturedEvents {
    lines: std::sync::Mutex<Vec<String>>,
}

#[cfg(unix)]
impl JobEvents for CapturedEvents {
    fn emit(&self, _job_id: Uuid, event: WorkerEvent) -> Result<(), AppError> {
        if let WorkerEvent::Log { message, .. } = event
            && let Ok(mut lines) = self.lines.lock()
        {
            lines.push(message);
        }
        Ok(())
    }
}

#[cfg(unix)]
#[tokio::test]
async fn cancelling_removes_grandchild_processes_too() -> Result<(), Box<dyn std::error::Error>> {
    use nix::errno::Errno;
    use nix::sys::signal::kill;
    use nix::unistd::Pid;

    // Given: a shell that starts a background sleep and prints its pid
    let events = std::sync::Arc::new(CapturedEvents::default());
    let (sender, mut cancel) = tokio::sync::oneshot::channel();
    let spec = ProcessSpec {
        program: std::path::PathBuf::from("/bin/sh"),
        current_dir: std::env::temp_dir(),
        args: vec!["-c".into(), "sleep 30 & echo $!; wait".into()],
        env: std::collections::HashMap::new(),
        worker_protocol: false,
    };
    let job_events = events.clone();
    let job = tokio::spawn(async move {
        run_process(job_events.as_ref(), Uuid::now_v7(), spec, &mut cancel).await
    });
    let mut grandchild = None;
    for _ in 0..100 {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let first = events
            .lines
            .lock()
            .ok()
            .and_then(|lines| lines.first().cloned());
        if let Some(Ok(pid)) = first.map(|line| line.trim().parse::<i32>()) {
            grandchild = Some(pid);
            break;
        }
    }
    let pid = grandchild.ok_or("grandchild pid was never reported")?;

    // When: the job is cancelled
    let _requested = sender.send(());
    let _outcome = tokio::time::timeout(std::time::Duration::from_secs(5), job).await?;

    // Then: the grandchild is gone, not merely orphaned
    let mut gone = false;
    for _ in 0..100 {
        if kill(Pid::from_raw(pid), None) == Err(Errno::ESRCH) {
            gone = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(gone, "grandchild {pid} survived cancellation");
    Ok(())
}

#[cfg(windows)]
mod windows_job {
    use super::super::guard::{Escalation, ProcessGroupGuard, configure};
    use tokio::process::Command;

    async fn wait_for_active(guard: &ProcessGroupGuard, wanted: impl Fn(u32) -> bool) -> bool {
        for _ in 0..100 {
            if guard.active_processes().is_ok_and(&wanted) {
                return true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        false
    }

    fn ping_tree() -> Command {
        // cmd.exe starts ping.exe, so the job holds a child and a grandchild.
        let mut command = Command::new("cmd");
        command
            .args(["/c", "ping", "-n", "30", "127.0.0.1"])
            .kill_on_drop(true);
        configure(&mut command);
        command
    }

    #[tokio::test]
    async fn terminating_the_job_ends_the_child_and_the_grandchild()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given: cmd /c ping, supervised
        let mut child = ping_tree().spawn()?;
        let guard = ProcessGroupGuard::attach(&child)?;
        assert!(wait_for_active(&guard, |count| count >= 2).await);

        // When
        guard.terminate(Escalation::Graceful)?;
        let _status = child.wait().await;

        // Then: nothing is left in the job
        assert!(wait_for_active(&guard, |count| count == 0).await);
        Ok(())
    }

    #[tokio::test]
    async fn dropping_an_armed_guard_cleans_up_what_is_left()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given
        let mut child = ping_tree().spawn()?;
        let guard = ProcessGroupGuard::attach(&child)?;
        assert!(wait_for_active(&guard, |count| count >= 2).await);

        // When: the guard is dropped without disarming
        drop(guard);

        // Then: the child exits instead of running its 30 pings
        let status = tokio::time::timeout(std::time::Duration::from_secs(10), child.wait()).await?;
        assert!(!status?.success());
        Ok(())
    }

    #[tokio::test]
    async fn a_disarmed_guard_still_clears_the_job_when_it_closes()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given: a finished supervisor that disarms after a normal exit
        let mut child = ping_tree().spawn()?;
        let mut guard = ProcessGroupGuard::attach(&child)?;
        assert!(wait_for_active(&guard, |count| count >= 2).await);

        // When: it disarms and the job handle closes
        guard.disarm();
        drop(guard);

        // Then: KILL_ON_JOB_CLOSE ends the stragglers well before 30 pings.
        // The kernel uses exit code 0 for that kill, so only the timing proves it.
        let status = tokio::time::timeout(std::time::Duration::from_secs(10), child.wait()).await?;
        status?;
        Ok(())
    }
}

/// Proves the `env_clear` environment is enough for the engine to start: CI
/// installs the CPU lock, then runs
/// `GALPI_TEST_PYTHON=<venv python> cargo test -- --ignored windows_worker_environment_starts_python`.
#[cfg(windows)]
#[tokio::test]
#[ignore = "needs GALPI_TEST_PYTHON pointing at a venv with the Windows lock installed"]
async fn windows_worker_environment_starts_python() -> Result<(), Box<dyn std::error::Error>> {
    use crate::adapters::outbound::paths::AppPaths;
    use crate::adapters::outbound::platform::{Os, worker_environment};

    // Given: the real worker environment builder and an interpreter to start
    let python = std::env::var_os("GALPI_TEST_PYTHON").ok_or("GALPI_TEST_PYTHON is not set")?;
    let root = std::env::temp_dir().join(format!("galpi-env-{}", Uuid::now_v7()));
    let paths = AppPaths::from_roots(Os::Windows, root, std::path::Path::new("documents"));
    let worker_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../worker");
    let env = worker_environment(
        Os::Windows,
        &|key| std::env::var_os(key),
        &paths,
        &worker_root,
    );
    let (_sender, mut cancel) = tokio::sync::oneshot::channel();
    let spec = ProcessSpec {
        program: std::path::PathBuf::from(python),
        current_dir: std::env::temp_dir(),
        args: vec!["-c".into(), "import whisperx, torch, pyannote.audio".into()],
        env,
        worker_protocol: false,
    };

    // When
    let result = run_process(&IgnoredEvents, Uuid::now_v7(), spec, &mut cancel).await;

    // Then: the heavy imports succeed under the cleared environment
    assert!(result.is_ok(), "python failed to start: {:?}", result.err());
    Ok(())
}
