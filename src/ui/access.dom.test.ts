import { describe, expect, test } from "bun:test"
import { Window } from "happy-dom"

import type { AppAccess, BackendPort } from "../domain/backend"
import { AppView } from "./app-view"
import { AppController } from "./controller"

const signedIn: AppAccess = { state: "signedIn", email: "dev@example.com", offline: false }

// The controller continues in microtasks after a backend promise settles.
async function settle(): Promise<void> {
  for (let tick = 0; tick < 25; tick += 1) await Promise.resolve()
}

/** A native runtime with a ready workspace; `calls` records gateway and workspace calls. */
function backendWith(overrides: Partial<BackendPort>, calls: string[]): BackendPort {
  const unused = () => Promise.reject(new Error("unused"))
  const record =
    <T>(name: string, value: T) =>
    async (): Promise<T> => {
      calls.push(name)
      return value
    }
  return {
    diagnose: record("diagnose", {
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
    saveHuggingFaceToken: unused,
    loadAssistantSettings: async () => ({
      apiKeyStored: false,
      model: null,
      baseUrl: null,
      reasoningEffort: null,
      background: null,
      participants: [],
      glossary: [],
    }),
    saveAssistantApiKey: unused,
    saveAssistantSettings: unused,
    saveEnginePreset: unused,
    saveComputeDevice: unused,
    refineTranscript: unused,
    transcribe: unused,
    importTranscript: unused,
    cancel: unused,
    openArtifact: unused,
    revealOutput: unused,
    startRecording: unused,
    stopRecording: unused,
    cancelRecording: unused,
    listenToRecordingFailures: async () => () => undefined,
    chooseAudio: unused,
    chooseTranscript: unused,
    chooseOutputDirectory: unused,
    openModelAccessPage: unused,
    listenToJobs: async () => () => undefined,
    loadChatGptSettings: async () => ({
      authMode: "apiKey",
      model: null,
      welcomeAcknowledged: true,
      account: { state: "signedOut", email: null },
    }),
    saveChatGptPreferences: unused,
    signInWithChatGpt: unused,
    cancelChatGptSignIn: unused,
    listChatGptModels: unused,
    signOutOfChatGpt: unused,
    openChatGptUsagePage: unused,
    listenToChatGptEvents: async () => () => undefined,
    loadAppAccess: record("loadAppAccess", { state: "signedOut" } as AppAccess),
    signInToGateway: record("signInToGateway", signedIn),
    cancelGatewaySignIn: record("cancelGatewaySignIn", undefined),
    signOutOfGateway: record("signOutOfGateway", undefined),
    ...overrides,
  }
}

async function start(
  overrides: Partial<BackendPort>,
): Promise<{ root: HTMLElement; view: AppView; controller: AppController; calls: string[] }> {
  const calls: string[] = []
  const window = new Window()
  const root = window.document.createElement("div") as unknown as HTMLElement
  window.document.body.appendChild(root as unknown as never)
  const view = new AppView(root)
  const controller = new AppController(backendWith(overrides, calls), view)
  await controller.start()
  return { root, view, controller, calls }
}

function pick<T extends HTMLElement = HTMLElement>(root: HTMLElement, selector: string): T {
  const element = root.querySelector<T>(selector)
  if (element === null) throw new Error(`missing ${selector}`)
  return element
}

function click(root: HTMLElement, action: string): void {
  pick(root, `[data-action="${action}"]`).click()
}

function shellInert(root: HTMLElement): boolean {
  return pick(root, ".app-shell").hasAttribute("inert")
}

describe("Google sign-in gate (real DOM)", () => {
  test("without a stored sign-in only the login screen is usable", async () => {
    // Given / When
    const { root, controller, calls } = await start({})

    // Then
    expect(pick(root, "#access-screen").hidden).toBe(false)
    expect(shellInert(root)).toBe(true)
    expect(pick(root, "#access-sign-in-button").hidden).toBe(false)
    expect(pick(root, "#access-status").textContent).toContain("Google 계정으로 로그인")
    expect(calls).not.toContain("diagnose")
    controller.stop()
  })

  test("a check that fails shows the error with both a retry and a sign-in", async () => {
    // Given
    let attempts = 0
    const { root, controller } = await start({
      loadAppAccess: async () => {
        attempts += 1
        if (attempts === 1) {
          throw { code: "SECRET_READ_FAILED", message: "저장된 로그인 정보를 읽지 못했습니다." }
        }
        return signedIn
      },
    })

    // Then
    const status = pick(root, "#access-status")
    expect(status.dataset["state"]).toBe("error")
    expect(status.textContent).toContain("저장된 로그인 정보를 읽지 못했습니다.")
    expect(pick(root, "#access-sign-in-button").hidden).toBe(false)
    expect(pick(root, "#access-retry-button").hidden).toBe(false)

    // When: the user checks again
    click(root, "retry-app-access")
    await settle()

    // Then
    expect(pick(root, "#access-screen").hidden).toBe(true)
    expect(shellInert(root)).toBe(false)
    controller.stop()
  })

  test("a pending browser sign-in offers only cancel and starts no second attempt", async () => {
    // Given: a sign-in the user has not finished in the browser
    const pending = Promise.withResolvers<AppAccess>()
    let signIns = 0
    const { root, controller } = await start({
      signInToGateway: () => {
        signIns += 1
        return pending.promise
      },
      cancelGatewaySignIn: async () => {
        pending.reject({ code: "CANCELLED", message: "사용자가 작업을 취소했습니다." })
      },
    })

    // When
    click(root, "sign-in-gateway")
    await settle()
    click(root, "sign-in-gateway")
    await settle()

    // Then
    expect(signIns).toBe(1)
    expect(pick(root, "#access-sign-in-button").hidden).toBe(true)
    expect(pick(root, "#access-cancel-button").hidden).toBe(false)
    expect(pick(root, "#access-status").textContent).toContain("브라우저에서 Google 로그인")

    // When: the user cancels
    click(root, "cancel-gateway-sign-in")
    await settle()

    // Then: back to the start without an error
    expect(pick(root, "#access-cancel-button").hidden).toBe(true)
    expect(pick(root, "#access-sign-in-button").hidden).toBe(false)
    expect(pick(root, "#access-status").dataset["state"]).toBe("ready")
    controller.stop()
  })

  test("signing in opens the app and loads the workspace", async () => {
    // Given
    const { root, controller, calls } = await start({})

    // When
    click(root, "sign-in-gateway")
    await settle()

    // Then
    expect(calls.indexOf("signInToGateway")).toBeLessThan(calls.indexOf("diagnose"))
    expect(pick(root, "#access-screen").hidden).toBe(true)
    expect(shellInert(root)).toBe(false)
    expect(pick(root, "#account-chip").hidden).toBe(false)
    expect(pick(root, "#account-email").textContent).toBe("dev@example.com")
    expect(pick(root, "#account-mode").textContent).toBe("온라인")
    controller.stop()
  })

  test("a stored sign-in the gateway could not renew says the app is offline", async () => {
    const { root, controller } = await start({
      loadAppAccess: async () => ({ state: "signedIn", email: null, offline: true }),
    })

    expect(pick(root, "#access-screen").hidden).toBe(true)
    expect(pick(root, "#account-email").textContent).toBe("Google 계정")
    expect(pick(root, "#account-mode").textContent).toBe("오프라인 · 저장된 로그인 사용")
    controller.stop()
  })

  test("signing out returns to the login screen, with the error when it failed", async () => {
    // Given: a signed-in app whose sign-out cannot clear the stored session
    const { root, controller } = await start({
      loadAppAccess: async () => signedIn,
      signOutOfGateway: async () => {
        throw { code: "GATEWAY_SIGN_OUT_INCOMPLETE", message: "로그인 정보를 지우지 못했습니다." }
      },
    })

    // When
    click(root, "sign-out-gateway")
    await settle()

    // Then
    expect(pick(root, "#access-screen").hidden).toBe(false)
    expect(shellInert(root)).toBe(true)
    expect(pick(root, "#account-chip").hidden).toBe(true)
    expect(pick(root, "#access-status").textContent).toBe("로그인 정보를 지우지 못했습니다.")
    expect(pick(root, "#access-sign-in-button").hidden).toBe(false)
    controller.stop()
  })

  test("sign-out waits while a job runs", async () => {
    const { root, view, controller } = await start({ loadAppAccess: async () => signedIn })

    view.setBusy("transcription")
    expect(pick<HTMLButtonElement>(root, "#sign-out-button").disabled).toBe(true)

    view.setBusy(null)
    expect(pick<HTMLButtonElement>(root, "#sign-out-button").disabled).toBe(false)
    controller.stop()
  })
})
