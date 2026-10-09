import { describe, expect, test } from "bun:test"
import { Window } from "happy-dom"

import type { BackendPort } from "../domain/backend"
import type {
  ChatGptModel,
  ChatGptSettings,
  ChatGptSignInPhase,
  ChatGptSignOutResult,
} from "../domain/chatgpt"
import type { AssistantSettings } from "../domain/job"
import { AppView } from "./app-view"
import { AppController } from "./controller"

const assistant: AssistantSettings = {
  apiKeyStored: false,
  model: null,
  baseUrl: null,
  reasoningEffort: null,
  background: null,
  participants: [],
  glossary: [],
}

const signedOut: ChatGptSettings = {
  authMode: "apiKey",
  model: null,
  welcomeAcknowledged: false,
  account: { state: "signedOut", email: null },
}

const signedIn: ChatGptSettings = {
  authMode: "chatGpt",
  model: null,
  welcomeAcknowledged: false,
  account: { state: "signedIn", email: "dev@example.com" },
}

const models: readonly ChatGptModel[] = [
  { slug: "gpt-5.5", displayName: "GPT-5.5" },
  { slug: "gpt-5.4-mini", displayName: "GPT-5.4 mini" },
]

// The controller continues in microtasks after a backend promise settles;
// draining a few of them lets the view catch up without a wall-clock wait.
async function settle(): Promise<void> {
  for (let tick = 0; tick < 25; tick += 1) await Promise.resolve()
}

function deferred<T>(): {
  readonly promise: Promise<T>
  readonly resolve: (value: T) => void
  readonly reject: (error: unknown) => void
} {
  return Promise.withResolvers<T>()
}

function backendWith(overrides: Partial<BackendPort>): BackendPort {
  const unused = () => Promise.reject(new Error("unused"))
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
      computeDevice: "cpu" as const,
      availablePresets: ["qwen3", "whisperx"] as const,
      availableDevices: [] as const,
      cudaDriverDetected: false,
    }),
    prepare: unused,
    huggingFaceTokenStored: async () => false,
    saveHuggingFaceToken: async () => undefined,
    loadAssistantSettings: async () => assistant,
    saveAssistantApiKey: async () => undefined,
    saveAssistantSettings: async () => undefined,
    saveEnginePreset: async () => undefined,
    saveComputeDevice: async () => undefined,
    refineTranscript: unused,
    transcribe: unused,
    importTranscript: unused,
    cancel: async () => undefined,
    openArtifact: async () => undefined,
    revealOutput: async () => undefined,
    startRecording: unused,
    stopRecording: unused,
    cancelRecording: async () => undefined,
    listenToRecordingFailures: async () => () => undefined,
    chooseAudio: async () => null,
    chooseTranscript: async () => null,
    chooseOutputDirectory: async () => null,
    openModelAccessPage: async () => undefined,
    listenToJobs: async () => () => undefined,
    loadChatGptSettings: async () => signedOut,
    saveChatGptPreferences: async () => undefined,
    signInWithChatGpt: unused,
    cancelChatGptSignIn: async () => undefined,
    listChatGptModels: async () => models,
    signOutOfChatGpt: unused,
    openChatGptUsagePage: async () => undefined,
    listenToChatGptEvents: async () => () => undefined,
    ...overrides,
  }
}

async function harness(overrides: Partial<BackendPort>): Promise<{
  readonly root: HTMLElement
  readonly controller: AppController
}> {
  const window = new Window()
  const root = window.document.createElement("div") as unknown as HTMLElement
  window.document.body.appendChild(root as unknown as never)
  const controller = new AppController(backendWith(overrides), new AppView(root))
  await controller.start()
  return { root, controller }
}

function pick<T extends HTMLElement>(root: HTMLElement, selector: string): T {
  const element = root.querySelector<T>(selector)
  if (element === null) throw new Error(`missing ${selector}`)
  return element
}

function click(root: HTMLElement, action: string): void {
  pick(root, `[data-action="${action}"]`).click()
}

function text(root: HTMLElement, selector: string): string {
  return pick(root, selector).textContent ?? ""
}

function hidden(root: HTMLElement, selector: string): boolean {
  return pick(root, selector).hidden === true
}

function hiddenAncestor(element: HTMLElement): HTMLElement | null {
  for (let node: HTMLElement | null = element; node !== null; node = node.parentElement) {
    if (node.hidden) return node
  }
  return null
}

describe("ChatGPT settings panel (real DOM)", () => {
  test("shows the ChatGPT panel only in ChatGPT mode", async () => {
    // Given: a window that starts in API key mode
    const { root, controller } = await harness({})
    expect(hidden(root, "#assistant-chatgpt-panel")).toBe(true)
    expect(hidden(root, "#assistant-api-key-panel")).toBe(false)

    // When: the user picks ChatGPT
    pick<HTMLInputElement>(root, 'input[name="assistant-auth-mode"][value="chatGpt"]').click()

    // Then: the panels swap, and the API key field keeps its place in the document
    expect(hidden(root, "#assistant-chatgpt-panel")).toBe(false)
    expect(hidden(root, "#assistant-api-key-panel")).toBe(true)
    expect(root.querySelector("#settings-assistant-key")).not.toBeNull()

    // When: and back
    pick<HTMLInputElement>(root, 'input[name="assistant-auth-mode"][value="apiKey"]').click()

    // Then
    expect(hidden(root, "#assistant-chatgpt-panel")).toBe(true)
    expect(hidden(root, "#assistant-api-key-panel")).toBe(false)
    controller.stop()
  })

  test("offers the continue button and the data notice before sign-in", async () => {
    // Given / When
    const { root, controller } = await harness({})
    pick<HTMLInputElement>(root, 'input[name="assistant-auth-mode"][value="chatGpt"]').click()

    // Then
    expect(text(root, "#chatgpt-sign-in-button")).toBe("ChatGPT로 계속하기")
    expect(hidden(root, "#chatgpt-sign-in-button")).toBe(false)
    expect(hidden(root, "#chatgpt-sign-out-button")).toBe(true)
    expect(hidden(root, "#chatgpt-model-field")).toBe(true)
    expect(text(root, "#chatgpt-status")).toBe("ChatGPT에 로그인하지 않았습니다.")
    expect(text(root, "#chatgpt-data-notice")).toContain("OpenAI API로 전송됩니다")
    controller.stop()
  })

  test("signs in, shows the e-mail and fills the model list in server order", async () => {
    // Given: a sign-in that completes with an account and a two-model list
    const { root, controller } = await harness({
      signInWithChatGpt: async () => signedIn,
    })
    pick<HTMLInputElement>(root, 'input[name="assistant-auth-mode"][value="chatGpt"]').click()

    // When
    click(root, "sign-in-chatgpt")
    await settle()

    // Then: the status names the account in text, not just color
    expect(text(root, "#chatgpt-status")).toBe("dev@example.com로 로그인됨")
    expect(pick(root, "#chatgpt-status").dataset["state"]).toBe("signedIn")
    expect(hidden(root, "#chatgpt-sign-in-button")).toBe(true)
    expect(hidden(root, "#chatgpt-sign-out-button")).toBe(false)
    // And the select shows display names, carries slugs, and defaults to the first
    const select = pick<HTMLSelectElement>(root, "#settings-chatgpt-model")
    expect([...select.options].map((option) => option.textContent)).toEqual([
      "GPT-5.5",
      "GPT-5.4 mini",
    ])
    expect([...select.options].map((option) => option.value)).toEqual(["gpt-5.5", "gpt-5.4-mini"])
    expect(select.value).toBe("gpt-5.5")
    expect(hidden(root, "#chatgpt-model-field")).toBe(false)
    // And the augment stage is no longer waiting for an API key
    expect(hidden(root, "#augment-key-hint")).toBe(true)
    controller.stop()
  })

  test("keeps a saved model that the account still lists", async () => {
    // Given: the host remembers the second model
    const { root, controller } = await harness({
      loadChatGptSettings: async () => ({ ...signedIn, model: "gpt-5.4-mini" }),
    })

    // When: the sheet opens
    click(root, "open-settings")
    await settle()

    // Then
    expect(pick<HTMLSelectElement>(root, "#settings-chatgpt-model").value).toBe("gpt-5.4-mini")
    controller.stop()
  })

  test("shows a cancel button while the browser sign-in is pending", async () => {
    // Given: a sign-in the user has not finished in the browser
    const pending = deferred<ChatGptSettings>()
    let emit: (phase: ChatGptSignInPhase) => void = () => undefined
    let cancelCalls = 0
    const { root, controller } = await harness({
      signInWithChatGpt: () => pending.promise,
      cancelChatGptSignIn: async () => {
        cancelCalls += 1
        pending.reject({ code: "CANCELLED", message: "작업이 취소되었습니다." })
      },
      listenToChatGptEvents: async (handler) => {
        emit = handler
        return () => undefined
      },
    })
    pick<HTMLInputElement>(root, 'input[name="assistant-auth-mode"][value="chatGpt"]').click()

    // When
    click(root, "sign-in-chatgpt")
    await settle()

    // Then: waiting is stated in words and cancel replaces the continue button
    expect(text(root, "#chatgpt-message")).toContain("브라우저에서 ChatGPT 로그인을 마쳐 주세요")
    expect(hidden(root, "#chatgpt-cancel-button")).toBe(false)
    expect(hidden(root, "#chatgpt-sign-in-button")).toBe(true)

    // When: the host reports the code exchange
    emit("exchanging")

    // Then
    expect(text(root, "#chatgpt-message")).toBe("로그인을 마무리하는 중입니다.")
    expect(hidden(root, "#chatgpt-cancel-button")).toBe(false)

    // When: the user cancels
    click(root, "cancel-chatgpt-sign-in")
    await settle()

    // Then: back to the start without an error, and no second sign-in was attempted
    expect(cancelCalls).toBe(1)
    expect(hidden(root, "#chatgpt-cancel-button")).toBe(true)
    expect(hidden(root, "#chatgpt-sign-in-button")).toBe(false)
    expect(pick(root, "#chatgpt-message").dataset["state"]).toBe("ready")
    expect(text(root, "#chatgpt-message")).toBe("ChatGPT 로그인을 취소했습니다.")
    controller.stop()
  })

  test("returns to API key mode after sign-out", async () => {
    // Given: a signed-in account
    const result: ChatGptSignOutResult = {
      settings: signedOut,
      remoteRevocationConfirmed: true,
    }
    const { root, controller } = await harness({
      loadChatGptSettings: async () => signedIn,
      signOutOfChatGpt: async () => result,
    })
    expect(hidden(root, "#assistant-chatgpt-panel")).toBe(false)

    // When
    expect(text(root, "#chatgpt-sign-out-button")).toBe("ChatGPT 로그아웃")
    click(root, "sign-out-chatgpt")
    await settle()

    // Then: the API key panel is back and the account is gone from the sheet
    expect(hidden(root, "#assistant-api-key-panel")).toBe(false)
    expect(hidden(root, "#assistant-chatgpt-panel")).toBe(true)
    expect(
      pick<HTMLInputElement>(root, 'input[name="assistant-auth-mode"][value="apiKey"]').checked,
    ).toBe(true)
    expect(text(root, "#chatgpt-status")).toBe("ChatGPT에 로그인하지 않았습니다.")
    expect(hidden(root, "#chatgpt-model-field")).toBe(true)
    controller.stop()
  })

  test("says so when the server could not confirm the sign-out", async () => {
    // Given: the host removed the local session but the revocation call failed
    const { root, controller } = await harness({
      loadChatGptSettings: async () => signedIn,
      signOutOfChatGpt: async () => ({ settings: signedOut, remoteRevocationConfirmed: false }),
    })
    click(root, "open-settings")
    await settle()

    // When
    click(root, "sign-out-chatgpt")
    await settle()

    // Then: the notice lands in the sheet-wide status line, which stays visible
    // after the ChatGPT panel hides on the return to API key mode
    const message = pick(root, "#settings-message")
    expect(message.textContent).toContain("서버 쪽 연결 해제는 확인하지 못했")
    expect(message.dataset["state"]).toBe("error")
    expect(hidden(root, "#assistant-chatgpt-panel")).toBe(true)
    expect(hiddenAncestor(message)?.id ?? null).toBeNull()
    controller.stop()
  })

  test("ignores a model list that arrives after sign-out", async () => {
    // Given: the sheet opened while signed in and the model list is still on its way
    const list = deferred<readonly ChatGptModel[]>()
    const savedPreferences: unknown[] = []
    const { root, controller } = await harness({
      loadChatGptSettings: async () => signedIn,
      listChatGptModels: () => list.promise,
      saveChatGptPreferences: async (preferences) => {
        savedPreferences.push(preferences)
      },
      signOutOfChatGpt: async () => ({ settings: signedOut, remoteRevocationConfirmed: false }),
    })
    click(root, "open-settings")
    await settle()
    click(root, "sign-out-chatgpt")
    await settle()

    // When: the list resolves only now
    list.resolve(models)
    await settle()

    // Then: the sign-out notice stays and the cleared model is not saved back
    const message = pick(root, "#settings-message")
    expect(message.textContent).toContain("서버 쪽 연결 해제는 확인하지 못했")
    expect(message.dataset["state"]).toBe("error")
    expect(savedPreferences).toEqual([])
    controller.stop()
  })

  test("explains denied consent in Korean and does not call sign-in again", async () => {
    // Given: the user refused the plan-usage consent in the browser
    let signInCalls = 0
    const { root, controller } = await harness({
      signInWithChatGpt: async () => {
        signInCalls += 1
        throw { code: "CHATGPT_CONSENT_DENIED", message: "consent denied" }
      },
    })
    pick<HTMLInputElement>(root, 'input[name="assistant-auth-mode"][value="chatGpt"]').click()

    // When
    click(root, "sign-in-chatgpt")
    await settle()

    // Then
    expect(
      text(root, "#chatgpt-message").startsWith("ChatGPT 요금제 사용 동의가 거부되었습니다"),
    ).toBe(true)
    expect(pick(root, "#chatgpt-message").dataset["state"]).toBe("error")
    expect(signInCalls).toBe(1)
    // And the user can choose to try again themselves
    expect(hidden(root, "#chatgpt-sign-in-button")).toBe(false)
    controller.stop()
  })

  test("asks to sign in again when the session is no longer usable", async () => {
    // Given
    const { root, controller } = await harness({
      loadChatGptSettings: async () => ({
        ...signedIn,
        account: { state: "signInRequired", email: "dev@example.com" },
      }),
    })

    // Then: the state is spelled out and the button says what it will do
    expect(text(root, "#chatgpt-status")).toContain("다시 로그인")
    expect(text(root, "#chatgpt-sign-in-button")).toBe("다시 로그인")
    expect(hidden(root, "#augment-key-hint")).toBe(false)
    controller.stop()
  })

  test("shows the first sign-in notice until it is acknowledged", async () => {
    // Given: a signed-in account that has not seen the notice
    const saved: unknown[] = []
    const { root, controller } = await harness({
      loadChatGptSettings: async () => signedIn,
      saveChatGptPreferences: async (preferences) => void saved.push(preferences),
    })
    expect(hidden(root, "#chatgpt-welcome")).toBe(false)

    // When
    click(root, "acknowledge-chatgpt-welcome")
    await settle()

    // Then: it is dismissed and remembered as a preference
    expect(hidden(root, "#chatgpt-welcome")).toBe(true)
    expect(saved).toEqual([{ authMode: "chatGpt", model: null, welcomeAcknowledged: true }])
    controller.stop()
  })

  test("shows the usage-management action when the plan limit stops a refinement", async () => {
    // Given: an imported transcript and a refinement that hits the ChatGPT limit
    let refineCalls = 0
    let usagePageOpens = 0
    const { root, controller } = await harness({
      loadChatGptSettings: async () => signedIn,
      chooseTranscript: async () => "/tmp/팀미팅.txt",
      importTranscript: async () => ({
        jobId: "job-import-1",
        txt: "/tmp/팀미팅.txt",
        outputDirectory: "/tmp/out",
      }),
      refineTranscript: async () => {
        refineCalls += 1
        throw {
          code: "CHATGPT_USAGE_LIMIT_EXCEEDED",
          message: "ChatGPT 사용량 한도에 도달했습니다.",
        }
      },
      openChatGptUsagePage: async () => {
        usagePageOpens += 1
      },
    })
    expect(hidden(root, "#chatgpt-limit-hint")).toBe(true)
    click(root, "import-transcript")
    await settle()

    // When
    click(root, "refine")
    await settle()

    // Then: the hint is visible with "사용량 관리" as its primary action
    expect(hidden(root, "#chatgpt-limit-hint")).toBe(false)
    const action = pick<HTMLButtonElement>(root, "#chatgpt-limit-hint [data-action]")
    expect(action.textContent).toBe("사용량 관리")
    expect(action.classList.contains("primary-button")).toBe(true)
    // And nothing re-sent the request on its own
    expect(refineCalls).toBe(1)

    // When: the user follows the action
    action.click()
    await settle()

    // Then
    expect(usagePageOpens).toBe(1)
    expect(refineCalls).toBe(1)
    controller.stop()
  })
})
