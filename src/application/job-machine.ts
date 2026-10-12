import type { JobEvent, TranscriptionResult } from "../domain/job"

export type JobStatus = "idle" | "running" | "completed" | "failed" | "cancelled"

export interface JobViewState {
  readonly status: JobStatus
  readonly jobId: string | null
  readonly phase: string
  readonly percent: number
  readonly message: string
  readonly logs: readonly string[]
  readonly error: string | null
}

export const initialJobState: JobViewState = {
  status: "idle",
  jobId: null,
  phase: "idle",
  percent: 0,
  message: "",
  logs: [],
  error: null,
}

export function reduceJobEvent(state: JobViewState, event: JobEvent): JobViewState {
  if (state.jobId !== null && event.jobId !== state.jobId) return state
  if (event.type === "phase" && isSettled(state.status)) return state
  switch (event.type) {
    case "phase":
      return {
        ...state,
        status: "running",
        jobId: event.jobId,
        phase: event.phase,
        percent:
          state.phase === event.phase ? Math.max(state.percent, event.percent) : event.percent,
        message: event.message,
        error: null,
      }
    case "log": {
      const lines = event.message.split("\n").map((line) => `[${event.stream}] ${line}`)
      return { ...state, jobId: event.jobId, logs: [...state.logs, ...lines].slice(-200) }
    }
    case "completed":
      return {
        ...state,
        status: "completed",
        jobId: event.jobId,
        phase: "writing",
        percent: 100,
        message: "결과 파일을 저장했습니다.",
      }
    case "prepared":
      return {
        ...state,
        status: "completed",
        jobId: event.jobId,
        phase: "ready",
        percent: 100,
        message: `로컬 엔진 준비가 완료되었습니다. (${event.engineVersion})`,
      }
    case "refined":
      return {
        ...state,
        status: "completed",
        jobId: event.jobId,
        phase: "writing",
        percent: 100,
        message: "회의록을 저장했습니다.",
      }
    case "error":
      return {
        ...state,
        status: "failed",
        jobId: event.jobId,
        error: event.message,
        message: "작업이 실패했습니다.",
      }
  }
}

function isSettled(status: JobStatus): boolean {
  return status === "completed" || status === "failed" || status === "cancelled"
}

export function beginJob(message: string, jobId: string | null = null): JobViewState {
  return {
    ...initialJobState,
    status: "running",
    jobId,
    message,
  }
}

export function completeJob(state: JobViewState, result: TranscriptionResult): JobViewState {
  return {
    ...state,
    status: "completed",
    jobId: result.jobId,
    phase: "writing",
    percent: 100,
    message: "전사가 완료되었습니다.",
    error: null,
  }
}

export function cancelJob(state: JobViewState, message: string): JobViewState {
  // Cancellation is a user decision, not a failure: announce it once, politely.
  return { ...state, status: "cancelled", message, error: null }
}

export function failJob(state: JobViewState, message: string): JobViewState {
  if (state.status === "cancelled") {
    // The job was already cancelled here; whatever the backend raised as the
    // process died is the consequence, not a new failure to report.
    return state
  }
  // The polite slot stays on a stable status line; the specific cause is
  // announced exactly once through the alert slot (no duplicate readings).
  return { ...state, status: "failed", message: "작업이 실패했습니다.", error: message }
}
