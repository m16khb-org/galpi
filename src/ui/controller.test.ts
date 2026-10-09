import { describe, expect, test } from "bun:test"
import { Window } from "happy-dom"

import type { AppAccess, BackendPort } from "../domain/backend"
import type { ChatGptModel, ChatGptSettings } from "../domain/chatgpt"
import { AppView } from "./app-view"
import { AppController } from "./controller"

const styles = await Bun.file(new URL("../styles.css", import.meta.url)).text()

const signedOutChatGpt: ChatGptSettings = {
  authMode: "apiKey",
  model: null,
  welcomeAcknowledged: false,
  account: { state: "signedOut", email: null },
}

const openAccess: AppAccess = { state: "signedIn", email: "dev@example.com", offline: false }

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
    loadChatGptSettings: unavailable,
    saveChatGptPreferences: unavailable,
    signInWithChatGpt: unavailable,
    cancelChatGptSignIn: unavailable,
    listChatGptModels: unavailable,
    signOutOfChatGpt: unavailable,
    openChatGptUsagePage: unavailable,
    listenToChatGptEvents: unavailable,
    loadAppAccess: unavailable,
    signInToGateway: unavailable,
    cancelGatewaySignIn: unavailable,
    signOutOfGateway: unavailable,
  }
}

function createHarness(backend: BackendPort): { controller: AppController; root: HTMLElement } {
  const window = new Window()
  const sheet = window.document.createElement("style")
  sheet.textContent = styles
  window.document.head.appendChild(sheet)
  const root = window.document.createElement("div") as unknown as HTMLElement
  window.document.body.appendChild(root as unknown as never)
  return { controller: new AppController(backend, new AppView(root)), root }
}

describe("AppController startup without a native runtime", () => {
  test("the login screen asks for a restart and never starts a sign-in", async () => {
    // Given: every backend call, starting with the event subscription, rejects
    let signIns = 0
    const { controller, root } = createHarness({
      ...unavailableBackend(),
      signInToGateway: () => {
        signIns += 1
        return Promise.reject(new Error("no native runtime"))
      },
    })

    // When: the window starts and the login action fires anyway
    await controller.start()
    ;(root.querySelector('[data-action="sign-in-gateway"]') as HTMLElement).click()
    for (let tick = 0; tick < 10; tick += 1) await Promise.resolve()

    // Then: the failure is on the login screen, which offers nothing but a restart
    expect((root.querySelector("#access-screen") as HTMLElement).hidden).toBe(false)
    const status = root.querySelector("#access-status") as HTMLElement
    expect(status.textContent).toContain("네이티브 런타임")
    expect(status.dataset["state"]).toBe("error")
    expect((root.querySelector("#access-sign-in-button") as HTMLElement).hidden).toBe(true)
    expect((root.querySelector("#access-retry-button") as HTMLElement).hidden).toBe(true)
    expect((root.querySelector(".app-shell") as HTMLElement).hasAttribute("inert")).toBe(true)
    expect(signIns).toBe(0)
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
      loadChatGptSettings: async () => signedOutChatGpt,
      saveChatGptPreferences: unavailable,
      signInWithChatGpt: unavailable,
      cancelChatGptSignIn: unavailable,
      listChatGptModels: unavailable,
      signOutOfChatGpt: unavailable,
      openChatGptUsagePage: unavailable,
      listenToChatGptEvents: async () => () => undefined,
      loadAppAccess: async () => openAccess,
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
      loadChatGptSettings: async () => signedOutChatGpt,
      saveChatGptPreferences: unavailable,
      signInWithChatGpt: unavailable,
      cancelChatGptSignIn: unavailable,
      listChatGptModels: unavailable,
      signOutOfChatGpt: unavailable,
      openChatGptUsagePage: unavailable,
      listenToChatGptEvents: async () => () => undefined,
      loadAppAccess: async () => openAccess,
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

describe("AppController ChatGPT sign-in", () => {
  const signedIn: ChatGptSettings = {
    authMode: "chatGpt",
    model: null,
    welcomeAcknowledged: true,
    account: { state: "signedIn", email: "dev@example.com" },
  }

  // Microtask drain instead of a wall-clock wait: the controller resumes
  // after each settled backend promise.
  async function settle(): Promise<void> {
    for (let tick = 0; tick < 25; tick += 1) await Promise.resolve()
  }

  function nativeBackend(overrides: Partial<BackendPort>, calls: string[] = []): BackendPort {
    const record =
      <T>(name: string, value: T) =>
      async (): Promise<T> => {
        calls.push(name)
        return value
      }
    return {
      ...unavailableBackend(),
      diagnose: async () => ({
        enginePreset: "qwen3" as const,
        engineReady: true,
        modelsReady: true,
        ffmpegReady: true,
        qwen3Ready: true,
        whisperxReady: false,
        dataDirectory: "/tmp/galpi",
        defaultOutputDirectory: "/tmp/galpi/out",
        engineVersion: "test",
        computeDevice: "cpu" as const,
        availablePresets: ["qwen3", "whisperx"] as const,
        availableDevices: [] as const,
        cudaDriverDetected: false,
      }),
      huggingFaceTokenStored: async () => false,
      loadAssistantSettings: async () => ({
        apiKeyStored: false,
        model: null,
        baseUrl: null,
        reasoningEffort: null,
        background: null,
        participants: [],
        glossary: [],
      }),
      saveAssistantSettings: async () => undefined,
      listenToJobs: async () => () => undefined,
      listenToRecordingFailures: async () => () => undefined,
      listenToChatGptEvents: async () => {
        calls.push("listenToChatGptEvents")
        return () => undefined
      },
      loadChatGptSettings: record("loadChatGptSettings", signedOutChatGpt),
      listChatGptModels: record("listChatGptModels", [
        { slug: "gpt-5.5", displayName: "GPT-5.5" },
      ]),
      loadAppAccess: async () => openAccess,
      ...overrides,
    }
  }

  function mount(backend: BackendPort): { controller: AppController; root: HTMLElement } {
    const window = new Window()
    const root = window.document.createElement("div") as unknown as HTMLElement
    window.document.body.appendChild(root as unknown as never)
    return { controller: new AppController(backend, new AppView(root)), root }
  }

  test("subscribes to sign-in events before any sign-in call", async () => {
    // Given: a backend that records the order of the calls that matter
    const calls: string[] = []
    const backend = nativeBackend(
      {
        signInWithChatGpt: async () => {
          calls.push("signInWithChatGpt")
          return signedIn
        },
      },
      calls,
    )
    const { controller, root } = mount(backend)

    // When: the window starts and the user signs in
    await controller.start()
    ;(root.querySelector('[data-action="sign-in-chatgpt"]') as HTMLElement).click()
    await settle()

    // Then: the listener was in place first
    expect(calls.indexOf("listenToChatGptEvents")).toBeGreaterThanOrEqual(0)
    expect(calls.indexOf("listenToChatGptEvents")).toBeLessThan(calls.indexOf("signInWithChatGpt"))
    controller.stop()
  })

  test("lists the account's models when settings open while signed in", async () => {
    // Given
    const calls: string[] = []
    const backend = nativeBackend({ loadChatGptSettings: async () => signedIn }, calls)
    const { controller, root } = mount(backend)
    await controller.start()
    expect(calls).not.toContain("listChatGptModels")

    // When
    ;(root.querySelector('[data-action="open-settings"]') as HTMLElement).click()
    await settle()

    // Then
    expect(calls).toContain("listChatGptModels")
    expect((root.querySelector("#settings-chatgpt-model") as HTMLSelectElement).value).toBe(
      "gpt-5.5",
    )
    controller.stop()
  })

  test("does not list models while signed out", async () => {
    // Given
    const calls: string[] = []
    const { controller, root } = mount(nativeBackend({}, calls))
    await controller.start()

    // When
    ;(root.querySelector('[data-action="open-settings"]') as HTMLElement).click()
    await settle()

    // Then
    expect(calls).not.toContain("listChatGptModels")
    controller.stop()
  })

  test("keeps the sheet editable while the model list is still loading", async () => {
    // Given: a signed-in account whose model list request never settles
    const calls: string[] = []
    const backend = nativeBackend(
      {
        loadChatGptSettings: async () => signedIn,
        listChatGptModels: () => {
          calls.push("listChatGptModels")
          return Promise.withResolvers<readonly ChatGptModel[]>().promise
        },
      },
      calls,
    )
    const { controller, root } = mount(backend)
    await controller.start()

    // When
    ;(root.querySelector('[data-action="open-settings"]') as HTMLElement).click()
    await settle()

    // Then: the remote request is out, but the local fields are already usable
    expect(calls).toContain("listChatGptModels")
    expect(
      (root.querySelector("#settings-assistant-background") as HTMLTextAreaElement).disabled,
    ).toBe(false)
    controller.stop()
  })

  test("lists models only once the ChatGPT mode is chosen", async () => {
    // Given: signed in to ChatGPT but using the API key mode
    const calls: string[] = []
    const backend = nativeBackend(
      {
        loadChatGptSettings: async () => ({ ...signedIn, authMode: "apiKey" }),
        saveChatGptPreferences: async () => undefined,
        saveAssistantSettings: async () => undefined,
      },
      calls,
    )
    const { controller, root } = mount(backend)
    await controller.start()
    ;(root.querySelector('[data-action="open-settings"]') as HTMLElement).click()
    await settle()
    expect(calls).not.toContain("listChatGptModels")

    // When
    ;(
      root.querySelector('input[name="assistant-auth-mode"][value="chatGpt"]') as HTMLInputElement
    ).click()
    await settle()

    // Then
    expect(calls).toContain("listChatGptModels")
    controller.stop()
  })

  test("a sheet that failed to load neither lists models nor autosaves", async () => {
    // Given: start-up loads fine, but the next settings load fails
    const calls: string[] = []
    let loads = 0
    const backend = nativeBackend(
      {
        loadChatGptSettings: async () => signedIn,
        loadAssistantSettings: async () => {
          loads += 1
          if (loads > 1) throw { code: "SETTINGS_INVALID", message: "설정을 읽지 못했습니다." }
          return {
            apiKeyStored: false,
            model: "my-model",
            baseUrl: "https://llm.example",
            reasoningEffort: null,
            background: "team context",
            participants: [],
            glossary: [],
          }
        },
        saveAssistantSettings: async () => {
          calls.push("saveAssistantSettings")
        },
        saveChatGptPreferences: async () => {
          calls.push("saveChatGptPreferences")
        },
      },
      calls,
    )
    const { controller, root } = mount(backend)
    await controller.start()

    // When
    ;(root.querySelector('[data-action="open-settings"]') as HTMLElement).click()
    await settle()

    // Then: the load error stays and nothing reaches the host
    expect(calls).not.toContain("listChatGptModels")
    expect(calls).not.toContain("saveAssistantSettings")
    expect(calls).not.toContain("saveChatGptPreferences")
    expect((root.querySelector("#settings-message") as HTMLElement).textContent).toBe(
      "설정을 읽지 못했습니다.",
    )
    controller.stop()
  })

  test("a failed sign-in is shown once and never retried", async () => {
    // Given: the host rejects the sign-in
    let signInCalls = 0
    const backend = nativeBackend({
      signInWithChatGpt: async () => {
        signInCalls += 1
        throw { code: "CHATGPT_SIGN_IN_TIMEOUT", message: "로그인 시간이 지났습니다." }
      },
    })
    const { controller, root } = mount(backend)
    await controller.start()

    // When
    ;(root.querySelector('[data-action="sign-in-chatgpt"]') as HTMLElement).click()
    await settle()
    await settle()

    // Then: one call, one visible message, no automatic second attempt
    expect(signInCalls).toBe(1)
    expect((root.querySelector("#chatgpt-message") as HTMLElement).textContent).toBe(
      "로그인 시간이 지났습니다.",
    )
    controller.stop()
  })

  test("a model list failure stays in the sheet message and leaves sign-in intact", async () => {
    // Given: the account is signed in but the list endpoint fails
    const backend = nativeBackend({
      loadChatGptSettings: async () => signedIn,
      listChatGptModels: async () => {
        throw { code: "CHATGPT_MODELS_UNAVAILABLE", message: "모델 목록을 불러오지 못했습니다." }
      },
    })
    const { controller, root } = mount(backend)
    await controller.start()

    // When
    ;(root.querySelector('[data-action="open-settings"]') as HTMLElement).click()
    await settle()

    // Then
    const message = root.querySelector("#chatgpt-message") as HTMLElement
    expect(message.textContent).toBe("모델 목록을 불러오지 못했습니다.")
    expect(message.dataset["state"]).toBe("error")
    expect((root.querySelector("#chatgpt-status") as HTMLElement).textContent).toBe(
      "dev@example.com로 로그인됨",
    )
    expect((root.querySelector("#app-error") as HTMLElement).hidden).toBe(true)
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
    const state: { device: "cpu" | "cuda"; saved: string[]; diagnoses: number } = {
      device: "cpu",
      saved: [],
      diagnoses: 0,
    }
    const backend: BackendPort = {
      ...unavailableBackend(),
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
      huggingFaceTokenStored: async () => false,
      loadAssistantSettings: async () => ({
        apiKeyStored: false,
        model: null,
        baseUrl: null,
        reasoningEffort: null,
        background: null,
        participants: [],
        glossary: [],
      }),
      saveComputeDevice: async (device) => {
        state.saved.push(device)
        state.device = device
      },
      loadChatGptSettings: async () => ({
        authMode: "apiKey",
        model: null,
        welcomeAcknowledged: true,
        account: { state: "signedOut", email: null },
      }),
      listenToJobs: async () => () => undefined,
      listenToRecordingFailures: async () => () => undefined,
      listenToChatGptEvents: async () => () => undefined,
      loadAppAccess: async () => openAccess,
    }
    const controller = new AppController(backend, new AppView(root))
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
      (root.querySelector('input[name="compute-device"][value="cuda"]') as HTMLInputElement).checked,
    ).toBe(true)
    controller.stop()
  })
})
