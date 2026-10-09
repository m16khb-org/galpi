import type { GlossaryEntry } from "./glossary"
import type { Participant } from "./participant"

export type EnginePreset = "qwen3" | "whisperx"
export type ComputeDevice = "cpu" | "cuda"

export interface EnvironmentStatus {
  readonly enginePreset: EnginePreset
  readonly engineReady: boolean
  readonly modelsReady: boolean
  readonly ffmpegReady: boolean
  readonly qwen3Ready: boolean
  readonly whisperxReady: boolean
  readonly dataDirectory: string
  readonly defaultOutputDirectory: string
  readonly engineVersion: string
  readonly computeDevice: ComputeDevice
  /** Presets this platform offers; the first is the platform default. */
  readonly availablePresets: readonly EnginePreset[]
  /** Devices the platform can run on; empty when there is no choice to make. */
  readonly availableDevices: readonly ComputeDevice[]
  readonly cudaDriverDetected: boolean
}

export interface TranscriptionResult {
  readonly jobId: string
  readonly srt: string
  readonly txt: string
  readonly checkpoint: string | null
  readonly outputDirectory: string
  readonly segments: number
  readonly filtered: number
}

export interface AssistantSettings {
  /** Whether the host holds a key. The value itself never crosses the IPC border. */
  readonly apiKeyStored: boolean
  readonly model: string | null
  readonly baseUrl: string | null
  readonly reasoningEffort: string | null
  readonly background: string | null
  readonly participants: readonly Participant[]
  readonly glossary: readonly GlossaryEntry[]
}

export interface RefinementResult {
  readonly jobId: string
  readonly minutes: string
}

export interface ImportedTranscript {
  readonly jobId: string
  readonly txt: string
  readonly outputDirectory: string
}

export type JobEvent =
  | {
      readonly jobId: string
      readonly type: "phase"
      readonly phase: string
      readonly percent: number
      readonly message: string
    }
  | {
      readonly jobId: string
      readonly type: "log"
      readonly stream: string
      readonly message: string
    }
  | {
      readonly jobId: string
      readonly type: "completed"
      readonly srt: string
      readonly txt: string
      readonly checkpoint: string
      readonly segments: number
      readonly filtered: number
    }
  | {
      readonly jobId: string
      readonly type: "prepared"
      readonly engineVersion: string
    }
  | {
      readonly jobId: string
      readonly type: "refined"
      readonly minutes: string
    }
  | {
      readonly jobId: string
      readonly type: "error"
      readonly code: string
      readonly message: string
    }
