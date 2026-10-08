import { describe, expect, test } from "bun:test"
import { Window } from "happy-dom"

import type { BackendPort } from "../domain/backend"
import { AppView } from "./app-view"
import { AppController } from "./controller"

const styles = await Bun.file(new URL("../styles.css", import.meta.url)).text()

function unavailableBackend(): BackendPort {
  const unavailable = () => Promise.reject(new Error("no native runtime"))
  return {
    diagnose: unavailable,
    prepare: unavailable,
    huggingFaceTokenStored: unavailable,
    saveHuggingFaceToken: unavailable,
    saveAssistantApiKey: unavailable,
    loadAssistantSettings: unavailable,
    saveAssistantSettings: unavailable,
    saveEnginePreset: unavailable,
    saveComputeDevice: unavailable,
    refineTranscript: unavailable,
    transcribe: unavailable,
    importTranscript: unavailable,
    cancel: unavailable,
    openArtifact: unavailable,
    revealOutput: unavailable,
    startRecording: unavailable,
    stopRecording: unavailable,
    cancelRecording: unavailable,
    listenToRecordingFailures: unavailable,
    chooseAudio: unavailable,
    chooseTranscript: unavailable,
    chooseOutputDirectory: unavailable,
    openModelAccessPage: unavailable,
    listenToJobs: unavailable,
  }
}

function createHarness(): { controller: AppController; root: HTMLElement; window: Window } {
  const window = new Window()
  const sheet = window.document.createElement("style")
  sheet.textContent = styles
  window.document.head.appendChild(sheet)
  const root = window.document.createElement("div") as unknown as HTMLElement
  window.document.body.appendChild(root as unknown as never)
  const controller = new AppController(unavailableBackend(), new AppView(root))
  return { controller, root, window }
}

describe("AppController startup without a native runtime", () => {
  test("surfaces a visible error instead of a silent dead shell", async () => {
    // Given / When: every backend call, starting with the event subscription, rejects
    const { controller, root } = createHarness()
    await controller.start()

    // Then: the persistent banner carries the failure in Korean user copy
    const banner = root.querySelector("#app-error") as HTMLElement
    expect(banner.hidden).toBe(false)
    expect(banner.textContent).toContain("네이티브 런타임")
    controller.stop()
  })

  test("keeps controls bound so the settings sheet still opens", async () => {
    // Given: the native runtime never attached
    const { controller, root } = createHarness()
    await controller.start()

    // When: the user opens settings anyway
    ;(root.querySelector('[data-action="open-settings"]') as HTMLElement).click()
    await new Promise((resolve) => setTimeout(resolve, 0))
    await new Promise((resolve) => setTimeout(resolve, 0))

    // Then: the dialog opened and reports the load failure inside the sheet
    expect((root.querySelector("#settings-dialog") as HTMLElement).hidden).toBe(false)
    const message = root.querySelector("#settings-message") as HTMLElement
    expect(message.dataset["state"]).toBe("error")
    controller.stop()
  })

  test("surfaces direct IPC actions that fail instead of dying silently", async () => {
    // Given: the native runtime never attached, so every direct IPC call rejects
    const { controller, root } = createHarness()
    await controller.start()
    const banner = root.querySelector("#app-error") as HTMLElement
    const flush = () => new Promise((resolve) => setTimeout(resolve, 0))

    // When: the user triggers the three unguarded direct actions. Transcript
    // import is absent here: without a runtime the output folder is unknown and
    // the import action reports that guard error instead of reaching IPC.
    for (const action of ["choose-audio", "choose-output", "model-access"]) {
      banner.textContent = ""
      ;(root.querySelector(`[data-action="${action}"]`) as HTMLElement).click()
      await flush()
      await flush()

      // Then: each failure lands in the visible banner with user-facing copy
      expect(banner.hidden).toBe(false)
      expect(banner.textContent).toBe("예기치 못한 오류가 발생했습니다.")
    }
    controller.stop()
  })
})

describe("AppController engine preset", () => {
  test("switching the preset saves it and re-diagnoses the environment", async () => {
    // Given: a runtime whose diagnose reflects the saved preset
    const window = new Window()
    const sheet = window.document.createElement("style")
    sheet.textContent = styles
    window.document.head.appendChild(sheet)
    const root = window.document.createElement("div") as unknown as HTMLElement
    window.document.body.appendChild(root as unknown as never)
    const unavailable = () => Promise.reject(new Error("unused"))
    const state: { preset: string | null } = { preset: null }
    const backend = {
      diagnose: async () => {
        const preset = (state.preset ?? "qwen3") as "qwen3" | "whisperx"
        // In this fake only the legacy whisperx stack is installed: qwen3 is
        // the unready default, whisperx becomes ready once selected.
        const ready = preset === "whisperx"
        return {
          enginePreset: preset,
          engineReady: ready,
          modelsReady: ready,
          ffmpegReady: ready,
          qwen3Ready: false,
          whisperxReady: true,
          dataDirectory: "/tmp/galpi",
          defaultOutputDirectory: "/tmp/Documents/Galpi",
          engineVersion: "test",
          computeDevice: "cpu" as const,
          availablePresets: ["qwen3", "whisperx"] as const,
          availableDevices: [] as const,
          cudaDriverDetected: false,
        }
      },
      prepare: unavailable,
      huggingFaceTokenStored: async () => false,
      saveHuggingFaceToken: unavailable,
      saveAssistantApiKey: unavailable,
      loadAssistantSettings: async () => ({
        apiKeyStored: false,
        model: null,
        baseUrl: null,
        reasoningEffort: null,
        background: null,
        participants: [],
        glossary: [],
      }),
      saveAssistantSettings: unavailable,
      saveEnginePreset: async (preset: "qwen3" | "whisperx") => {
        state.preset = preset
      },
      saveComputeDevice: unavailable,
      refineTranscript: unavailable,
      transcribe: unavailable,
      importTranscript: unavailable,
      cancel: unavailable,
      openArtifact: unavailable,
      revealOutput: unavailable,
      startRecording: unavailable,
      stopRecording: unavailable,
      cancelRecording: unavailable,
      listenToRecordingFailures: async () => () => undefined,
      chooseAudio: unavailable,
      chooseTranscript: unavailable,
      chooseOutputDirectory: unavailable,
      openModelAccessPage: unavailable,
      listenToJobs: async () => () => undefined,
    }
    const controller = new AppController(backend as unknown as BackendPort, new AppView(root))
    await controller.start()

    // When: the user opens settings and switches to the legacy engine
    ;(root.querySelector('[data-action="open-settings"]') as HTMLElement).click()
    await new Promise((resolve) => setTimeout(resolve, 0))
    await new Promise((resolve) => setTimeout(resolve, 0))
    const whisperx = root.querySelector(
      'input[name="engine-preset"][value="whisperx"]',
    ) as HTMLInputElement
    // The picker must live in the always-reachable settings dialog, not the
    // setup panel that hides once the selected engine is ready.
    expect(whisperx.closest("#settings-dialog")).not.toBe(null)
    whisperx.click()
    await new Promise((resolve) => setTimeout(resolve, 0))
    await new Promise((resolve) => setTimeout(resolve, 0))

    // Then: the preset persisted and the panel re-rendered from the new state
    expect(state.preset).toBe("whisperx")
    expect((root.querySelector("#engine-check") as HTMLElement).textContent).toContain(
      "WhisperX 엔진",
    )
    // Regression: whisperx is ready here, so the setup panel hides itself —
    // the picker must survive that, still reachable inside the settings dialog.
    expect((root.querySelector("#setup-panel") as HTMLElement).hidden).toBe(true)
    expect(whisperx.closest("#settings-dialog")).not.toBe(null)
    expect((root.querySelector("#engine-settings-state") as HTMLElement).textContent).toBe(
      "WhisperX",
    )
    controller.stop()
  })
})

describe("AppController transcript import", () => {
  test("imports an existing transcript and unlocks augmentation", async () => {
    // Given: a native runtime that can import a transcript without transcription
    const window = new Window()
    const sheet = window.document.createElement("style")
    sheet.textContent = styles
    window.document.head.appendChild(sheet)
    const root = window.document.createElement("div") as unknown as HTMLElement
    window.document.body.appendChild(root as unknown as never)
    const unavailable = () => Promise.reject(new Error("unused"))
    let capturedDefaultPath: string | null | undefined
    const backend = {
      diagnose: async () => ({
        enginePreset: "qwen3" as const,
        engineReady: false,
        modelsReady: false,
        ffmpegReady: false,
        qwen3Ready: false,
        whisperxReady: false,
        dataDirectory: "/tmp/galpi",
        defaultOutputDirectory: "/tmp/Documents/Galpi",
        engineVersion: "test",
        computeDevice: "cpu" as const,
        availablePresets: ["qwen3", "whisperx"] as const,
        availableDevices: [] as const,
        cudaDriverDetected: false,
      }),
      prepare: unavailable,
      huggingFaceTokenStored: async () => false,
      saveHuggingFaceToken: unavailable,
      saveAssistantApiKey: unavailable,
      loadAssistantSettings: async () => ({
        apiKeyStored: true,
        model: null,
        baseUrl: null,
        reasoningEffort: null,
        background: null,
        participants: [],
        glossary: [],
      }),
      saveAssistantSettings: unavailable,
      saveEnginePreset: unavailable,
      saveComputeDevice: unavailable,
      refineTranscript: unavailable,
      transcribe: unavailable,
      importTranscript: async () => ({
        jobId: "job-import-1",
        txt: "/tmp/Documents/Galpi/팀미팅/팀미팅.txt",
        outputDirectory: "/tmp/Documents/Galpi/팀미팅",
      }),
      cancel: unavailable,
      openArtifact: unavailable,
      revealOutput: unavailable,
      startRecording: unavailable,
      stopRecording: unavailable,
      cancelRecording: unavailable,
      listenToRecordingFailures: async () => () => undefined,
      chooseAudio: unavailable,
      chooseTranscript: async (defaultPath: string | null) => {
        capturedDefaultPath = defaultPath
        return "/tmp/팀미팅.txt"
      },
      chooseOutputDirectory: unavailable,
      openModelAccessPage: unavailable,
      listenToJobs: async () => () => undefined,
    }
    const controller = new AppController(backend as unknown as BackendPort, new AppView(root))
    await controller.start()

    // When: the user imports an existing transcript
    ;(root.querySelector('[data-action="import-transcript"]') as HTMLElement).click()
    await new Promise((resolve) => setTimeout(resolve, 0))
    await new Promise((resolve) => setTimeout(resolve, 0))

    // Then: the transcript renders as the meeting result and augmentation unlocks
    expect(capturedDefaultPath).toBe("/tmp/Documents/Galpi")
    expect((root.querySelector("#transcript-selection") as HTMLElement).dataset["selected"]).toBe(
      "true",
    )
    expect((root.querySelector("#result-txt") as HTMLElement).textContent).toBe(
      "/tmp/Documents/Galpi/팀미팅/팀미팅.txt",
    )
    expect((root.querySelector("#result-srt-row") as HTMLElement).hidden).toBe(true)
    expect((root.querySelector("#result-checkpoint-row") as HTMLElement).hidden).toBe(true)
    expect((root.querySelector("#refine-button") as HTMLButtonElement).disabled).toBe(false)
    controller.stop()
  })
})

describe("AppController compute device", () => {
  test("switching the device saves it and re-diagnoses the environment", async () => {
    // Given: a Windows-like host where whisperx is the only preset and the
    // diagnosed device follows the saved choice
    const window = new Window()
    const sheet = window.document.createElement("style")
    sheet.textContent = styles
    window.document.head.appendChild(sheet)
    const root = window.document.createElement("div") as unknown as HTMLElement
    window.document.body.appendChild(root as unknown as never)
    const unavailable = () => Promise.reject(new Error("unused"))
    const state: { device: "cpu" | "cuda"; saved: string[]; diagnoses: number } = {
      device: "cpu",
      saved: [],
      diagnoses: 0,
    }
    const backend = {
      diagnose: async () => {
        state.diagnoses += 1
        return {
          enginePreset: "whisperx" as const,
          engineReady: true,
          modelsReady: true,
          ffmpegReady: true,
          qwen3Ready: false,
          whisperxReady: true,
          dataDirectory: "C:\\Galpi",
          defaultOutputDirectory: "C:\\Galpi\\out",
          engineVersion: "test",
          computeDevice: state.device,
          availablePresets: ["whisperx"] as const,
          availableDevices: ["cpu", "cuda"] as const,
          cudaDriverDetected: true,
        }
      },
      prepare: unavailable,
      huggingFaceTokenStored: async () => false,
      saveHuggingFaceToken: unavailable,
      saveAssistantApiKey: unavailable,
      loadAssistantSettings: async () => ({
        apiKeyStored: false,
        model: null,
        baseUrl: null,
        background: null,
      }),
      saveAssistantSettings: unavailable,
      saveEnginePreset: unavailable,
      saveComputeDevice: async (device: "cpu" | "cuda") => {
        state.saved.push(device)
        state.device = device
      },
      refineTranscript: unavailable,
      transcribe: unavailable,
      importTranscript: unavailable,
      listenToJobs: async () => () => undefined,
    }
    const controller = new AppController(backend as unknown as BackendPort, new AppView(root))
    await controller.start()
    const before = state.diagnoses

    // When
    ;(root.querySelector('input[name="compute-device"][value="cuda"]') as HTMLInputElement).click()
    await new Promise((resolve) => setTimeout(resolve, 0))
    await new Promise((resolve) => setTimeout(resolve, 0))

    // Then: saved first, then diagnosed again
    expect(state.saved).toEqual(["cuda"])
    expect(state.diagnoses).toBe(before + 1)
    expect(
      (root.querySelector('input[name="compute-device"][value="cuda"]') as HTMLInputElement)
        .checked,
    ).toBe(true)
    controller.stop()
  })
})
