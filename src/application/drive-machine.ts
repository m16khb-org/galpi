import type { DriveRecordingUpload, DriveStatus, DriveSyncResult } from "../domain/drive"

export type DriveConnection =
  | { readonly status: "unknown" }
  | { readonly status: "disconnected"; readonly onboardingDismissed: boolean }
  | {
      readonly status: "connected"
      readonly email: string
      readonly saveRecordings: boolean
    }

/**
 * The one Drive action that blocks others. Importing shows the Picker wait
 * and the download as one step: nothing tells the window when one ends.
 */
export type DriveActivity = "idle" | "connecting" | "importing"

/** The latest meeting's sync; an answer for any other job is stale. */
export type DriveSync =
  | { readonly status: "syncing"; readonly jobId: string }
  | { readonly status: "done"; readonly jobId: string; readonly result: DriveSyncResult }
  | { readonly status: "failed"; readonly jobId: string; readonly message: string }

/** The latest recording's upload; an answer for any other recording is stale. */
export type DriveRecording =
  | { readonly status: "uploading"; readonly recordingId: string }
  | { readonly status: "done"; readonly recordingId: string; readonly result: DriveRecordingUpload }
  | { readonly status: "failed"; readonly recordingId: string; readonly message: string }

export interface DriveState {
  readonly connection: DriveConnection
  readonly activity: DriveActivity
  /** A connect, disconnect, or onboarding failure, or a request to reconnect. */
  readonly message: string | null
  readonly sync: DriveSync | null
  /** The preference write in flight; the settings switch waits for it. */
  readonly savingRecordings: boolean
  readonly recording: DriveRecording | null
}

export type DriveEvent =
  | { readonly type: "statusLoaded"; readonly status: DriveStatus }
  | { readonly type: "connectStarted" }
  | { readonly type: "connectSucceeded"; readonly status: DriveStatus }
  | { readonly type: "connectFailed"; readonly message: string }
  | { readonly type: "connectCancelled" }
  | { readonly type: "onboardingDismissed"; readonly status: DriveStatus }
  | { readonly type: "disconnected"; readonly status: DriveStatus }
  | { readonly type: "failed"; readonly message: string }
  | { readonly type: "importStarted" }
  | { readonly type: "imported" }
  | { readonly type: "importFailed" }
  | { readonly type: "syncStarted"; readonly jobId: string }
  | { readonly type: "syncFinished"; readonly jobId: string; readonly result: DriveSyncResult }
  | { readonly type: "syncFailed"; readonly jobId: string; readonly message: string }
  | { readonly type: "syncCleared" }
  | { readonly type: "reconnectRequired"; readonly message: string }
  | { readonly type: "saveRecordingsStarted" }
  | { readonly type: "saveRecordingsSaved"; readonly status: DriveStatus }
  | { readonly type: "saveRecordingsFailed" }
  | { readonly type: "recordingUploadStarted"; readonly recordingId: string }
  | {
      readonly type: "recordingUploadFinished"
      readonly recordingId: string
      readonly result: DriveRecordingUpload
    }
  | {
      readonly type: "recordingUploadFailed"
      readonly recordingId: string
      readonly message: string
    }
  | { readonly type: "recordingUploadCleared" }
  | { readonly type: "signedOut" }

export const initialDrive: DriveState = {
  connection: { status: "unknown" },
  activity: "idle",
  message: null,
  sync: null,
  savingRecordings: false,
  recording: null,
}

function connectionOf(status: DriveStatus): DriveConnection {
  return status.state === "connected"
    ? { status: "connected", email: status.email, saveRecordings: status.saveRecordings }
    : { status: "disconnected", onboardingDismissed: status.onboardingDismissed }
}

export function reduceDrive(state: DriveState, event: DriveEvent): DriveState {
  switch (event.type) {
    case "statusLoaded":
      return { ...state, connection: connectionOf(event.status) }
    case "connectStarted":
      return state.activity === "idle" && state.connection.status === "disconnected"
        ? { ...state, activity: "connecting", message: null }
        : state
    case "connectSucceeded":
      return state.activity === "connecting"
        ? { ...state, connection: connectionOf(event.status), activity: "idle", message: null }
        : state
    case "connectFailed":
      return state.activity === "connecting"
        ? { ...state, activity: "idle", message: event.message }
        : state
    case "connectCancelled":
      return state.activity === "connecting" ? { ...state, activity: "idle", message: null } : state
    case "onboardingDismissed":
      return { ...state, connection: connectionOf(event.status), message: null }
    case "disconnected":
      return {
        ...state,
        connection: connectionOf(event.status),
        message: null,
        sync: null,
        recording: null,
      }
    case "failed":
      return { ...state, message: event.message }
    case "importStarted":
      return state.activity === "idle" && state.connection.status === "connected"
        ? { ...state, activity: "importing" }
        : state
    case "imported":
    case "importFailed":
      return state.activity === "importing" ? { ...state, activity: "idle" } : state
    case "syncStarted":
      return state.connection.status === "connected"
        ? { ...state, sync: { status: "syncing", jobId: event.jobId } }
        : state
    case "syncFinished":
      return state.sync?.jobId === event.jobId
        ? { ...state, sync: { status: "done", jobId: event.jobId, result: event.result } }
        : state
    case "syncFailed":
      return state.sync?.jobId === event.jobId
        ? { ...state, sync: { status: "failed", jobId: event.jobId, message: event.message } }
        : state
    case "syncCleared":
      return { ...state, sync: null }
    case "saveRecordingsStarted":
      return state.connection.status === "connected" && !state.savingRecordings
        ? { ...state, savingRecordings: true }
        : state
    case "saveRecordingsSaved":
      return state.savingRecordings
        ? { ...state, savingRecordings: false, connection: connectionOf(event.status) }
        : state
    case "saveRecordingsFailed":
      return state.savingRecordings ? { ...state, savingRecordings: false } : state
    case "recordingUploadStarted":
      return state.connection.status === "connected"
        ? { ...state, recording: { status: "uploading", recordingId: event.recordingId } }
        : state
    case "recordingUploadFinished":
      return state.recording?.status === "uploading" &&
        state.recording.recordingId === event.recordingId
        ? {
            ...state,
            recording: { status: "done", recordingId: event.recordingId, result: event.result },
          }
        : state
    case "recordingUploadFailed":
      return state.recording?.status === "uploading" &&
        state.recording.recordingId === event.recordingId
        ? {
            ...state,
            recording: { status: "failed", recordingId: event.recordingId, message: event.message },
          }
        : state
    case "recordingUploadCleared":
      return { ...state, recording: null }
    case "reconnectRequired":
      // The host already forgot the connection; the onboarding card offers it again.
      return {
        connection: { status: "disconnected", onboardingDismissed: false },
        activity: state.activity === "connecting" ? "connecting" : "idle",
        message: event.message,
        sync:
          state.sync === null
            ? null
            : { status: "failed", jobId: state.sync.jobId, message: event.message },
        savingRecordings: state.savingRecordings,
        recording:
          state.recording === null
            ? null
            : {
                status: "failed",
                recordingId: state.recording.recordingId,
                message: event.message,
              },
      }
    case "signedOut":
      return initialDrive
  }
}
