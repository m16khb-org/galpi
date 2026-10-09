import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"
import { open } from "@tauri-apps/plugin-dialog"
import { openUrl } from "@tauri-apps/plugin-opener"
import { z } from "zod"

import type {
  AppAccess,
  ArtifactKind,
  BackendPort,
  RecordingFailure,
  RecordingResult,
  RecordingStatus,
  SetupResult,
  TranscriptImportRequest,
  TranscriptionRequest,
} from "../domain/backend"
import type {
  AssistantSettings,
  ComputeDevice,
  EnginePreset,
  EnvironmentStatus,
  ImportedTranscript,
  JobEvent,
  RefinementResult,
  TranscriptionResult,
} from "../domain/job"
import type {
  ChatGptModel,
  ChatGptPreferences,
  ChatGptSettings,
  ChatGptSignInPhase,
  ChatGptSignOutResult,
} from "../domain/chatgpt"

const enginePresetSchema = z.enum(["qwen3", "whisperx"])
const computeDeviceSchema = z.enum(["cpu", "cuda"])

const environmentSchema = z.object({
  enginePreset: enginePresetSchema,
  engineReady: z.boolean(),
  modelsReady: z.boolean(),
  ffmpegReady: z.boolean(),
  qwen3Ready: z.boolean(),
  whisperxReady: z.boolean(),
  dataDirectory: z.string(),
  defaultOutputDirectory: z.string(),
  engineVersion: z.string(),
  computeDevice: computeDeviceSchema,
  availablePresets: z.array(enginePresetSchema),
  availableDevices: z.array(computeDeviceSchema),
  cudaDriverDetected: z.boolean(),
})

const transcriptionResultSchema = z.object({
  jobId: z.string(),
  srt: z.string(),
  txt: z.string(),
  checkpoint: z.string().nullable(),
  outputDirectory: z.string(),
  segments: z.number().int().nonnegative(),
  filtered: z.number().int().nonnegative(),
})

const setupResultSchema = z.object({
  jobId: z.string(),
  status: environmentSchema,
})
const participantSchema = z.object({
  id: z.string(),
  name: z.string(),
  team: z.string().nullable(),
  role: z.string().nullable(),
  description: z.string().nullable(),
  aliases: z.array(z.string()),
})

const glossaryEntrySchema = z.object({
  id: z.string(),
  term: z.string(),
  description: z.string().nullable(),
})

const assistantSettingsSchema = z.object({
  apiKeyStored: z.boolean(),
  model: z.string().nullable(),
  baseUrl: z.string().nullable(),
  reasoningEffort: z.string().nullable(),
  background: z.string().nullable(),
  participants: z.array(participantSchema),
  glossary: z.array(glossaryEntrySchema),
})

const refinementResultSchema = z.object({
  jobId: z.string(),
  minutes: z.string(),
})

// Strict on purpose: the host promises that no token, client id or host id
// ever reaches the window, so an unknown key is a contract breach to refuse
// rather than a field to quietly drop.
export const chatGptSettingsSchema = z.strictObject({
  authMode: z.enum(["apiKey", "chatGpt"]),
  model: z.string().nullable(),
  welcomeAcknowledged: z.boolean(),
  account: z.strictObject({
    state: z.enum(["signedOut", "signInRequired", "signedIn"]),
    email: z.string().nullable(),
  }),
})

export const chatGptModelsSchema = z.array(
  z.strictObject({ slug: z.string(), displayName: z.string() }),
)

export const chatGptSignOutSchema = z.strictObject({
  settings: chatGptSettingsSchema,
  remoteRevocationConfirmed: z.boolean(),
})

const chatGptEventSchema = z.object({ phase: z.enum(["awaitingBrowser", "exchanging"]) })

// Strict for the same reason: no session token may ever reach the window.
export const appAccessSchema = z.discriminatedUnion("state", [
  z.strictObject({ state: z.literal("signedOut") }),
  z.strictObject({
    state: z.literal("signedIn"),
    email: z.string().nullable(),
    offline: z.boolean(),
  }),
])

const transcriptImportSchema = z.object({
  jobId: z.string(),
  txt: z.string(),
  outputDirectory: z.string(),
})

const recordingStatusSchema = z.object({
  recordingId: z.string(),
  path: z.string(),
  sampleRate: z.number().int().positive(),
  channels: z.number().int().positive(),
})

const recordingResultSchema = recordingStatusSchema.extend({
  frames: z.number().int().nonnegative(),
  droppedFrames: z.number().int().nonnegative(),
  durationSeconds: z.number().nonnegative(),
})

const recordingFailureSchema = z.object({
  recordingId: z.string(),
  code: z.string(),
  message: z.string(),
})

const rawJobEventSchema = z.discriminatedUnion("type", [
  z.object({
    jobId: z.string(),
    type: z.literal("phase"),
    phase: z.string(),
    percent: z.number(),
    message: z.string(),
  }),
  z.object({
    jobId: z.string(),
    type: z.literal("log"),
    stream: z.string(),
    message: z.string(),
  }),
  z.object({
    jobId: z.string(),
    type: z.literal("completed"),
    srt: z.string(),
    txt: z.string(),
    checkpoint: z.string(),
    segments: z.number().int().nonnegative(),
    filtered: z.number().int().nonnegative(),
  }),
  z.object({
    jobId: z.string(),
    type: z.literal("prepared"),
    engine_version: z.string(),
  }),
  z.object({
    jobId: z.string(),
    type: z.literal("refined"),
    minutes: z.string(),
  }),
  z.object({
    jobId: z.string(),
    type: z.literal("error"),
    code: z.string(),
    message: z.string(),
  }),
])

export class TauriBackend implements BackendPort {
  async diagnose(): Promise<EnvironmentStatus> {
    return environmentSchema.parse(await invoke<unknown>("diagnose_environment"))
  }

  async prepare(jobId: string): Promise<SetupResult> {
    return setupResultSchema.parse(
      await invoke<unknown>("prepare_environment", {
        request: { jobId, huggingFaceToken: null },
      }),
    )
  }

  async huggingFaceTokenStored(): Promise<boolean> {
    return z.boolean().parse(await invoke<unknown>("hugging_face_token_stored"))
  }

  async saveHuggingFaceToken(token: string): Promise<void> {
    await invoke("save_hugging_face_token", { token })
  }

  async loadAssistantSettings(): Promise<AssistantSettings> {
    return assistantSettingsSchema.parse(await invoke<unknown>("load_assistant_settings"))
  }

  async saveAssistantApiKey(key: string): Promise<void> {
    await invoke("save_assistant_api_key", { key })
  }

  async saveAssistantSettings(settings: AssistantSettings): Promise<void> {
    await invoke("save_assistant_settings", { settings })
  }

  async saveEnginePreset(preset: EnginePreset): Promise<void> {
    await invoke("save_engine_preset", { preset })
  }

  async saveComputeDevice(device: ComputeDevice): Promise<void> {
    await invoke("save_compute_device", { device })
  }

  async refineTranscript(
    jobId: string,
    target: string,
    attendees: readonly string[],
  ): Promise<RefinementResult> {
    return refinementResultSchema.parse(
      await invoke<unknown>("refine_transcript", { jobId, target, attendees }),
    )
  }

  async transcribe(request: TranscriptionRequest): Promise<TranscriptionResult> {
    return transcriptionResultSchema.parse(
      await invoke<unknown>("start_transcription", { request }),
    )
  }

  async importTranscript(request: TranscriptImportRequest): Promise<ImportedTranscript> {
    return transcriptImportSchema.parse(await invoke<unknown>("import_transcript", { request }))
  }

  async cancel(jobId: string): Promise<void> {
    await invoke("cancel_job", { jobId })
  }

  async openArtifact(jobId: string, kind: ArtifactKind): Promise<void> {
    await invoke("open_artifact", { jobId, kind })
  }

  async revealOutput(jobId: string): Promise<void> {
    await invoke("reveal_output_directory", { jobId })
  }

  async startRecording(outputRoot: string): Promise<RecordingStatus> {
    return recordingStatusSchema.parse(await invoke<unknown>("start_recording", { outputRoot }))
  }

  async stopRecording(recordingId: string): Promise<RecordingResult> {
    return recordingResultSchema.parse(await invoke<unknown>("stop_recording", { recordingId }))
  }

  async cancelRecording(recordingId: string): Promise<void> {
    await invoke("cancel_recording", { recordingId })
  }

  listenToRecordingFailures(handler: (event: RecordingFailure) => void): Promise<() => void> {
    return listen<unknown>("recording-event", ({ payload }) => {
      handler(recordingFailureSchema.parse(payload))
    })
  }

  async chooseAudio(): Promise<string | null> {
    const selection = await open({
      multiple: false,
      directory: false,
      filters: [
        {
          name: "오디오",
          extensions: ["m4a", "mp3", "wav", "mp4", "mov", "aac", "flac", "ogg"],
        },
      ],
    })
    return typeof selection === "string" ? selection : null
  }

  async chooseTranscript(defaultPath: string | null): Promise<string | null> {
    const options: Parameters<typeof open>[0] = {
      multiple: false,
      directory: false,
      filters: [{ name: "전사문", extensions: ["txt", "md"] }],
    }
    if (defaultPath !== null) options.defaultPath = defaultPath
    const selection = await open(options)
    return typeof selection === "string" ? selection : null
  }

  async chooseOutputDirectory(): Promise<string | null> {
    const selection = await open({ multiple: false, directory: true })
    return typeof selection === "string" ? selection : null
  }

  async openModelAccessPage(): Promise<void> {
    await openUrl("https://huggingface.co/pyannote/speaker-diarization-community-1")
  }

  async listenToJobs(handler: (event: JobEvent) => void): Promise<() => void> {
    return listen<unknown>("job-event", ({ payload }) => {
      handler(toJobEvent(payload))
    })
  }

  async loadChatGptSettings(): Promise<ChatGptSettings> {
    return chatGptSettingsSchema.parse(await invoke<unknown>("load_chatgpt_settings"))
  }

  async saveChatGptPreferences(preferences: ChatGptPreferences): Promise<void> {
    await invoke("save_chatgpt_preferences", { preferences })
  }

  async signInWithChatGpt(): Promise<ChatGptSettings> {
    return chatGptSettingsSchema.parse(await invoke<unknown>("sign_in_with_chatgpt"))
  }

  async cancelChatGptSignIn(): Promise<void> {
    await invoke("cancel_chatgpt_sign_in")
  }

  async listChatGptModels(): Promise<readonly ChatGptModel[]> {
    return chatGptModelsSchema.parse(await invoke<unknown>("list_chatgpt_models"))
  }

  async signOutOfChatGpt(): Promise<ChatGptSignOutResult> {
    return chatGptSignOutSchema.parse(await invoke<unknown>("sign_out_of_chatgpt"))
  }

  async openChatGptUsagePage(): Promise<void> {
    await openUrl("https://chatgpt.com/settings/usage")
  }

  async listenToChatGptEvents(handler: (phase: ChatGptSignInPhase) => void): Promise<() => void> {
    return listen<unknown>("chatgpt-event", ({ payload }) => {
      const phase = parseChatGptEvent(payload)
      if (phase !== null) handler(phase)
    })
  }

  async loadAppAccess(): Promise<AppAccess> {
    return appAccessSchema.parse(await invoke<unknown>("load_app_access"))
  }

  async signInToGateway(): Promise<AppAccess> {
    return appAccessSchema.parse(await invoke<unknown>("sign_in_to_gateway"))
  }

  async cancelGatewaySignIn(): Promise<void> {
    await invoke("cancel_gateway_sign_in")
  }

  async signOutOfGateway(): Promise<void> {
    await invoke("sign_out_of_gateway")
  }
}

/**
 * Translate one raw job payload from the host into a domain event.
 *
 * A payload this build does not recognize becomes a log line rather than a
 * thrown error: the listener runs inside Tauri's own callback, where a throw
 * is swallowed and the event simply disappears with nothing on screen to say
 * so. Surfacing it in the job log keeps a host/window version mismatch visible.
 */
export function toJobEvent(payload: unknown): JobEvent {
  const parsed = rawJobEventSchema.safeParse(payload)
  if (!parsed.success) {
    return {
      jobId: payloadJobId(payload),
      type: "log",
      stream: "frontend",
      message: `알 수 없는 작업 이벤트를 받았습니다: ${JSON.stringify(payload)}`,
    }
  }
  const raw = parsed.data
  return raw.type === "prepared"
    ? { jobId: raw.jobId, type: raw.type, engineVersion: raw.engine_version }
    : raw
}

/**
 * Read the phase out of a `chatgpt-event` payload. An unrecognized payload is
 * dropped: the listener runs inside Tauri's callback where a throw is
 * swallowed, and a missing progress hint costs nothing — the sign-in command's
 * own result still decides the outcome.
 */
export function parseChatGptEvent(payload: unknown): ChatGptSignInPhase | null {
  const parsed = chatGptEventSchema.safeParse(payload)
  return parsed.success ? parsed.data.phase : null
}

function payloadJobId(payload: unknown): string {
  if (typeof payload !== "object" || payload === null) return ""
  const jobId = (payload as { jobId?: unknown }).jobId
  return typeof jobId === "string" ? jobId : ""
}
