import { describe, expect, test } from "bun:test"
import { Window } from "happy-dom"

import type { BackendPort } from "../domain/backend"
import type { AssistantSettings } from "../domain/job"
import type { ChatGptPreferences, ChatGptSettings } from "../domain/chatgpt"
import { AppView } from "./app-view"
import { AppController } from "./controller"

const initialAssistant: AssistantSettings = {
  apiKeyStored: true,
  model: "glm-5.3-flash",
  baseUrl: null,
  reasoningEffort: "max",
  background: null,
  participants: [
    {
      id: "person-1",
      name: "하빈",
      team: "devops",
      role: "팀원",
      description: null,
      aliases: [],
    },
  ],
  glossary: [{ id: "term-1", term: "갈피", description: null }],
}

const signedOutChatGpt: ChatGptSettings = {
  authMode: "apiKey",
  model: null,
  welcomeAcknowledged: false,
  account: { state: "signedOut", email: null },
}

function createBackend(
  onSave: (settings: AssistantSettings) => void | Promise<void>,
  onSaveKey: (key: string) => void = () => undefined,
  onSavePreferences: (preferences: ChatGptPreferences) => void = () => undefined,
  chatGpt: ChatGptSettings = signedOutChatGpt,
): BackendPort {
  return {
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
    }),
    prepare: async () => {
      throw new Error("unused prepare")
    },
    huggingFaceTokenStored: async () => false,
    saveHuggingFaceToken: async () => undefined,
    loadAssistantSettings: async () => initialAssistant,
    saveAssistantApiKey: async (key) => {
      onSaveKey(key)
    },
    saveEnginePreset: async () => undefined,
    saveAssistantSettings: async (settings) => {
      await onSave(settings)
    },
    refineTranscript: async () => {
      throw new Error("unused refinement")
    },
    transcribe: async () => {
      throw new Error("unused transcription")
    },
    importTranscript: async () => {
      throw new Error("unused import")
    },
    cancel: async () => undefined,
    openArtifact: async () => undefined,
    revealOutput: async () => undefined,
    startRecording: async () => {
      throw new Error("unused recording")
    },
    stopRecording: async () => {
      throw new Error("unused recording")
    },
    cancelRecording: async () => undefined,
    listenToRecordingFailures: async () => () => undefined,
    chooseAudio: async () => null,
    chooseTranscript: async () => null,
    chooseOutputDirectory: async () => null,
    openModelAccessPage: async () => undefined,
    listenToJobs: async () => () => undefined,
    loadChatGptSettings: async () => chatGpt,
    saveChatGptPreferences: async (preferences) => {
      onSavePreferences(preferences)
    },
    signInWithChatGpt: async () => {
      throw new Error("unused sign-in")
    },
    cancelChatGptSignIn: async () => undefined,
    listChatGptModels: async () => [
      { slug: "gpt-5.5", displayName: "GPT-5.5" },
      { slug: "gpt-5.4-mini", displayName: "GPT-5.4 mini" },
    ],
    signOutOfChatGpt: async () => {
      throw new Error("unused sign-out")
    },
    openChatGptUsagePage: async () => undefined,
    listenToChatGptEvents: async () => () => undefined,
  }
}

function withTimeout<T>(value: Promise<T>): Promise<T> {
  return Promise.race([
    value,
    new Promise<never>((_resolve, reject) => {
      setTimeout(() => reject(new Error("settings autosave timed out")), 200)
    }),
  ])
}

function deferred<T>(): {
  readonly promise: Promise<T>
  readonly resolve: (value: T) => void
} {
  let resolver: ((value: T) => void) | undefined
  const promise = new Promise<T>((resolve) => {
    resolver = resolve
  })
  return {
    promise,
    resolve: (value) => {
      if (resolver === undefined) throw new Error("deferred resolver is missing")
      resolver(value)
    },
  }
}

function dispatchChange(window: Window, element: HTMLElement): void {
  element.dispatchEvent(new window.Event("change", { bubbles: true }) as unknown as Event)
}

async function createHarness(
  onSave: (settings: AssistantSettings) => void | Promise<void>,
  onSaveKey: (key: string) => void = () => undefined,
  onSavePreferences: (preferences: ChatGptPreferences) => void = () => undefined,
  chatGpt: ChatGptSettings = signedOutChatGpt,
): Promise<{
  readonly window: Window
  readonly root: HTMLElement
  readonly view: AppView
  readonly controller: AppController
}> {
  const window = new Window()
  const root = window.document.createElement("div") as unknown as HTMLElement
  window.document.body.appendChild(root as unknown as never)
  const view = new AppView(root)
  const controller = new AppController(
    createBackend(onSave, onSaveKey, onSavePreferences, chatGpt),
    view,
  )
  await controller.start()
  view.assistantSettings.setSettings(initialAssistant)
  return { window, root, view, controller }
}

describe("settings autosave (real DOM)", () => {
  test("persists a committed settings edit without a save button", async () => {
    // Given
    const saved = deferred<AssistantSettings>()
    const { window, root, controller } = await createHarness((settings) => saved.resolve(settings))
    const model = root.querySelector<HTMLInputElement>("#settings-assistant-model")
    if (model === null) throw new Error("model field is missing")

    // When
    model.value = "glm-5-turbo"
    dispatchChange(window, model)

    // Then
    const persisted = await withTimeout(saved.promise)
    expect(persisted.model).toBe("glm-5-turbo")
    expect(root.querySelector('[data-action="save-token"]')).toBeNull()
    expect(root.querySelector("#settings-message")?.textContent).toContain("자동 저장")
    controller.stop()
  })

  test("never rewrites a stored assistant key when another field changes", async () => {
    // Given: a stored key the window never received, and an unrelated edit
    const saved = deferred<AssistantSettings>()
    const keyWrites: string[] = []
    const { window, root, controller } = await createHarness(
      (settings) => saved.resolve(settings),
      (key) => keyWrites.push(key),
    )
    const background = root.querySelector<HTMLTextAreaElement>("#settings-assistant-background")
    if (background === null) throw new Error("background field is missing")

    // When
    background.value = "팀리더: 하빈"
    dispatchChange(window, background)

    // Then: the key was left alone and is still reported as saved
    const persisted = await withTimeout(saved.promise)
    expect(keyWrites).toEqual([])
    expect(persisted.apiKeyStored).toBeTrue()
    expect(persisted.background).toBe("팀리더: 하빈")
    expect(root.querySelector("#assistant-configured-state")?.textContent).toBe("API 키 저장됨")
    controller.stop()
  })

  test("sends a newly typed assistant key on its own command", async () => {
    // Given: no key on file yet
    const saved = deferred<AssistantSettings>()
    const keyWrites: string[] = []
    const { window, root, view, controller } = await createHarness(
      (settings) => saved.resolve(settings),
      (key) => keyWrites.push(key),
    )
    const key = root.querySelector<HTMLInputElement>("#settings-assistant-key")
    if (key === null) throw new Error("assistant key field is missing")
    view.assistantSettings.clearKey()

    // When
    key.value = "  zai_typed  "
    dispatchChange(window, key)

    // Then
    await withTimeout(saved.promise)
    expect(keyWrites).toEqual(["zai_typed"])
    controller.stop()
  })

  test("persists participant removal immediately", async () => {
    // Given
    const saved = deferred<AssistantSettings>()
    const { root, controller } = await createHarness((settings) => saved.resolve(settings))
    const remove = root.querySelector<HTMLButtonElement>(".participant-remove")
    if (remove === null) throw new Error("participant remove button is missing")

    // When
    remove.click()

    // Then
    expect((await withTimeout(saved.promise)).participants).toEqual([])
    controller.stop()
  })

  test("persists glossary edits when the field change is committed", async () => {
    // Given
    const saved = deferred<AssistantSettings>()
    const { window, root, controller } = await createHarness((settings) => saved.resolve(settings))
    const term = root.querySelector<HTMLInputElement>(".glossary-term")
    if (term === null) throw new Error("glossary term field is missing")

    // When
    term.value = "화자분리"
    dispatchChange(window, term)

    // Then
    expect((await withTimeout(saved.promise)).glossary[0]?.term).toBe("화자분리")
    controller.stop()
  })

  test("coalesces edits made while a save is active", async () => {
    // Given
    const firstGate = deferred<void>()
    const firstStarted = deferred<void>()
    const secondSaved = deferred<AssistantSettings>()
    let saves = 0
    const { window, root, controller } = await createHarness(async (settings) => {
      saves += 1
      if (saves === 1) {
        firstStarted.resolve(undefined)
        await firstGate.promise
      } else {
        secondSaved.resolve(settings)
      }
    })
    const model = root.querySelector<HTMLInputElement>("#settings-assistant-model")
    if (model === null) throw new Error("model field is missing")

    // When
    model.value = "glm-5.2"
    dispatchChange(window, model)
    await withTimeout(firstStarted.promise)
    model.value = "glm-5-turbo"
    dispatchChange(window, model)
    firstGate.resolve(undefined)

    // Then
    expect((await withTimeout(secondSaved.promise)).model).toBe("glm-5-turbo")
    expect(saves).toBe(2)
    controller.stop()
  })

  test("preserves edits and shows the error when autosave fails", async () => {
    // Given
    const attempted = deferred<void>()
    const { window, root, controller } = await createHarness(() => {
      attempted.resolve(undefined)
      throw new Error("settings write failed")
    })
    const model = root.querySelector<HTMLInputElement>("#settings-assistant-model")
    if (model === null) throw new Error("model field is missing")

    // When
    model.value = "glm-5-turbo"
    dispatchChange(window, model)
    await withTimeout(attempted.promise)
    await Promise.resolve()

    // Then
    expect(model.value).toBe("glm-5-turbo")
    expect(model.disabled).toBeFalse()
    expect(root.querySelector("#settings-message")?.textContent).toContain(
      "수정 내용은 유지되며 다음 변경 때 다시 저장합니다.",
    )
    controller.stop()
  })
})

// The controller continues in microtasks after a backend promise settles;
// draining a few of them lets the view catch up without a wall-clock wait.
async function settle(): Promise<void> {
  for (let tick = 0; tick < 25; tick += 1) await Promise.resolve()
}

describe("ChatGPT preferences autosave (real DOM)", () => {
  const signedIn: ChatGptSettings = {
    authMode: "chatGpt",
    model: "gpt-5.5",
    welcomeAcknowledged: true,
    account: { state: "signedIn", email: "dev@example.com" },
  }

  test("choosing ChatGPT saves only the preferences and leaves the assistant payload alone", async () => {
    // Given: a signed-out window in API key mode
    const assistantSaves: AssistantSettings[] = []
    const keySaves: string[] = []
    const preferenceSaves: ChatGptPreferences[] = []
    const preferencesSaved = deferred<void>()
    const { root, controller, view } = await createHarness(
      (settings) => void assistantSaves.push(settings),
      (key) => void keySaves.push(key),
      (preferences) => {
        preferenceSaves.push(preferences)
        preferencesSaved.resolve(undefined)
      },
    )

    // When: the user switches the augmentation mode to ChatGPT
    const radio = root.querySelector<HTMLInputElement>(
      'input[name="assistant-auth-mode"][value="chatGpt"]',
    )
    if (radio === null) throw new Error("ChatGPT mode radio is missing")
    radio.click()
    await withTimeout(preferencesSaved.promise)
    await settle()

    // Then: the choice went to its own command with no account data
    expect(preferenceSaves).toEqual([
      { authMode: "chatGpt", model: null, welcomeAcknowledged: false },
    ])
    // And the assistant document and key travel exactly as before
    expect(keySaves).toEqual([])
    expect(assistantSaves).toHaveLength(1)
    expect(assistantSaves[0]).toEqual({
      apiKeyStored: true,
      model: "glm-5.3-flash",
      baseUrl: null,
      reasoningEffort: "max",
      background: null,
      participants: initialAssistant.participants,
      glossary: initialAssistant.glossary,
    })
    // And ChatGPT mode without an account keeps the augment stage locked
    expect(root.querySelector<HTMLElement>("#augment-key-hint")?.hidden).toBe(false)
    expect(view.chatGptSettings.authMode()).toBe("chatGpt")
    controller.stop()
  })

  test("picking another model saves it as a preference and nothing else", async () => {
    // Given: a signed-in account whose list carries two models
    const assistantSaves: AssistantSettings[] = []
    const preferenceSaves: ChatGptPreferences[] = []
    const preferencesSaved = deferred<void>()
    const { window, root, controller } = await createHarness(
      (settings) => void assistantSaves.push(settings),
      () => undefined,
      (preferences) => {
        preferenceSaves.push(preferences)
        preferencesSaved.resolve(undefined)
      },
      signedIn,
    )
    ;(root.querySelector('[data-action="open-settings"]') as HTMLElement).click()
    await settle()
    const select = root.querySelector<HTMLSelectElement>("#settings-chatgpt-model")
    if (select === null) throw new Error("ChatGPT model select is missing")
    expect(select.value).toBe("gpt-5.5")
    expect(preferenceSaves).toEqual([])

    // When
    select.value = "gpt-5.4-mini"
    dispatchChange(window, select)
    await withTimeout(preferencesSaved.promise)

    // Then
    expect(preferenceSaves).toEqual([
      { authMode: "chatGpt", model: "gpt-5.4-mini", welcomeAcknowledged: true },
    ])
    expect(assistantSaves.at(-1)?.model).toBe("glm-5.3-flash")
    controller.stop()
  })
})
