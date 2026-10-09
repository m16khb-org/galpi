import {
  beginJob,
  completeJob,
  cancelJob,
  failJob,
  initialJobState,
  type JobViewState,
  reduceJobEvent,
} from "../application/job-machine"
import {
  type ArtifactKind,
  type BackendPort,
  errorDetail,
  errorMessage,
} from "../domain/backend"
import type { ComputeDevice, EnginePreset, ImportedTranscript, TranscriptionResult } from "../domain/job"
import { buildSpeakerHint, type SpeakerHint } from "../domain/speaker"
import type { AppView } from "./app-view"
import { ChatGptController } from "./chatgpt-controller"
import { RecordingController } from "./recording-controller"

export class AppController {
  private readonly backend: BackendPort
  private readonly view: AppView
  private audioPath: string | null = null
  private outputRoot: string | null = null
  private job: JobViewState = initialJobState
  private lastResult: TranscriptionResult | ImportedTranscript | null = null
  private readonly recording: RecordingController
  private readonly chatgpt: ChatGptController
  private unlisten: (() => void) | null = null
  private unlistenRecording: (() => void) | null = null
  private settingsSavePending = false
  private settingsSaveActive = false

  constructor(backend: BackendPort, view: AppView) {
    this.backend = backend
    this.view = view
    this.recording = new RecordingController(backend, view, (path) => {
      this.audioPath = path
      this.view.setAudio(path)
    })
    this.chatgpt = new ChatGptController(backend, view, () => this.requestSettingsSave())
  }

  async start(): Promise<void> {
    // Bind controls before any await: a failed native subscription must not
    // leave the shell inert and silent.
    this.bind()
    this.recording.render()
    try {
      this.unlisten = await this.backend.listenToJobs((event) => {
        this.job = reduceJobEvent(this.job, event)
        this.view.renderJob(this.job)
      })
      this.unlistenRecording = await this.backend.listenToRecordingFailures((event) => {
        void this.recording.handleFailure(event)
      })
      // Before any sign-in call: a phase event with no listener is lost for good.
      await this.chatgpt.subscribe()
    } catch {
      // Without the event channel no other IPC call can succeed either; stop
      // here with a visible message instead of piling up raw IPC errors.
      this.view.showError("네이티브 런타임에 연결할 수 없습니다. 앱을 다시 실행해 주세요.")
      return
    }
    try {
      const environment = await this.backend.diagnose()
      this.outputRoot = environment.defaultOutputDirectory
      this.view.setOutput(environment.defaultOutputDirectory)
      this.view.setEnvironment(environment)
      this.view.tokenSettings.setStored(await this.backend.huggingFaceTokenStored())
      const assistant = await this.backend.loadAssistantSettings()
      this.view.assistantSettings.setStored(assistant.apiKeyStored)
      this.view.setAssistantKeyReady(this.chatgpt.ready(assistant.apiKeyStored))
      this.view.participantSettings.setRoster(assistant.participants)
      this.view.glossarySettings.setEntries(assistant.glossary)
      this.view.attendees.setRoster(assistant.participants)
      await this.chatgpt.load()
    } catch (error) {
      this.view.showError(errorMessage(error))
    }
  }

  stop(): void {
    this.recording.dispose()
    this.chatgpt.dispose()
    this.unlisten?.()
    this.unlisten = null
    this.unlistenRecording?.()
    this.unlistenRecording = null
  }

  private bind(): void {
    this.view.on("prepare", () => void this.prepare())
    this.view.on("open-settings", () => void this.openSettings())
    this.view.on("close-settings", () => this.view.tokenSettings.close())
    this.view.on("toggle-token-visibility", () => this.view.tokenSettings.toggleVisibility())
    this.view.on("toggle-assistant-visibility", () =>
      this.view.assistantSettings.toggleVisibility(),
    )
    this.view.onSettingsChange(() => this.requestSettingsSave())
    this.view.on("add-participant", () => this.view.participantSettings.addRow())
    this.view.on("add-glossary-entry", () => this.view.glossarySettings.addRow())
    this.view.on("clear-attendees", () => this.view.attendees.clear())
    this.view.onEnginePresetChange((preset) => void this.switchEngine(preset))
    this.view.onComputeDeviceChange((device) => void this.switchComputeDevice(device))
    this.view.on("clear-token", () => void this.clearToken())
    this.view.on("clear-assistant-key", () => void this.clearAssistantKey())
    this.view.on("refine", () => void this.refine())
    this.view.on("open-minutes", () => void this.openArtifact("minutes"))
    this.view.on("model-access", () => void this.openModelAccess())
    this.view.on("choose-audio", () => void this.chooseAudio())
    this.view.on("import-transcript", () => void this.importTranscript())
    this.view.on("choose-output", () => void this.chooseOutput())
    this.view.on("transcribe", () => void this.transcribe())
    this.view.on("record", () => void this.recording.start(this.outputRoot))
    this.view.on("stop-recording", () => void this.recording.stop())
    this.view.on("cancel-recording", () => void this.recording.cancel())
    this.view.on("sign-in-chatgpt", () => void this.chatgpt.signIn())
    this.view.on("cancel-chatgpt-sign-in", () => void this.chatgpt.cancelSignIn())
    this.view.on("sign-out-chatgpt", () => void this.chatgpt.signOut())
    this.view.on("open-chatgpt-usage", () => void this.chatgpt.openUsagePage())
    this.view.on("acknowledge-chatgpt-welcome", () => this.chatgpt.acknowledgeWelcome())
    this.view.on("cancel", () => void this.cancel())
    this.view.on("open-srt", () => void this.openArtifact("srt"))
    this.view.on("open-txt", () => void this.openArtifact("speaker_text"))
    this.view.on("open-checkpoint", () => void this.openArtifact("checkpoint"))
    this.view.on("reveal-output", () => void this.revealOutput())
    this.view.onSpeakerMode((mode) => this.view.setSpeakerMode(mode))
  }

  /** Saving the preset re-diagnoses so the setup panel reflects the switch. */
  private async switchEngine(preset: EnginePreset): Promise<void> {
    try {
      await this.backend.saveEnginePreset(preset)
      const environment = await this.backend.diagnose()
      this.view.setEnvironment(environment)
    } catch (error) {
      this.view.showError(errorMessage(error))
    }
  }

  /** Same contract as the preset: save, then re-diagnose so the panel follows. */
  private async switchComputeDevice(device: ComputeDevice): Promise<void> {
    try {
      await this.backend.saveComputeDevice(device)
      const environment = await this.backend.diagnose()
      this.view.setEnvironment(environment)
    } catch (error) {
      this.view.showError(errorMessage(error))
    }
  }

  private async prepare(): Promise<void> {
    // The window mints the id so the very first worker event already belongs to
    // this job; a job that adopts the id of whatever arrives first can inherit
    // the trailing events of a job the user just cancelled.
    const jobId = crypto.randomUUID()
    this.begin("setup", "로컬 엔진과 모델을 준비합니다.", jobId)
    try {
      const result = await this.backend.prepare(jobId)
      this.view.setEnvironment(result.status)
      this.job = {
        ...this.job,
        status: "completed",
        jobId: result.jobId,
        phase: "ready",
        percent: 100,
        message: "로컬 전사 환경이 준비되었습니다.",
      }
      this.view.renderJob(this.job)
    } catch (error) {
      this.handleFailure(error)
    } finally {
      this.view.setBusy(null)
    }
  }

  private async openSettings(): Promise<void> {
    const settings = this.view.tokenSettings
    const assistant = this.view.assistantSettings
    settings.show()
    settings.setBusy(true)
    assistant.setBusy(true)
    settings.showMessage("저장된 설정을 불러오는 중입니다.")
    try {
      settings.setStored(await this.backend.huggingFaceTokenStored())
      const loaded = await this.backend.loadAssistantSettings()
      assistant.setSettings(loaded)
      this.view.setAssistantKeyReady(this.chatgpt.ready(loaded.apiKeyStored))
      this.view.participantSettings.setRoster(loaded.participants)
      this.view.glossarySettings.setEntries(loaded.glossary)
      await this.chatgpt.open()
      settings.showMessage("변경사항은 자동으로 저장됩니다.")
    } catch (error) {
      settings.showMessage(errorMessage(error), "error")
      return
    } finally {
      settings.setBusy(false)
      assistant.setBusy(false)
    }
    await this.chatgpt.showModels()
  }

  private async persistSettings(): Promise<void> {
    const settings = this.view.tokenSettings
    const assistant = this.view.assistantSettings
    settings.showMessage("변경사항을 자동 저장하는 중입니다.", "saving")
    try {
      // Autosave runs on every edit anywhere in the sheet. Saving the token
      // only when the user has actually typed a new one keeps a roster edit
      // from reaching the keychain, which on macOS means a prompt.
      const pending = settings.pendingToken()
      if (pending !== null) {
        await this.backend.saveHuggingFaceToken(pending)
        settings.setToken(pending)
      }
      // The same rule as the token above: the sheet does not hold a stored key,
      // so only one the user just typed is worth sending. The key has its own
      // command, which keeps it out of the settings payload entirely.
      const pendingKey = assistant.pendingKey()
      if (pendingKey !== null) {
        await this.backend.saveAssistantApiKey(pendingKey)
        assistant.setKey(pendingKey)
      }
      const saved = {
        ...assistant.settings(),
        participants: this.view.participantSettings.roster(),
        glossary: this.view.glossarySettings.entries(),
      }
      await this.backend.saveAssistantSettings(saved)
      await this.chatgpt.persist()
      this.view.setAssistantKeyReady(this.chatgpt.ready(saved.apiKeyStored))
      this.view.attendees.setRoster(saved.participants)
      settings.showMessage("변경사항을 자동 저장했습니다.")
    } catch (error) {
      settings.showMessage(
        `${errorMessage(error)} · 수정 내용은 유지되며 다음 변경 때 다시 저장합니다.`,
        "error",
      )
    }
  }

  private requestSettingsSave(): void {
    this.settingsSavePending = true
    if (!this.settingsSaveActive) void this.flushSettingsSaves()
  }

  private async flushSettingsSaves(): Promise<void> {
    this.settingsSaveActive = true
    try {
      while (this.settingsSavePending) {
        this.settingsSavePending = false
        await this.persistSettings()
      }
    } finally {
      this.settingsSaveActive = false
    }
  }

  private async clearToken(): Promise<void> {
    const settings = this.view.tokenSettings
    settings.setBusy(true)
    try {
      await this.backend.saveHuggingFaceToken("")
      settings.clearToken()
      settings.showMessage("저장된 토큰을 지웠습니다.")
    } catch (error) {
      settings.showMessage(errorMessage(error), "error")
    } finally {
      settings.setBusy(false)
    }
  }

  /// Clearing is how a stored key is replaced: the window never receives the
  /// value, so there is nothing to edit in place.
  private async clearAssistantKey(): Promise<void> {
    const settings = this.view.tokenSettings
    const assistant = this.view.assistantSettings
    assistant.setBusy(true)
    try {
      await this.backend.saveAssistantApiKey("")
      assistant.clearKey()
      this.view.setAssistantKeyReady(this.chatgpt.ready(false))
      settings.showMessage("저장된 API 키를 지웠습니다.")
    } catch (error) {
      settings.showMessage(errorMessage(error), "error")
    } finally {
      assistant.setBusy(false)
    }
  }

  private async openModelAccess(): Promise<void> {
    try {
      await this.backend.openModelAccessPage()
    } catch (error) {
      this.view.showError(errorMessage(error))
    }
  }

  private async chooseAudio(): Promise<void> {
    try {
      const selected = await this.backend.chooseAudio()
      if (selected !== null) {
        this.audioPath = selected
        this.view.setAudio(selected)
        this.view.clearError()
      }
    } catch (error) {
      this.view.showError(errorMessage(error))
    }
  }

  private async importTranscript(): Promise<void> {
    if (this.outputRoot === null) {
      this.view.showError("결과를 저장할 폴더를 선택해 주세요.")
      return
    }
    let selected: string | null
    try {
      selected = await this.backend.chooseTranscript(this.outputRoot)
    } catch (error) {
      this.view.showError(errorMessage(error))
      return
    }
    if (selected === null) return
    try {
      const imported = await this.backend.importTranscript({
        jobId: crypto.randomUUID(),
        inputPath: selected,
        outputRoot: this.outputRoot,
      })
      this.lastResult = imported
      this.view.setTranscript(selected)
      this.view.renderImportedTranscript(imported)
    } catch (error) {
      this.view.showError(errorMessage(error))
    }
  }

  private async chooseOutput(): Promise<void> {
    try {
      const selected = await this.backend.chooseOutputDirectory()
      if (selected !== null) {
        this.outputRoot = selected
        this.view.setOutput(selected)
      }
    } catch (error) {
      this.view.showError(errorMessage(error))
    }
  }

  private async transcribe(): Promise<void> {
    if (this.audioPath === null) {
      this.view.showError("먼저 전사할 오디오 파일을 선택해 주세요.")
      return
    }
    if (this.outputRoot === null) {
      this.view.showError("결과를 저장할 폴더를 선택해 주세요.")
      return
    }

    let speakerHint: SpeakerHint
    try {
      speakerHint = buildSpeakerHint(this.view.speakerForm())
    } catch (error) {
      this.view.showError(errorMessage(error))
      return
    }

    const jobId = crypto.randomUUID()
    this.begin("transcription", "오디오를 분석할 준비를 합니다.", jobId)
    try {
      const result = await this.backend.transcribe({
        jobId,
        inputPath: this.audioPath,
        outputRoot: this.outputRoot,
        speakerHint,
      })
      this.lastResult = result
      this.job = completeJob(this.job, result)
      this.view.renderJob(this.job)
      this.view.renderResult(result)
    } catch (error) {
      this.handleFailure(error)
    } finally {
      this.view.setBusy(null)
    }
  }

  private async cancel(): Promise<void> {
    if (this.job.jobId === null) {
      this.view.showError("취소할 작업 ID를 아직 받지 못했습니다.")
      return
    }
    try {
      await this.backend.cancel(this.job.jobId)
      this.job = cancelJob(this.job, "작업 취소를 요청했습니다.")
      this.view.renderJob(this.job)
    } catch (error) {
      this.view.showError(errorMessage(error))
    }
  }

  private async refine(): Promise<void> {
    const result = this.lastResult
    if (result === null) {
      this.view.showError("먼저 전사를 완료해 주세요.")
      return
    }
    const jobId = crypto.randomUUID()
    this.begin("refinement", "저장한 사전 정보로 회의록을 만듭니다.", jobId)
    try {
      const refined = await this.backend.refineTranscript(
        jobId,
        result.jobId,
        this.view.attendees.selectedIds(),
      )
      this.job = {
        ...this.job,
        status: "completed",
        jobId: refined.jobId,
        phase: "writing",
        percent: 100,
        message: "회의록을 저장했습니다.",
      }
      this.view.renderJob(this.job)
      this.view.renderMinutes(refined.minutes)
    } catch (error) {
      this.handleFailure(error)
    } finally {
      this.view.setBusy(null)
    }
  }

  private async openArtifact(kind: ArtifactKind): Promise<void> {
    const jobId = this.lastResult?.jobId
    if (jobId === undefined) return
    try {
      await this.backend.openArtifact(jobId, kind)
    } catch (error) {
      this.view.showError(errorMessage(error))
    }
  }

  private async revealOutput(): Promise<void> {
    const jobId = this.lastResult?.jobId
    if (jobId === undefined) return
    try {
      await this.backend.revealOutput(jobId)
    } catch (error) {
      this.view.showError(errorMessage(error))
    }
  }

  private begin(
    kind: "setup" | "transcription" | "refinement",
    message: string,
    jobId: string | null = null,
  ): void {
    this.view.clearError()
    this.chatgpt.clearLimitHint()
    this.job = beginJob(message, jobId)
    this.view.renderJob(this.job)
    this.view.setBusy(kind)
  }

  private handleFailure(error: unknown): void {
    const message = errorMessage(error)
    this.chatgpt.noteFailure(error)
    const detail = errorDetail(error)
    // Raw non-AppError diagnostics stay inspectable in the log disclosure.
    const logs = detail === message ? this.job.logs : [...this.job.logs, `[frontend] ${detail}`]
    this.job = failJob({ ...this.job, logs }, message)
    this.view.renderJob(this.job)
  }
}
