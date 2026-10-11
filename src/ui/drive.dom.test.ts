import { afterAll, beforeAll, describe, expect, test } from "bun:test"
import { Window } from "happy-dom"

import type { BackendPort, TranscriptionRequest } from "../domain/backend"
import type {
  DriveImport,
  DriveRecordingUpload,
  DriveStatus,
  DriveSyncResult,
} from "../domain/drive"
import type { ImportedTranscript, TranscriptionResult } from "../domain/job"
import { AppView } from "./app-view"
import { AppController } from "./controller"

const styles = await Bun.file(new URL("../styles.css", import.meta.url)).text()

// The recording path drives `window.setInterval` and document listeners, so
// a DOM is installed globally like in the recording controller's own tests.
const previous = {
  window: Reflect.get(globalThis, "window") as unknown,
  document: Reflect.get(globalThis, "document") as unknown,
}
let globalDom: Window

beforeAll(() => {
  globalDom = new Window()
  Object.assign(globalThis, { window: globalDom, document: globalDom.document })
})

afterAll(() => {
  Object.assign(globalThis, previous)
  void globalDom.happyDOM.close()
})

const connected: DriveStatus = {
  state: "connected",
  email: "dev@example.com",
  saveRecordings: false,
}
const fresh: DriveStatus = { state: "disconnected", onboardingDismissed: false }
const dismissed: DriveStatus = { state: "disconnected", onboardingDismissed: true }

const transcribed: TranscriptionResult = {
  jobId: "transcription-1",
  srt: "/out/회의/회의.srt",
  txt: "/out/회의/회의_화자별.txt",
  checkpoint: null,
  outputDirectory: "/out/회의",
  segments: 3,
  filtered: 0,
}

const uploaded: DriveSyncResult = {
  folderLink: "https://drive.google.com/drive/folders/m1",
  files: [
    { kind: "srt", state: "uploaded", webViewLink: null },
    { kind: "speaker_text", state: "uploaded", webViewLink: null },
  ],
}

const imported = (jobId: string): DriveImport => ({
  jobId,
  inputPath: "/cache/drive-imports/u1/주간 회의.m4a",
  driveFileId: "file-1",
  name: "주간 회의.m4a",
})

function failWith(code: string, message: string): () => Promise<never> {
  return () => Promise.reject({ code, message })
}

/** A signed-in, engine-ready host; anything a test does not override is unused. */
function backend(overrides: Partial<BackendPort>, calls: string[] = []): BackendPort {
  const unused = (name: string) => () => Promise.reject(new Error(`unused: ${name}`))
  const record =
    <T>(name: string, value: T) =>
    async (): Promise<T> => {
      calls.push(name)
      return value
    }
  return {
    diagnose: async () => ({
      enginePreset: "qwen3" as const,
      engineReady: true,
      modelsReady: true,
      ffmpegReady: true,
      qwen3Ready: true,
      whisperxReady: false,
      dataDirectory: "/tmp/galpi",
      defaultOutputDirectory: "/out",
      engineVersion: "test",
      computeDevice: "cpu" as const,
      availablePresets: ["qwen3", "whisperx"] as const,
      availableDevices: [] as const,
      cudaDriverDetected: false,
    }),
    prepare: unused("prepare"),
    huggingFaceTokenStored: async () => false,
    saveHuggingFaceToken: unused("saveHuggingFaceToken"),
    loadAssistantSettings: async () => ({
      apiKeyStored: true,
      model: null,
      baseUrl: null,
      reasoningEffort: null,
      background: null,
      participants: [],
      glossary: [],
    }),
    saveAssistantApiKey: unused("saveAssistantApiKey"),
    saveAssistantSettings: async () => undefined,
    saveEnginePreset: unused("saveEnginePreset"),
    saveComputeDevice: unused("saveComputeDevice"),
    refineTranscript: unused("refineTranscript"),
    transcribe: unused("transcribe"),
    importTranscript: unused("importTranscript"),
    cancel: async (jobId) => {
      calls.push(`cancel:${jobId}`)
    },
    openArtifact: unused("openArtifact"),
    revealOutput: unused("revealOutput"),
    startRecording: unused("startRecording"),
    stopRecording: unused("stopRecording"),
    cancelRecording: unused("cancelRecording"),
    listenToRecordingFailures: async () => () => undefined,
    chooseAudio: async () => "/local/로컬 회의.m4a",
    chooseTranscript: unused("chooseTranscript"),
    chooseOutputDirectory: unused("chooseOutputDirectory"),
    openModelAccessPage: unused("openModelAccessPage"),
    listenToJobs: async () => () => undefined,
    loadChatGptSettings: async () => ({
      authMode: "apiKey" as const,
      model: null,
      reasoningEffort: null,
      welcomeAcknowledged: true,
      account: { state: "signedOut" as const, email: null },
    }),
    saveChatGptPreferences: async () => undefined,
    signInWithChatGpt: unused("signInWithChatGpt"),
    cancelChatGptSignIn: unused("cancelChatGptSignIn"),
    listChatGptModels: async () => [],
    signOutOfChatGpt: unused("signOutOfChatGpt"),
    openChatGptUsagePage: unused("openChatGptUsagePage"),
    listenToChatGptEvents: async () => () => undefined,
    loadAppAccess: async () => ({ state: "signedIn", email: "dev@example.com", offline: false }),
    signInToGateway: unused("signInToGateway"),
    cancelGatewaySignIn: unused("cancelGatewaySignIn"),
    signOutOfGateway: unused("signOutOfGateway"),
    loadDriveStatus: async () => connected,
    connectDrive: unused("connectDrive"),
    cancelDriveOperation: record("cancelDriveOperation", undefined),
    disconnectDrive: unused("disconnectDrive"),
    dismissDriveOnboarding: record("dismissDriveOnboarding", dismissed),
    importFromDrive: unused("importFromDrive"),
    importTranscriptFromDrive: unused("importTranscriptFromDrive"),
    syncMeetingToDrive: async (jobId, overwrite) => {
      calls.push(`sync:${jobId}:${overwrite}`)
      return uploaded
    },
    setDriveSaveRecordings: unused("setDriveSaveRecordings"),
    uploadRecordingToDrive: unused("uploadRecordingToDrive"),
    openDriveLink: unused("openDriveLink"),
    ...overrides,
  }
}

async function mount(port: BackendPort): Promise<{ controller: AppController; root: HTMLElement }> {
  const window = new Window()
  const sheet = window.document.createElement("style")
  sheet.textContent = styles
  window.document.head.appendChild(sheet)
  const root = window.document.createElement("div") as unknown as HTMLElement
  window.document.body.appendChild(root as unknown as never)
  const controller = new AppController(port, new AppView(root))
  await controller.start()
  await settle()
  return { controller, root }
}

// Microtask drain instead of a wall-clock wait.
async function settle(): Promise<void> {
  for (let tick = 0; tick < 40; tick += 1) await Promise.resolve()
}

function pick<T extends HTMLElement = HTMLElement>(root: HTMLElement, selector: string): T {
  const element = root.querySelector<T>(selector)
  if (element === null) throw new Error(`missing ${selector}`)
  return element
}

function visible(root: HTMLElement, selector: string): boolean {
  return pick(root, selector).closest("[hidden]") === null
}

async function click(root: HTMLElement, selector: string): Promise<void> {
  pick(root, selector).click()
  await settle()
}

/** Text a person can see: inside no hidden ancestor. */
function visibleText(root: HTMLElement): string {
  return [...root.querySelectorAll<HTMLElement>("*")]
    .filter((element) => element.children.length === 0 && element.closest("[hidden]") === null)
    .map((element) => element.textContent ?? "")
    .join("\n")
}

describe("Drive onboarding and settings", () => {
  test("the card shows until dismissed, and only while disconnected", async () => {
    // Given
    const calls: string[] = []
    const { controller, root } = await mount(
      backend({ loadDriveStatus: async () => fresh }, calls),
    )
    expect(visible(root, "#drive-onboarding")).toBe(true)
    expect(visible(root, "#drive-import-button")).toBe(false)

    // When
    await click(root, "#drive-dismiss-button")

    // Then
    expect(calls).toContain("dismissDriveOnboarding")
    expect(visible(root, "#drive-onboarding")).toBe(false)
    controller.stop()
  })

  test("connecting from the card can be cancelled there, and success hides the card", async () => {
    // Given: a consent waiting in the browser
    const calls: string[] = []
    const consent = Promise.withResolvers<DriveStatus>()
    const { controller, root } = await mount(
      backend({ loadDriveStatus: async () => fresh, connectDrive: () => consent.promise }, calls),
    )

    // When
    pick(root, "#drive-connect-button").focus()
    await click(root, "#drive-connect-button")

    // Then: the card offers a cancel instead of the connect button, and focus follows it
    expect(visible(root, "#drive-onboarding-cancel-button")).toBe(true)
    expect(visible(root, "#drive-connect-button")).toBe(false)
    expect(root.ownerDocument.activeElement?.id).toBe("drive-onboarding-cancel-button")
    expect(pick(root, "#drive-onboarding-status").textContent).toContain("브라우저")
    await click(root, "#drive-onboarding-cancel-button")
    expect(calls.filter((call) => call === "cancelDriveOperation")).toHaveLength(1)

    // When: the consent completes anyway
    consent.resolve(connected)
    await settle()

    // Then
    expect(visible(root, "#drive-onboarding")).toBe(false)
    expect(visible(root, "#drive-chip")).toBe(true)
    // The signed-in account already shows the address; the chip only says it is connected
    expect(pick(root, "#drive-account").textContent).toBe("연결됨")
    expect(pick(root, "#drive-chip").title).toBe("dev@example.com")
    expect(visible(root, "#drive-import-button")).toBe(true)
    expect(root.ownerDocument.activeElement?.id).toBe("drive-import-button")
    controller.stop()
  })

  test("names the Drive account in the chip when it is not the signed-in one", async () => {
    // Given: Drive connected with another Google account than the Galpi sign-in
    const { controller, root } = await mount(
      backend({
        loadDriveStatus: async () => ({ ...connected, email: "team@example.com" }),
      }),
    )

    // Then
    expect(pick(root, "#drive-account").textContent).toBe("team@example.com")
    expect(pick(root, "#drive-chip").title).toBe("team@example.com")
    controller.stop()
  })

  test("dismissing the card while a transcription runs hands focus to settings", async () => {
    // Given: a running transcription disables the start actions
    const running = Promise.withResolvers<TranscriptionResult>()
    const { controller, root } = await mount(
      backend({ loadDriveStatus: async () => fresh, transcribe: () => running.promise }),
    )
    await click(root, '[data-action="choose-audio"]')
    await click(root, "#start-button")
    expect(pick<HTMLButtonElement>(root, "#audio-selection").disabled).toBe(true)

    // When
    pick(root, "#drive-dismiss-button").focus()
    await click(root, "#drive-dismiss-button")

    // Then
    expect(visible(root, "#drive-onboarding")).toBe(false)
    expect(root.ownerDocument.activeElement?.matches('[data-action="open-settings"]')).toBe(true)
    running.reject({ code: "CANCELLED", message: "사용자가 작업을 취소했습니다." })
    await settle()
    controller.stop()
  })

  test("connecting from settings shows the settings cancel button, which cancels once", async () => {
    // Given: the card was dismissed, so settings is the way in
    const calls: string[] = []
    const consent = Promise.withResolvers<DriveStatus>()
    const { controller, root } = await mount(
      backend({ loadDriveStatus: async () => dismissed, connectDrive: () => consent.promise }, calls),
    )
    await click(root, '[data-action="open-settings"]')
    await click(root, "#settings-tab-drive")
    expect(pick(root, "#drive-settings-state").textContent).toBe("연결 안 됨")

    // When
    await click(root, "#drive-settings-connect-button")
    await click(root, "#drive-settings-cancel-button")
    consent.reject({ code: "CANCELLED", message: "사용자가 작업을 취소했습니다." })
    await settle()

    // Then
    expect(calls.filter((call) => call === "cancelDriveOperation")).toHaveLength(1)
    expect(visible(root, "#drive-settings-cancel-button")).toBe(false)
    expect(visible(root, "#drive-settings-connect-button")).toBe(true)
    controller.stop()
  })

  test("a Drive command refused for sign-in returns to the login screen", async () => {
    // Given
    const { controller, root } = await mount(
      backend({
        loadDriveStatus: async () => fresh,
        connectDrive: failWith("AUTH_REQUIRED", "Galpi를 사용하려면 Google로 로그인해 주세요."),
      }),
    )

    // When
    await click(root, "#drive-connect-button")

    // Then
    expect(visible(root, "#access-sign-in-button")).toBe(true)
    expect(pick(root, ".app-shell").hasAttribute("inert")).toBe(true)
    controller.stop()
  })

  test("an unreadable status offers no Drive action anywhere", async () => {
    // Given
    const { controller, root } = await mount(
      backend({ loadDriveStatus: () => Promise.reject(new Error("unreadable")) }),
    )

    // When
    await click(root, '[data-action="open-settings"]')

    // Then
    for (const selector of [
      "#drive-onboarding",
      "#drive-settings-connect-button",
      "#drive-disconnect-button",
      "#drive-import-button",
    ]) {
      expect(visible(root, selector)).toBe(false)
    }
    controller.stop()
  })

  test("disconnecting from settings hides every connected surface", async () => {
    // Given
    const { controller, root } = await mount(
      backend({ disconnectDrive: async () => dismissed }),
    )
    await click(root, '[data-action="open-settings"]')

    // When
    await click(root, "#drive-disconnect-button")

    // Then
    expect(visible(root, "#drive-chip")).toBe(false)
    expect(root.querySelector<HTMLElement>("#drive-import-button")?.hidden).toBe(true)
    expect(pick(root, "#drive-settings-state").textContent).toBe("연결 안 됨")
    controller.stop()
  })

  test("a sync ended by the user's own disconnect does not bring the card back", async () => {
    // Given: the card was dismissed and a sync is still running
    const sync = Promise.withResolvers<DriveSyncResult>()
    const { controller, root } = await mount(
      backend({
        transcribe: async () => transcribed,
        syncMeetingToDrive: () => sync.promise,
        disconnectDrive: async () => dismissed,
      }),
    )
    await click(root, '[data-action="choose-audio"]')
    await click(root, "#start-button")

    // When: the user disconnects, so the host ends the sync as not connected
    await click(root, "#drive-disconnect-button")
    sync.reject({ code: "DRIVE_NOT_CONNECTED", message: "먼저 Google Drive를 연결해 주세요." })
    await settle()

    // Then
    expect(visible(root, "#drive-onboarding")).toBe(false)
    expect(pick(root, "#drive-settings-state").textContent).toBe("연결 안 됨")
    controller.stop()
  })
})

describe("Settings tabs", () => {
  test("a contextual link opens its tab, and the gear reopens the tab last shown", async () => {
    // Given: no roster yet, so the attendee picker links to the roster
    const { controller, root } = await mount(backend({}))

    // When
    await click(root, '#attendee-empty [data-action="open-settings"]')

    // Then: the meeting tab is the one shown, and the others are out of the way
    expect(pick(root, "#settings-tab-meeting").getAttribute("aria-selected")).toBe("true")
    expect(visible(root, "#participant-rows")).toBe(true)
    expect(visible(root, "#settings-hf-token")).toBe(false)

    // When: closed and reopened from the gear
    await click(root, '[data-action="close-settings"]')
    await click(root, '[data-action="open-settings"]')

    // Then
    expect(pick(root, "#settings-tab-meeting").getAttribute("aria-selected")).toBe("true")
    controller.stop()
  })

  test("arrow keys move between tabs and keep one tab in the tab order", async () => {
    // Given
    const { controller, root } = await mount(backend({}))
    await click(root, '[data-action="open-settings"]')
    const engine = pick(root, "#settings-tab-engine")

    // When: right from the first tab, then left wraps to the last
    keydown(engine, "ArrowRight")
    const meeting = pick(root, "#settings-tab-meeting")

    // Then
    expect(root.ownerDocument.activeElement).toBe(meeting)
    expect(meeting.getAttribute("aria-selected")).toBe("true")
    expect([...root.querySelectorAll<HTMLElement>('[role="tab"]')].map((tab) => tab.tabIndex)).toEqual([
      -1, 0, -1, -1,
    ])
    keydown(engine, "ArrowLeft")
    expect(pick(root, "#settings-tab-drive").getAttribute("aria-selected")).toBe("true")
    expect(visible(root, "#drive-settings")).toBe(true)
    controller.stop()
  })
})

function keydown(target: HTMLElement, key: string): void {
  const view = target.ownerDocument.defaultView
  if (view === null) throw new Error("the element has no window")
  target.dispatchEvent(new view.KeyboardEvent("keydown", { key, bubbles: true }))
}

describe("Drive sync after a meeting", () => {
  async function transcribedMeeting(overrides: Partial<BackendPort>, calls: string[] = []) {
    const mounted = await mount(
      backend({ transcribe: async () => transcribed, ...overrides }, calls),
    )
    await click(mounted.root, '[data-action="choose-audio"]')
    await click(mounted.root, "#start-button")
    return mounted
  }

  test("a finished transcription syncs, and only while Drive is connected", async () => {
    // Given / When
    const calls: string[] = []
    const { controller, root } = await transcribedMeeting({}, calls)

    // Then
    expect(calls).toContain("sync:transcription-1:false")
    expect(visible(root, "#drive-sync-row")).toBe(true)
    expect(pick(root, "#drive-sync-status").textContent).toBe("Drive의 Galpi 폴더에 올렸습니다.")
    expect(pick(root, "#drive-sync-status").dataset["state"]).toBe("ready")
    controller.stop()

    // And: nothing is synced without a connection
    const offline: string[] = []
    const second = await transcribedMeeting({ loadDriveStatus: async () => dismissed }, offline)
    expect(offline.some((call) => call.startsWith("sync:"))).toBe(false)
    second.controller.stop()
  })

  test("a conflict offers to overwrite, which syncs again with overwrite", async () => {
    // Given
    const calls: string[] = []
    let answer: DriveSyncResult = {
      folderLink: null,
      files: [{ kind: "speaker_text", state: "conflict", webViewLink: null }],
    }
    const { controller, root } = await transcribedMeeting(
      {
        syncMeetingToDrive: async (jobId, overwrite) => {
          calls.push(`sync:${jobId}:${overwrite}`)
          return answer
        },
      },
      calls,
    )
    expect(pick(root, "#drive-sync-status").textContent).toBe(
      "Drive의 파일이 다른 곳에서 바뀌어 덮어쓰지 않았습니다.",
    )
    expect(visible(root, "#drive-overwrite-button")).toBe(true)

    // When
    answer = { folderLink: null, files: [{ kind: "speaker_text", state: "updated", webViewLink: null }] }
    await click(root, "#drive-overwrite-button")

    // Then
    expect(calls).toContain("sync:transcription-1:true")
    expect(visible(root, "#drive-overwrite-button")).toBe(false)
    controller.stop()
  })

  test("a sync that needs reconnecting brings the onboarding card back", async () => {
    // Given / When
    const { controller, root } = await transcribedMeeting({
      syncMeetingToDrive: failWith(
        "DRIVE_RECONNECT_REQUIRED",
        "Drive 연결이 만료되었습니다. 다시 연결해 주세요.",
      ),
    })

    // Then
    expect(visible(root, "#drive-onboarding")).toBe(true)
    expect(pick(root, "#drive-sync-status").textContent).toBe(
      "Drive 연결이 만료되었습니다. 다시 연결해 주세요.",
    )
    expect(visible(root, "#drive-import-button")).toBe(false)
    controller.stop()
  })

  test("refinement syncs under the transcription job, and an imported transcript syncs too", async () => {
    // Given
    const calls: string[] = []
    const { controller, root } = await transcribedMeeting(
      {
        refineTranscript: async (jobId) => ({ jobId, minutes: "/out/회의/회의_회의록.md" }),
        chooseTranscript: async () => "/notes/회의.txt",
        importTranscript: async () => ({
          jobId: "import-1",
          txt: "/out/회의 2/회의.txt",
          outputDirectory: "/out/회의 2",
        }),
      },
      calls,
    )

    // When
    await click(root, "#refine-button")
    await click(root, "#transcript-selection")

    // Then
    expect(calls.filter((call) => call === "sync:transcription-1:false")).toHaveLength(2)
    expect(calls).toContain("sync:import-1:false")
    controller.stop()
  })

  test("the next meeting's sync starts without the previous meeting's files or folder link", async () => {
    // Given: meeting A synced, and meeting B's sync still running
    const second = Promise.withResolvers<DriveSyncResult>()
    let syncs = 0
    let transcriptions = 0
    const { controller, root } = await transcribedMeeting({
      transcribe: async () => {
        transcriptions += 1
        return { ...transcribed, jobId: `transcription-${transcriptions}` }
      },
      syncMeetingToDrive: () => {
        syncs += 1
        return syncs === 1 ? Promise.resolve(uploaded) : second.promise
      },
    })
    expect(pick(root, "#drive-sync-files").children).toHaveLength(2)

    // When
    await click(root, "#start-button")

    // Then
    expect(pick(root, "#drive-sync-status").textContent).toBe("Drive에 결과를 올리는 중입니다.")
    expect(pick(root, "#drive-sync-files").children).toHaveLength(0)
    expect(visible(root, "#drive-open-folder-button")).toBe(false)
    second.resolve(uploaded)
    await settle()
    controller.stop()
  })
})

describe("A finished run comes into view", () => {
  test("results take focus after a transcription, and the minutes action after a refinement", async () => {
    // Given / When: a transcription finishes
    const { controller, root } = await mount(
      backend({
        transcribe: async () => transcribed,
        refineTranscript: async (jobId) => ({ jobId, minutes: "/out/회의/회의_회의록.md" }),
      }),
    )
    await click(root, '[data-action="choose-audio"]')
    await click(root, "#start-button")

    // Then: the results heading holds focus, so the outcome is announced where it is
    expect(root.ownerDocument.activeElement?.id).toBe("results-title")

    // When: the minutes are written
    await click(root, "#refine-button")

    // Then: the enabled primary action to open them takes focus
    expect(root.ownerDocument.activeElement?.id).toBe("open-minutes-button")
    controller.stop()
  })

  test("an open settings sheet keeps its focus when a run finishes behind it", async () => {
    // Given: a transcription still running while the user opens settings
    const running = Promise.withResolvers<typeof transcribed>()
    const { controller, root } = await mount(backend({ transcribe: () => running.promise }))
    await click(root, '[data-action="choose-audio"]')
    await click(root, "#start-button")
    await click(root, '[data-action="open-settings"]')
    const focused = root.ownerDocument.activeElement

    // When
    running.resolve(transcribed)
    await settle()

    // Then
    expect(root.ownerDocument.activeElement).toBe(focused)
    controller.stop()
  })
})

describe("Drive import as a job", () => {
  test("a pending import shows a running card whose cancel targets the import job", async () => {
    // Given: a fresh session and an import still in the Picker
    const calls: string[] = []
    const pending = Promise.withResolvers<DriveImport>()
    let importJob = ""
    const { controller, root } = await mount(
      backend(
        {
          importFromDrive: (jobId) => {
            importJob = jobId
            return pending.promise
          },
        },
        calls,
      ),
    )

    // When
    await click(root, "#drive-import-button")

    // Then
    expect(visible(root, "#cancel-button")).toBe(true)
    expect(pick(root, "#job-panel").dataset["status"]).toBe("running")
    expect(pick(root, "#job-title").textContent).toBe("Drive에서 녹음을 가져오고 있습니다")
    expect(pick<HTMLButtonElement>(root, "#drive-import-button").disabled).toBe(true)
    await click(root, "#cancel-button")
    expect(calls).toContain(`cancel:${importJob}`)
    pending.reject({ code: "CANCELLED", message: "사용자가 작업을 취소했습니다." })
    await settle()
    expect(pick(root, "#job-title").textContent).toBe("Drive 가져오기를 취소했습니다")
    controller.stop()
  })

  test("a finished import hides the card and names the file, even after a transcription", async () => {
    // Given: a session that already finished a transcription
    const { controller, root } = await mount(
      backend({
        transcribe: async () => transcribed,
        importFromDrive: async (jobId) => imported(jobId),
      }),
    )
    await click(root, '[data-action="choose-audio"]')
    await click(root, "#start-button")
    expect(visibleText(root)).toContain("전사 완료")

    // When
    await click(root, "#drive-import-button")

    // Then
    expect(visible(root, "#job-panel")).toBe(false)
    expect(visibleText(root)).not.toContain("전사 완료")
    expect(pick(root, "#audio-path").textContent).toBe("주간 회의.m4a")
    expect(pick(root, "#task-status").textContent).toBe("전사 준비됨")
    expect(pick<HTMLButtonElement>(root, "#drive-import-button").disabled).toBe(false)
    controller.stop()
  })

  test("a refused import fails the card with the host's reason and frees the app", async () => {
    // Given
    const { controller, root } = await mount(
      backend({
        importFromDrive: failWith(
          "DRIVE_UNSUPPORTED_FILE",
          "오디오나 영상 파일만 가져올 수 있습니다.",
        ),
      }),
    )

    // When
    await click(root, "#drive-import-button")

    // Then
    expect(pick(root, "#job-panel").dataset["status"]).toBe("failed")
    expect(pick(root, "#error-message").textContent).toBe("오디오나 영상 파일만 가져올 수 있습니다.")
    expect(visible(root, "#error-message")).toBe(true)
    expect(pick(root, "#job-title").textContent).toBe("Drive에서 녹음을 가져오지 못했습니다")
    expect(pick<HTMLButtonElement>(root, "#record-button").disabled).toBe(false)
    expect(pick<HTMLButtonElement>(root, "#drive-import-button").disabled).toBe(false)
    controller.stop()
  })

  test("an import declined in the browser cancels the card instead of failing it", async () => {
    // Given
    const { controller, root } = await mount(
      backend({ importFromDrive: failWith("CANCELLED", "사용자가 작업을 취소했습니다.") }),
    )

    // When
    await click(root, "#drive-import-button")

    // Then
    expect(pick(root, "#job-panel").dataset["status"]).toBe("cancelled")
    expect(pick(root, "#job-title").textContent).toBe("Drive 가져오기를 취소했습니다")
    expect(visible(root, "#error-message")).toBe(false)
    expect(pick<HTMLButtonElement>(root, "#drive-import-button").disabled).toBe(false)
    controller.stop()
  })

  test("the import button waits while a transcription runs, whose cancel stays its own", async () => {
    // Given: a transcription still running
    const calls: string[] = []
    const running = Promise.withResolvers<TranscriptionResult>()
    let transcriptionJob = ""
    const { controller, root } = await mount(
      backend(
        {
          transcribe: (request) => {
            transcriptionJob = request.jobId
            return running.promise
          },
        },
        calls,
      ),
    )
    await click(root, '[data-action="choose-audio"]')

    // When
    await click(root, "#start-button")

    // Then
    expect(pick<HTMLButtonElement>(root, "#drive-import-button").disabled).toBe(true)
    await click(root, "#cancel-button")
    expect(calls).toContain(`cancel:${transcriptionJob}`)
    running.reject({ code: "CANCELLED", message: "사용자가 작업을 취소했습니다." })
    await settle()
    controller.stop()
  })

  test("only audio from Drive carries its Drive id into the transcription request", async () => {
    // Given
    const requests: TranscriptionRequest[] = []
    const { controller, root } = await mount(
      backend({
        importFromDrive: async (jobId) => imported(jobId),
        transcribe: async (request) => {
          requests.push(request)
          return { ...transcribed, jobId: request.jobId }
        },
        startRecording: async () => ({
          recordingId: "rec-1",
          path: "/out/녹음/녹음.wav.part",
          sampleRate: 48_000,
          channels: 1,
        }),
        stopRecording: async () => ({
          recordingId: "rec-1",
          path: "/out/녹음/녹음.wav",
          sampleRate: 48_000,
          channels: 1,
          frames: 48_000,
          droppedFrames: 0,
          durationSeconds: 1,
        }),
      }),
    )

    // When: Drive audio, then a local file, then Drive again, then a recording
    await click(root, "#drive-import-button")
    await click(root, "#start-button")
    await click(root, '[data-action="choose-audio"]')
    await click(root, "#start-button")
    await click(root, "#drive-import-button")
    await click(root, "#record-button")
    await click(root, "#stop-recording-button")
    await click(root, "#start-button")

    // Then
    expect(requests.map((request) => [request.inputPath, request.driveSourceId])).toEqual([
      ["/cache/drive-imports/u1/주간 회의.m4a", "file-1"],
      ["/local/로컬 회의.m4a", undefined],
      ["/out/녹음/녹음.wav", undefined],
    ])
    // The key itself is absent, so the host sees no claim at all.
    expect(requests.slice(1).some((request) => "driveSourceId" in request)).toBe(false)
    controller.stop()
  })
})

describe("Drive transcript import into the augment panel", () => {
  const transcriptImported = {
    jobId: "import-1",
    txt: "/out/새로운 노트/새로운 노트_화자별.txt",
    outputDirectory: "/out/새로운 노트",
  }

  test("the picker shows only while Drive is connected", async () => {
    // Given / When
    const connectedApp = await mount(backend({}))
    const disconnectedApp = await mount(backend({ loadDriveStatus: async () => dismissed }))

    // Then
    expect(visible(connectedApp.root, "#drive-transcript-button")).toBe(true)
    expect(pick(connectedApp.root, "#drive-transcript-button").textContent).toContain(
      "Drive에서 전사문 가져오기",
    )
    expect(visible(disconnectedApp.root, "#drive-transcript-button")).toBe(false)
    connectedApp.controller.stop()
    disconnectedApp.controller.stop()
  })

  test("a pending pick shows the augment card whose cancel targets the import job", async () => {
    // Given: a transcript still in the Picker
    const calls: string[] = []
    const pending = Promise.withResolvers<ImportedTranscript>()
    let importJob = ""
    const { controller, root } = await mount(
      backend(
        {
          importTranscriptFromDrive: (jobId) => {
            importJob = jobId
            return pending.promise
          },
        },
        calls,
      ),
    )

    // When
    await click(root, "#drive-transcript-button")

    // Then: the card has no percentage, a plain cancel, and the starters wait
    expect(visible(root, "#augment-progress")).toBe(true)
    expect(pick(root, "#augment-job-title").textContent).toBe(
      "Drive에서 전사문을 가져오고 있습니다",
    )
    expect(visible(root, "#augment-job-percent")).toBe(false)
    expect(visible(root, "#augment-job-progress")).toBe(false)
    expect(visible(root, "#job-panel")).toBe(false)
    expect(pick(root, "#augment-cancel-button").textContent).toBe("작업 취소")
    expect(pick<HTMLButtonElement>(root, "#drive-transcript-button").disabled).toBe(true)
    expect(pick<HTMLButtonElement>(root, "#transcript-selection").disabled).toBe(true)
    expect(pick<HTMLButtonElement>(root, "#drive-import-button").disabled).toBe(true)
    await click(root, "#augment-cancel-button")
    expect(calls).toContain(`cancel:${importJob}`)
    pending.reject({ code: "CANCELLED", message: "사용자가 작업을 취소했습니다." })
    await settle()
    expect(pick(root, "#augment-job-title").textContent).toBe("Drive 가져오기를 취소했습니다")
    expect(pick<HTMLButtonElement>(root, "#drive-transcript-button").disabled).toBe(false)
    controller.stop()
  })

  test("a finished pick becomes the augment target and syncs like a local transcript", async () => {
    // Given
    const calls: string[] = []
    const requests: string[][] = []
    const { controller, root } = await mount(
      backend(
        {
          importTranscriptFromDrive: async (jobId, outputRoot) => {
            requests.push([jobId, outputRoot])
            return transcriptImported
          },
        },
        calls,
      ),
    )

    // When
    await click(root, "#drive-transcript-button")

    // Then: imported into the chosen output folder as its own job
    expect(requests).toHaveLength(1)
    expect(requests[0]?.[1]).toBe("/out")
    expect(visible(root, "#augment-progress")).toBe(false)
    expect(pick(root, "#transcript-title").textContent).toBe("대상 전사문")
    expect(pick(root, "#transcript-path").textContent).toBe(transcriptImported.txt)
    expect(pick(root, "#result-txt").textContent).toBe(transcriptImported.txt)
    expect(pick(root, "#task-status").textContent).toBe("전사문 가져옴 · 회의록 작성 대기")
    expect(pick<HTMLButtonElement>(root, "#refine-button").disabled).toBe(false)
    expect(pick<HTMLButtonElement>(root, "#drive-transcript-button").disabled).toBe(false)
    expect(calls).toContain("sync:import-1:false")
    controller.stop()
  })

  test("a refused pick fails the augment card with the host's reason and frees the app", async () => {
    // Given
    const { controller, root } = await mount(
      backend({
        importTranscriptFromDrive: failWith(
          "DRIVE_UNSUPPORTED_FILE",
          "txt 또는 md 텍스트 파일만 가져올 수 있습니다.",
        ),
      }),
    )

    // When
    await click(root, "#drive-transcript-button")

    // Then
    expect(pick(root, "#augment-job-title").textContent).toBe(
      "Drive에서 전사문을 가져오지 못했습니다",
    )
    expect(pick(root, "#augment-error-message").textContent).toBe(
      "txt 또는 md 텍스트 파일만 가져올 수 있습니다.",
    )
    expect(visible(root, "#augment-error-message")).toBe(true)
    expect(visible(root, "#augment-cancel-button")).toBe(false)
    expect(pick<HTMLButtonElement>(root, "#drive-transcript-button").disabled).toBe(false)
    expect(pick(root, "#transcript-title").textContent).toBe("전사문 파일 가져오기")
    controller.stop()
  })

  test("a pick declined in the browser cancels the card instead of failing it", async () => {
    // Given
    const { controller, root } = await mount(
      backend({
        importTranscriptFromDrive: failWith("CANCELLED", "사용자가 작업을 취소했습니다."),
      }),
    )

    // When
    await click(root, "#drive-transcript-button")

    // Then
    expect(pick(root, "#augment-job-title").textContent).toBe("Drive 가져오기를 취소했습니다")
    expect(visible(root, "#augment-error-message")).toBe(false)
    expect(pick<HTMLButtonElement>(root, "#drive-transcript-button").disabled).toBe(false)
    controller.stop()
  })
})

describe("Drive saves a finished recording", () => {
  const saving: DriveStatus = {
    state: "connected",
    email: "dev@example.com",
    saveRecordings: true,
  }
  const folder = "https://drive.google.com/drive/folders/m1"
  const saved: DriveRecordingUpload = { state: "uploaded", folderLink: folder, webViewLink: null }
  const recorder: Partial<BackendPort> = {
    startRecording: async () => ({
      recordingId: "rec-1",
      path: "/out/녹음/녹음.wav.part",
      sampleRate: 48_000,
      channels: 1,
    }),
    stopRecording: async () => ({
      recordingId: "rec-1",
      path: "/out/녹음/녹음.wav",
      sampleRate: 48_000,
      channels: 1,
      frames: 48_000,
      droppedFrames: 0,
      durationSeconds: 1,
    }),
  }

  async function recorded(overrides: Partial<BackendPort>, calls: string[] = []) {
    const mounted = await mount(
      backend(
        {
          ...recorder,
          openDriveLink: async (url) => {
            calls.push(`open:${url}`)
          },
          ...overrides,
        },
        calls,
      ),
    )
    await click(mounted.root, "#record-button")
    await click(mounted.root, "#stop-recording-button")
    return mounted
  }

  const line = (root: HTMLElement) => pick(root, "#recording-drive-status")

  test("uploads right after stopping, while the recording is already selected", async () => {
    // Given: the choice is on and Drive is still working
    const calls: string[] = []
    const working = Promise.withResolvers<DriveRecordingUpload>()
    const { controller, root } = await recorded(
      {
        loadDriveStatus: async () => saving,
        uploadRecordingToDrive: (recordingId) => {
          calls.push(`upload:${recordingId}`)
          return working.promise
        },
      },
      calls,
    )

    // Then: the audio is chosen and the recorder is done before Drive answers
    expect(calls).toContain("upload:rec-1")
    expect(pick(root, "#audio-selection").dataset["selected"]).toBe("true")
    expect(pick(root, "#recording-status").textContent).toBe(
      "녹음이 완료되어 전사 파일로 선택했습니다.",
    )
    expect(line(root).textContent).toBe("녹음 파일을 Drive에 올리는 중입니다.")
    expect(line(root).dataset["state"]).toBe("pending")

    // When
    working.resolve(saved)
    await settle()

    // Then: the outcome is in words, with a way into the folder
    expect(line(root).textContent).toBe("녹음 파일을 Drive의 Galpi 폴더에 저장했습니다.")
    expect(line(root).dataset["state"]).toBe("ready")
    expect(visible(root, "#recording-drive-folder-button")).toBe(true)
    await click(root, "#recording-drive-folder-button")
    expect(calls).toContain(`open:${folder}`)
    controller.stop()
  })

  test("uploads nothing when the choice is off or Drive is not connected", async () => {
    for (const status of [connected, dismissed]) {
      // Given
      const calls: string[] = []
      const { controller, root } = await recorded(
        {
          loadDriveStatus: async () => status,
          uploadRecordingToDrive: async (recordingId) => {
            calls.push(`upload:${recordingId}`)
            return saved
          },
        },
        calls,
      )

      // Then
      expect(calls.some((call) => call.startsWith("upload:"))).toBe(false)
      expect(line(root).textContent).toBe("")
      controller.stop()
    }
  })

  test("a failed upload leaves the recording alone and retries on request", async () => {
    // Given: Drive is full on the first try
    const calls: string[] = []
    let attempts = 0
    const { controller, root } = await recorded(
      {
        loadDriveStatus: async () => saving,
        uploadRecordingToDrive: async (recordingId) => {
          attempts += 1
          calls.push(`upload:${recordingId}`)
          if (attempts === 1) {
            return failWith("DRIVE_STORAGE_FULL", "Google Drive 저장 공간이 부족합니다.")()
          }
          return saved
        },
      },
      calls,
    )

    // Then: the failure is stated and offers a retry; the local recording stays chosen
    expect(line(root).textContent).toBe(
      "녹음 파일을 Drive에 올리지 못했습니다. Google Drive 저장 공간이 부족합니다.",
    )
    expect(line(root).dataset["state"]).toBe("error")
    expect(visible(root, "#recording-drive-retry-button")).toBe(true)
    expect(visible(root, "#recording-drive-folder-button")).toBe(false)
    expect(pick(root, "#recorder").dataset["state"]).toBe("completed")
    expect(pick(root, "#audio-selection").dataset["selected"]).toBe("true")

    // When
    await click(root, "#recording-drive-retry-button")

    // Then: the same recording goes again, nothing else
    expect(calls.filter((call) => call.startsWith("upload:"))).toEqual([
      "upload:rec-1",
      "upload:rec-1",
    ])
    expect(line(root).dataset["state"]).toBe("ready")
    expect(visible(root, "#recording-drive-retry-button")).toBe(false)
    controller.stop()
  })

  test("a lost connection offers no retry, and a new recording clears the line", async () => {
    // Given: the connection expired while uploading
    const { controller, root } = await recorded({
      loadDriveStatus: async () => saving,
      uploadRecordingToDrive: failWith(
        "DRIVE_RECONNECT_REQUIRED",
        "Drive 연결이 만료되었습니다. 다시 연결해 주세요.",
      ),
    })

    // Then
    expect(line(root).textContent).toContain("Drive 연결이 만료되었습니다.")
    expect(visible(root, "#recording-drive-retry-button")).toBe(false)
    expect(visible(root, "#drive-onboarding")).toBe(true)

    // When
    await click(root, "#record-button")

    // Then
    expect(line(root).textContent).toBe("")
    controller.stop()
  })
})

describe("Drive settings: save recordings", () => {
  const saving: DriveStatus = {
    state: "connected",
    email: "dev@example.com",
    saveRecordings: true,
  }
  const toggle = (root: HTMLElement) => pick<HTMLInputElement>(root, "#drive-save-recordings")

  test("the switch saves at once and follows what the host stored", async () => {
    // Given
    const calls: string[] = []
    const { controller, root } = await mount(
      backend(
        {
          setDriveSaveRecordings: async (enabled) => {
            calls.push(`save:${enabled}`)
            return saving
          },
        },
        calls,
      ),
    )
    await click(root, '[data-action="open-settings"]')
    expect(toggle(root).disabled).toBe(false)
    expect(toggle(root).checked).toBe(false)

    // When
    await click(root, "#drive-save-recordings")

    // Then
    expect(calls).toContain("save:true")
    expect(toggle(root).checked).toBe(true)
    expect(toggle(root).disabled).toBe(false)
    expect(pick(root, "#settings-message").textContent).toBe("변경사항을 자동 저장했습니다.")
    controller.stop()
  })

  test("a refused save puts the switch back and says why", async () => {
    // Given
    const { controller, root } = await mount(
      backend({
        setDriveSaveRecordings: failWith("STATE_ERROR", "설정을 저장하지 못했습니다."),
      }),
    )
    await click(root, '[data-action="open-settings"]')

    // When
    await click(root, "#drive-save-recordings")

    // Then
    expect(toggle(root).checked).toBe(false)
    expect(toggle(root).disabled).toBe(false)
    expect(pick(root, "#settings-message").dataset["state"]).toBe("error")
    expect(pick(root, "#settings-message").textContent).toContain("설정을 저장하지 못했습니다.")
    controller.stop()
  })

  test("the switch is off and disabled until Drive is connected", async () => {
    // Given / When
    const { controller, root } = await mount(backend({ loadDriveStatus: async () => dismissed }))
    await click(root, '[data-action="open-settings"]')

    // Then
    expect(toggle(root).disabled).toBe(true)
    expect(toggle(root).checked).toBe(false)
    controller.stop()
  })
})
