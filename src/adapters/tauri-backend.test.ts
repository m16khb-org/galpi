import { describe, expect, mock, test } from "bun:test"

import type { DriveRecordingUpload, DriveSyncResult } from "../domain/drive"

import {
  appAccessSchema,
  chatGptModelsSchema,
  chatGptSettingsSchema,
  chatGptSignOutSchema,
  driveImportSchema,
  driveRecordingUploadSchema,
  driveStatusSchema,
  driveSyncResultSchema,
  parseChatGptEvent,
  toJobEvent,
} from "./tauri-backend"

describe("toJobEvent", () => {
  test("renames the worker's engine_version onto the domain field", () => {
    // Given: the host spells the field the way the worker protocol does
    const payload = { jobId: "job-1", type: "prepared", engine_version: "3.8.6" }

    // When
    const event = toJobEvent(payload)

    // Then
    expect(event).toEqual({ jobId: "job-1", type: "prepared", engineVersion: "3.8.6" })
  })

  test("passes a batched log through untouched", () => {
    // Given: the host groups worker output into one event
    const payload = { jobId: "job-1", type: "log" as const, stream: "stderr", message: "a\nb\nc" }

    // When / Then
    expect(toJobEvent(payload)).toEqual(payload)
  })

  test("turns an unrecognized payload into a visible log line", () => {
    // Given: a host newer than this window, emitting an event it does not know
    const payload = { jobId: "job-1", type: "teleported", destination: "mars" }

    // When
    const event = toJobEvent(payload)

    // Then: the job it belongs to still shows that something arrived
    expect(event.type).toBe("log")
    expect(event.jobId).toBe("job-1")
    if (event.type === "log") {
      expect(event.stream).toBe("frontend")
      expect(event.message).toContain("teleported")
    }
  })

  test("survives a payload that is not an object at all", () => {
    // Given / When
    const event = toJobEvent("nonsense")

    // Then: no throw, and the event is attributed to no job
    expect(event.type).toBe("log")
    expect(event.jobId).toBe("")
  })
})

describe("TauriBackend compute device", () => {
  test("parses the platform fields and invokes save_compute_device", async () => {
    // Given: a host reply carrying the Windows capability fields
    const calls: Array<[string, unknown]> = []
    mock.module("@tauri-apps/api/core", () => ({
      invoke: async (command: string, args?: unknown) => {
        calls.push([command, args])
        if (command !== "diagnose_environment") return undefined
        return {
          enginePreset: "whisperx",
          engineReady: true,
          modelsReady: true,
          ffmpegReady: true,
          qwen3Ready: false,
          whisperxReady: true,
          dataDirectory: "C:\\Galpi",
          defaultOutputDirectory: "C:\\Galpi\\out",
          engineVersion: "3.8.6",
          computeDevice: "cuda",
          availablePresets: ["whisperx"],
          availableDevices: ["cpu", "cuda"],
          cudaDriverDetected: true,
        }
      },
    }))
    const { TauriBackend } = await import("./tauri-backend")
    const backend = new TauriBackend()

    // When
    const status = await backend.diagnose()
    await backend.saveComputeDevice("cpu")

    // Then
    expect(status.computeDevice).toBe("cuda")
    expect(status.availablePresets).toEqual(["whisperx"])
    expect(status.availableDevices).toEqual(["cpu", "cuda"])
    expect(status.cudaDriverDetected).toBe(true)
    expect(calls).toContainEqual(["save_compute_device", { device: "cpu" }])
  })
})

const signedInSettings = {
  authMode: "chatGpt",
  model: "gpt-5.5",
  reasoningEffort: "high",
  welcomeAcknowledged: true,
  account: { state: "signedIn", email: "dev@example.com" },
}

describe("chatGptSettingsSchema", () => {
  test("accepts the contract shape", () => {
    // Given / When
    const parsed = chatGptSettingsSchema.parse(signedInSettings)

    // Then
    expect(parsed).toEqual(signedInSettings as typeof parsed)
  })

  test("rejects a response that smuggles a token in at the top level", () => {
    // Given: a host bug that leaks a credential next to the settings
    const leaked = { ...signedInSettings, accessToken: "secret-value" }

    // When / Then: strict parsing refuses the whole payload
    expect(chatGptSettingsSchema.safeParse(leaked).success).toBe(false)
  })

  test("rejects a token nested in the account", () => {
    // Given
    const leaked = {
      ...signedInSettings,
      account: { ...signedInSettings.account, refreshToken: "secret-value" },
    }

    // When / Then
    expect(chatGptSettingsSchema.safeParse(leaked).success).toBe(false)
  })

  test("rejects an account state the contract does not define", () => {
    const unknown = { ...signedInSettings, account: { state: "pending", email: null } }

    expect(chatGptSettingsSchema.safeParse(unknown).success).toBe(false)
  })
})

describe("chatGptSignOutSchema", () => {
  test("carries the settings and whether the server confirmed the revocation", () => {
    // Given / When
    const parsed = chatGptSignOutSchema.parse({
      settings: { ...signedInSettings, authMode: "apiKey", model: null, account: { state: "signedOut", email: null } },
      remoteRevocationConfirmed: false,
    })

    // Then
    expect(parsed.remoteRevocationConfirmed).toBe(false)
    expect(parsed.settings.account.state).toBe("signedOut")
  })
})

describe("chatGptModelsSchema", () => {
  test("keeps the server order, the slug/displayName pair, and each model's efforts", () => {
    // Given
    const models = [
      {
        slug: "b-model",
        displayName: "B",
        reasoningEfforts: ["low", "high"],
        defaultReasoningEffort: "low",
      },
      { slug: "a-model", displayName: "A", reasoningEfforts: [], defaultReasoningEffort: null },
    ]

    // When / Then
    expect(chatGptModelsSchema.parse(models)).toEqual(models)
  })
})

describe("parseChatGptEvent", () => {
  test("reads both sign-in phases", () => {
    expect(parseChatGptEvent({ phase: "awaitingBrowser" })).toBe("awaitingBrowser")
    expect(parseChatGptEvent({ phase: "exchanging" })).toBe("exchanging")
  })

  test("drops a payload this build does not know instead of throwing", () => {
    // Given: the listener runs inside Tauri's callback, where a throw vanishes
    expect(parseChatGptEvent({ phase: "teleporting" })).toBeNull()
    expect(parseChatGptEvent("nonsense")).toBeNull()
    expect(parseChatGptEvent(null)).toBeNull()
  })
})

describe("appAccessSchema", () => {
  test("reads both access states", () => {
    expect(appAccessSchema.parse({ state: "signedOut" })).toEqual({ state: "signedOut" })
    expect(
      appAccessSchema.parse({ state: "signedIn", email: "dev@example.com", offline: true }),
    ).toEqual({ state: "signedIn", email: "dev@example.com", offline: true })
  })

  test("rejects a response that carries a session token", () => {
    const leaked = { state: "signedIn", email: null, offline: false, refreshToken: "secret" }

    expect(appAccessSchema.safeParse(leaked).success).toBe(false)
  })

  test("rejects a signed-in answer without the offline flag", () => {
    expect(appAccessSchema.safeParse({ state: "signedIn", email: null }).success).toBe(false)
  })
})

describe("Drive schemas", () => {
  test("read both connection states and refuse a leaked token", () => {
    const connected = { state: "connected" as const, email: "dev@example.com", saveRecordings: true }
    expect(driveStatusSchema.parse(connected)).toEqual(connected)
    expect(driveStatusSchema.parse({ state: "disconnected", onboardingDismissed: true })).toEqual({
      state: "disconnected",
      onboardingDismissed: true,
    })
    expect(driveStatusSchema.safeParse({ ...connected, accessToken: "secret" }).success).toBe(false)
  })

  test("refuse a connected status that does not say whether recordings are saved", () => {
    expect(
      driveStatusSchema.safeParse({ state: "connected", email: "dev@example.com" }).success,
    ).toBe(false)
  })

  test("read an import and refuse one without the Drive file id", () => {
    const imported = {
      jobId: "job-1",
      inputPath: "/cache/drive-imports/x/회의.m4a",
      driveFileId: "file-1",
      name: "회의.m4a",
    }
    expect(driveImportSchema.parse(imported)).toEqual(imported)
    expect(driveImportSchema.safeParse({ ...imported, driveFileId: undefined }).success).toBe(false)
  })

  test("read every sync state and require a message only for failures", () => {
    const result: DriveSyncResult = {
      folderLink: "https://drive.google.com/drive/folders/m1",
      files: [
        { kind: "srt", state: "uploaded", webViewLink: null },
        { kind: "speaker_text", state: "conflict", webViewLink: "https://drive.google.com/t" },
        { kind: "minutes", state: "failed", message: "저장 공간 부족", webViewLink: null },
      ],
    }
    const parsed: DriveSyncResult = driveSyncResultSchema.parse(result)
    expect(parsed).toEqual(result)
    expect(
      driveSyncResultSchema.safeParse({
        folderLink: null,
        files: [{ kind: "srt", state: "failed", webViewLink: null }],
      }).success,
    ).toBe(false)
    expect(
      driveSyncResultSchema.safeParse({ ...result, refreshToken: "secret" }).success,
    ).toBe(false)
  })

  test("read how a recording upload ended and refuse an unknown state", () => {
    const upload: DriveRecordingUpload = {
      state: "uploaded",
      folderLink: "https://drive.google.com/drive/folders/m1",
      webViewLink: null,
    }
    expect(driveRecordingUploadSchema.parse(upload)).toEqual(upload)
    expect(driveRecordingUploadSchema.safeParse({ ...upload, state: "failed" }).success).toBe(false)
    expect(driveRecordingUploadSchema.safeParse({ ...upload, accessToken: "secret" }).success).toBe(
      false,
    )
  })
})
