export interface RecordingViewState {
  readonly status: "idle" | "starting" | "recording" | "stopping" | "failed" | "completed"
  readonly recordingId: string | null
  readonly path: string | null
  readonly startedAtMs: number | null
  readonly elapsedSeconds: number
  readonly message: string
  readonly warning: boolean
}

export const initialRecordingState: RecordingViewState = {
  status: "idle",
  recordingId: null,
  path: null,
  startedAtMs: null,
  elapsedSeconds: 0,
  message: "선택한 소스로 바로 녹음할 수 있습니다.",
  warning: false,
}

export function beginRecordingState(): RecordingViewState {
  return {
    ...initialRecordingState,
    status: "starting",
    message: "녹음 권한과 장치를 확인합니다.",
  }
}

export function canStartRecording(state: RecordingViewState): boolean {
  return state.status === "idle" || state.status === "completed" || state.status === "failed"
}

export function startRecordingState(
  recordingId: string,
  path: string,
  startedAtMs: number,
): RecordingViewState {
  return {
    status: "recording",
    recordingId,
    path,
    startedAtMs,
    elapsedSeconds: 0,
    message: "녹음 중입니다.",
    warning: false,
  }
}

// Elapsed time is measured against the session start instead of counting
// ticks: the webview throttles or suspends timers while the window sits in
// the background, so every skipped tick would otherwise be lost for good.
export function tickRecording(state: RecordingViewState, nowMs: number): RecordingViewState {
  if (state.status !== "recording") return state
  if (state.startedAtMs === null) return state
  const elapsedSeconds = Math.max(0, Math.floor((nowMs - state.startedAtMs) / 1_000))
  if (elapsedSeconds === state.elapsedSeconds) return state
  return { ...state, elapsedSeconds }
}

export function stopRecordingState(state: RecordingViewState): RecordingViewState {
  return { ...state, status: "stopping", message: "녹음 파일을 마무리합니다." }
}

export function cancelRecordingState(state: RecordingViewState): RecordingViewState {
  return { ...state, status: "stopping", message: "녹음을 취소하고 파일을 정리합니다." }
}

// The finished time comes from the captured frames the recorder actually
// wrote, which is the only authoritative length of the saved file.
export function completeRecordingState(
  state: RecordingViewState,
  path: string,
  durationSeconds: number,
  droppedFrames: number,
): RecordingViewState {
  return {
    ...state,
    status: "completed",
    path,
    startedAtMs: null,
    elapsedSeconds: Math.max(0, Math.round(durationSeconds)),
    warning: droppedFrames > 0,
    message:
      droppedFrames === 0
        ? "녹음이 완료되어 전사 파일로 선택했습니다."
        : "녹음은 완료됐지만 일부 오디오가 누락되어 무음으로 대체됐습니다.",
  }
}

export function failRecordingState(message: string): RecordingViewState {
  return { ...initialRecordingState, status: "failed", message }
}
