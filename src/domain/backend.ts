import type {
  AssistantSettings,
  EnginePreset,
  EnvironmentStatus,
  ImportedTranscript,
  JobEvent,
  RefinementResult,
  TranscriptionResult,
} from "./job"
import type {
  ChatGptModel,
  ChatGptPreferences,
  ChatGptSettings,
  ChatGptSignInPhase,
  ChatGptSignOutResult,
} from "./chatgpt"
import type { SpeakerHint } from "./speaker"

/**
 * Port contract between the UI controllers and the native runtime.
 * The inner layers own this interface; `TauriBackend` in `adapters/` implements it.
 */
export interface BackendPort {
  diagnose(): Promise<EnvironmentStatus>
  prepare(jobId: string): Promise<SetupResult>
  huggingFaceTokenStored(): Promise<boolean>
  saveHuggingFaceToken(token: string): Promise<void>
  loadAssistantSettings(): Promise<AssistantSettings>
  /** Stores the key on its own so a settings autosave can never carry it; "" clears it. */
  saveAssistantApiKey(key: string): Promise<void>
  saveAssistantSettings(settings: AssistantSettings): Promise<void>
  saveEnginePreset(preset: EnginePreset): Promise<void>
  refineTranscript(
    jobId: string,
    target: string,
    attendees: readonly string[],
  ): Promise<RefinementResult>
  transcribe(request: TranscriptionRequest): Promise<TranscriptionResult>
  importTranscript(request: TranscriptImportRequest): Promise<ImportedTranscript>
  cancel(jobId: string): Promise<void>
  openArtifact(jobId: string, kind: ArtifactKind): Promise<void>
  revealOutput(jobId: string): Promise<void>
  startRecording(outputRoot: string): Promise<RecordingStatus>
  stopRecording(recordingId: string): Promise<RecordingResult>
  cancelRecording(recordingId: string): Promise<void>
  listenToRecordingFailures(handler: (event: RecordingFailure) => void): Promise<() => void>
  chooseAudio(): Promise<string | null>
  /** Opens at the Galpi meeting root when one is known (default ~/Documents/Galpi). */
  chooseTranscript(defaultPath: string | null): Promise<string | null>
  chooseOutputDirectory(): Promise<string | null>
  openModelAccessPage(): Promise<void>
  listenToJobs(handler: (event: JobEvent) => void): Promise<() => void>
  loadChatGptSettings(): Promise<ChatGptSettings>
  /** Saves only what the window owns; the account and its tokens stay with the host. */
  saveChatGptPreferences(preferences: ChatGptPreferences): Promise<void>
  /** Resolves once the browser sign-in finished; progress arrives on `listenToChatGptEvents`. */
  signInWithChatGpt(): Promise<ChatGptSettings>
  cancelChatGptSignIn(): Promise<void>
  listChatGptModels(): Promise<readonly ChatGptModel[]>
  signOutOfChatGpt(): Promise<ChatGptSignOutResult>
  openChatGptUsagePage(): Promise<void>
  listenToChatGptEvents(handler: (phase: ChatGptSignInPhase) => void): Promise<() => void>
}

export interface SetupResult {
  readonly jobId: string
  readonly status: EnvironmentStatus
}

export interface TranscriptionRequest {
  readonly jobId: string
  readonly inputPath: string
  readonly outputRoot: string
  readonly speakerHint: SpeakerHint
}

export interface TranscriptImportRequest {
  readonly jobId: string
  readonly inputPath: string
  readonly outputRoot: string
}

export type ArtifactKind = "srt" | "speaker_text" | "checkpoint" | "minutes"

export interface RecordingStatus {
  readonly recordingId: string
  readonly path: string
  readonly sampleRate: number
  readonly channels: number
}

export interface RecordingResult extends RecordingStatus {
  readonly frames: number
  readonly droppedFrames: number
  readonly durationSeconds: number
}

export interface RecordingFailure {
  readonly recordingId: string
  readonly code: string
  readonly message: string
}

const UNEXPECTED_ERROR_MESSAGE = "예기치 못한 오류가 발생했습니다."

function isAppError(error: object): error is { code: string; message: string } {
  return (
    "code" in error &&
    typeof error.code === "string" &&
    "message" in error &&
    typeof error.message === "string"
  )
}

/** Native commands fail with AppError {code, message}; its message is user-facing
 * Korean copy. Anything else is a runtime fault the user cannot act on, so it gets
 * stable Korean copy while errorDetail keeps the raw diagnostic for the log. */
export function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && isAppError(error)) {
    return error.message
  }
  return UNEXPECTED_ERROR_MESSAGE
}

export function errorDetail(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = error.message
    if (typeof message === "string") {
      return message
    }
  }
  return error instanceof Error ? error.message : String(error)
}

/** The stable ASCII code of a native AppError, or null for any other failure. */
export function errorCode(error: unknown): string | null {
  return typeof error === "object" && error !== null && isAppError(error) ? error.code : null
}
