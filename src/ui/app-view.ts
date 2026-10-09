import type { JobStatus, JobViewState } from "../application/job-machine"
import type { RecordingViewState } from "../application/recording-machine"
import type {
  ComputeDevice,
  EnginePreset,
  EnvironmentStatus,
  ImportedTranscript,
  TranscriptionResult,
} from "../domain/job"
import type { SpeakerForm, SpeakerMode } from "../domain/speaker"
import { appTemplate } from "./app-template"
import { AssistantSettingsView } from "./assistant-settings"
import { ChatGptSettingsView } from "./chatgpt-settings"
import { GlossarySettingsView } from "./glossary-settings"
import { ParticipantPickerView } from "./participant-picker"
import { ParticipantSettingsView } from "./participant-settings"
import { setActionButtonVariant } from "./seed"
import { bindTokenGuide } from "./token-guide"
import { TokenSettingsView } from "./token-settings"

export type BusyKind = "setup" | "transcription" | "refinement" | null

export class AppView {
  readonly root: HTMLElement
  readonly tokenSettings: TokenSettingsView
  readonly assistantSettings: AssistantSettingsView
  readonly chatGptSettings: ChatGptSettingsView
  readonly participantSettings: ParticipantSettingsView
  readonly glossarySettings: GlossarySettingsView
  readonly attendees: ParticipantPickerView
  private environmentChecked = false
  private engineReady = false
  private jobBusy = false
  private jobKind: BusyKind = null
  /// Selector lookups for the static template, resolved once each.
  private readonly elements = new Map<string, HTMLElement>()
  /// What the log and phase slots were last rendered from. A worker under load
  /// emits progress far faster than either of them actually changes.
  private renderedLogs: readonly string[] | null = null
  private renderedPhase: string | null = null
  private recordingActive = false
  private hasResult = false
  private importedResult = false
  private minutesReady = false
  /// A finished transcription keeps its one-line summary until the next run.
  private transcriptionSummary = false
  private jobPhase = ""
  /// The meeting the top bar names: the chosen audio, or an imported transcript.
  private meetingName: string | null = null
  /// New audio chosen after a result makes starting the main task again.
  private audioSinceResult = false
  private assistantKeyReady = false
  private settingsChangeHandler: (() => void) | null = null

  constructor(root: HTMLElement) {
    this.root = root
    this.root.innerHTML = appTemplate
    this.tokenSettings = new TokenSettingsView(root)
    this.assistantSettings = new AssistantSettingsView(root)
    this.chatGptSettings = new ChatGptSettingsView(root)
    this.participantSettings = new ParticipantSettingsView(root, () => {
      this.attendees.setRoster(this.participantSettings.roster())
      this.settingsChangeHandler?.()
    })
    this.glossarySettings = new GlossarySettingsView(root, () => this.settingsChangeHandler?.())
    this.attendees = new ParticipantPickerView(root, (count) => this.applyAttendeeCount(count))
    bindTokenGuide(this.root)
  }

  /** Selecting attendees fills the speaker count; a later manual change is left alone. */
  applyAttendeeCount(count: number): void {
    const mode: SpeakerMode = count > 0 ? "exact" : "auto"
    for (const input of this.root.querySelectorAll<HTMLInputElement>(
      'input[name="speaker-mode"]',
    )) {
      input.checked = input.value === mode
    }
    this.setSpeakerMode(mode)
    if (count > 0) {
      this.element<HTMLInputElement>("#exact-speakers").value = String(count)
    }
    this.element("#speaker-hint-note").textContent =
      count > 0
        ? `참석자 ${count}명으로 맞췄습니다. 필요하면 직접 바꿀 수 있습니다.`
        : "참석 인원을 모르면 자동을 선택해도 됩니다."
  }

  on(action: string, handler: () => void): void {
    for (const element of this.root.querySelectorAll<HTMLElement>(`[data-action="${action}"]`)) {
      element.addEventListener("click", handler)
    }
  }

  onSettingsChange(handler: () => void): void {
    this.settingsChangeHandler = handler
    for (const selector of [
      "#settings-hf-token",
      "#settings-assistant-key",
      "#settings-assistant-model",
      "#settings-assistant-effort",
      "#settings-assistant-base-url",
      "#settings-assistant-background",
      "#settings-chatgpt-model",
    ]) {
      this.element(selector).addEventListener("change", handler)
    }
    for (const input of this.root.querySelectorAll<HTMLInputElement>(
      'input[name="assistant-auth-mode"]',
    )) {
      input.addEventListener("change", handler)
    }
  }

  onSpeakerMode(handler: (mode: SpeakerMode) => void): void {
    for (const input of this.root.querySelectorAll<HTMLInputElement>(
      'input[name="speaker-mode"]',
    )) {
      input.addEventListener("change", () => {
        if (input.checked) handler(input.value as SpeakerMode)
      })
    }
  }

  /** Preset switching saves immediately and re-diagnoses; no Apply step. */
  onEnginePresetChange(handler: (preset: EnginePreset) => void): void {
    for (const input of this.root.querySelectorAll<HTMLInputElement>(
      'input[name="engine-preset"]',
    )) {
      input.addEventListener("change", () => {
        if (input.checked) handler(input.value as EnginePreset)
      })
    }
  }

  /** Device switching saves immediately like the preset; CUDA needs a re-prepare afterwards. */
  onComputeDeviceChange(handler: (device: ComputeDevice) => void): void {
    for (const input of this.root.querySelectorAll<HTMLInputElement>(
      'input[name="compute-device"]',
    )) {
      input.addEventListener("change", () => {
        if (input.checked) handler(input.value as ComputeDevice)
      })
    }
  }

  speakerForm(): SpeakerForm {
    const checked = this.root.querySelector<HTMLInputElement>('input[name="speaker-mode"]:checked')
    return {
      mode: (checked?.value ?? "auto") as SpeakerMode,
      exact: this.numberValue("#exact-speakers"),
      min: this.numberValue("#min-speakers"),
      max: this.numberValue("#max-speakers"),
    }
  }

  setEnvironment(status: EnvironmentStatus): void {
    for (const input of this.root.querySelectorAll<HTMLInputElement>(
      'input[name="engine-preset"]',
    )) {
      input.checked = input.value === status.enginePreset
    }
    const defaultPreset = status.availablePresets[0]
    for (const preset of ["qwen3", "whisperx"] as const) {
      this.element(`[data-engine-option="${preset}"]`).hidden =
        !status.availablePresets.includes(preset)
      this.setEngineBadge(
        `#engine-${preset}-state`,
        preset === defaultPreset ? "기본" : "이전 엔진",
        preset === "qwen3" ? status.qwen3Ready : status.whisperxReady,
      )
    }
    this.setComputeDevice(status)
    this.element("#engine-settings-state").textContent =
      status.enginePreset === "qwen3" ? "Qwen3" : "WhisperX"
    const engineLabel =
      status.enginePreset === "qwen3" ? "Qwen3 엔진" : "WhisperX 엔진"
    this.statusRow("#engine-check", status.engineReady, engineLabel)
    this.statusRow("#model-check", status.modelsReady, "전사·정렬·화자분리 모델")
    this.statusRow("#ffmpeg-check", status.ffmpegReady, "내장 ffmpeg")
    const ready = status.engineReady && status.modelsReady && status.ffmpegReady
    this.environmentChecked = true
    this.engineReady = ready
    this.element("#setup-state").textContent = ready ? "준비 완료" : "설정 필요"
    this.element("#setup-state").dataset["state"] = ready ? "ready" : "pending"
    this.element<HTMLButtonElement>("#prepare-button").textContent = ready
      ? "준비 상태 다시 확인"
      : "로컬 엔진 준비"
    this.element("#engine-version").textContent = status.engineVersion
    this.applyOnboarding()
    this.refreshActions()
  }

  /** Reflect whether an assistant key is saved; the augment stage depends on it. */
  setAssistantKeyReady(ready: boolean): void {
    this.assistantKeyReady = ready
    this.element("#augment-key-hint").hidden = ready
    this.refreshAugment()
    this.refreshActions()
  }

  setAudio(path: string): void {
    this.path("#audio-path", path)
    this.element("#audio-selection").dataset["selected"] = "true"
    this.meetingName = meetingName(path)
    this.audioSinceResult = this.hasResult
    this.refreshStages()
    this.refreshActions()
  }

  /** Once a transcript is chosen the picker shows it as the augment target. */
  setTranscript(path: string): void {
    this.path("#transcript-path", path)
    this.element("#transcript-selection").dataset["selected"] = "true"
    this.element("#transcript-title").textContent = "대상 전사문"
  }

  setOutput(path: string): void {
    this.path("#output-path", path)
  }

  setSpeakerMode(mode: SpeakerMode): void {
    this.element("#exact-fields").hidden = mode !== "exact"
    this.element("#range-fields").hidden = mode !== "range"
  }

  setBusy(kind: BusyKind): void {
    const busy = kind !== null
    this.jobBusy = busy
    if (kind !== null) {
      this.jobKind = kind
      if (kind === "transcription") {
        this.transcriptionSummary = false
        this.element("#job-panel").dataset["status"] = "running"
      }
      this.element("#setup-progress-panel").hidden = kind !== "setup"
      // Each stage renders its own progress card in place: refinement inside
      // the augment panel, transcription inside the transcription panel.
      this.element("#job-panel").hidden = kind !== "transcription" && !this.transcriptionSummary
      this.element("#job-phase-list").hidden = kind !== "transcription"
      this.element("#augment-progress").hidden = kind !== "refinement"
    }
    this.element<HTMLButtonElement>("#cancel-button").hidden =
      !busy || this.jobKind !== "transcription"
    this.element<HTMLButtonElement>("#setup-cancel-button").hidden =
      !busy || this.jobKind !== "setup"
    this.element<HTMLButtonElement>("#augment-cancel-button").hidden =
      !busy || this.jobKind !== "refinement"
    this.element("#busy-label").textContent = busyLabel(kind)
    this.applyOnboarding()
    this.refreshActions()
  }
  setRecording(state: RecordingViewState): void {
    this.recordingActive =
      state.status === "starting" || state.status === "recording" || state.status === "stopping"
    this.element("#recorder").dataset["state"] = state.status
    this.element("#recorder").dataset["warning"] = String(state.warning)
    this.element<HTMLButtonElement>("#record-button").hidden = this.recordingActive
    this.element("#recording-active").hidden = !this.recordingActive
    this.element("#recording-status").textContent = state.message
    this.element("#recording-label").textContent =
      state.status === "starting"
        ? "마이크 연결 중"
        : state.status === "stopping"
          ? "저장 중"
          : "녹음 중"
    this.element("#recording-path").textContent = state.path ?? "마이크 입력을 저장합니다."
    this.setRecordingTime(state.elapsedSeconds)
    this.element<HTMLButtonElement>("#stop-recording-button").disabled =
      state.status === "starting" || state.status === "stopping"
    this.element<HTMLButtonElement>("#cancel-recording-button").disabled =
      state.status === "starting" || state.status === "stopping"
    this.refreshStages()
    this.refreshActions()
  }

  /// Update only the clock, which is all that changes on a recording tick.
  setRecordingTime(elapsedSeconds: number): void {
    const clock = this.element("#recording-time")
    clock.textContent = formatElapsed(elapsedSeconds)
    clock.setAttribute("datetime", `PT${Math.max(0, Math.round(elapsedSeconds))}S`)
  }

  renderJob(state: JobViewState): void {
    this.element("#setup-progress-panel").hidden =
      state.status === "idle" || this.jobKind !== "setup"
    this.element("#job-panel").hidden =
      (state.status === "idle" || this.jobKind !== "transcription") && !this.transcriptionSummary
    this.element("#augment-progress").hidden =
      state.status === "idle" || this.jobKind !== "refinement"
    if (!this.transcriptionSummary && this.jobKind === "transcription") {
      this.element("#job-panel").dataset["status"] = state.status
    }
    this.jobPhase = state.phase
    this.renderJobTitle(state.status)
    this.refreshStages()
    this.element("#job-message").textContent = state.message
    this.element("#setup-job-message").textContent = state.message
    this.element("#augment-job-message").textContent = state.message
    this.element("#job-percent").textContent = `${Math.round(state.percent)}%`
    this.element("#setup-job-percent").textContent = `${Math.round(state.percent)}%`
    this.element("#augment-job-percent").textContent = `${Math.round(state.percent)}%`
    this.renderProgress("#job-progress", state.percent)
    this.renderProgress("#setup-job-progress", state.percent)
    this.renderProgress("#augment-job-progress", state.percent)
    if (this.renderedPhase !== state.phase) {
      this.renderedPhase = state.phase
      for (const item of this.root.querySelectorAll<HTMLElement>("[data-phase]")) {
        item.dataset["state"] = phaseState(item.dataset["phase"] ?? "", state.phase)
      }
      for (const item of this.root.querySelectorAll<HTMLElement>("[data-setup-phase]")) {
        item.dataset["state"] = phaseState(item.dataset["setupPhase"] ?? "", state.phase)
      }
    }
    if (this.renderedLogs !== state.logs) {
      this.renderedLogs = state.logs
      const text = state.logs.join("\n")
      this.element("#log-output").textContent = text
      this.element("#setup-log-output").textContent = text
    }
    this.element("#error-message").textContent = state.error ?? ""
    this.element("#error-message").hidden = state.error === null
    this.element("#setup-error-message").textContent = state.error ?? ""
    this.element("#setup-error-message").hidden = state.error === null
    this.element("#augment-error-message").textContent = state.error ?? ""
    this.element("#augment-error-message").hidden = state.error === null
  }

  renderResult(result: TranscriptionResult): void {
    this.element("#results-panel").hidden = false
    const summary = `${result.segments}개 발화 보존 · ${result.filtered}개 환각 제거`
    this.element("#result-summary").textContent = summary
    this.element("#job-summary-text").textContent = `전사 완료 · ${summary}`
    this.element("#job-panel").dataset["status"] = "completed"
    this.transcriptionSummary = true
    this.element("#result-srt-row").hidden = false
    const hasCheckpoint = result.checkpoint !== null
    this.element("#result-checkpoint-row").hidden = !hasCheckpoint
    this.path("#result-srt", result.srt)
    this.path("#result-txt", result.txt)
    // Augmentation runs on the transcript this job just published, so the
    // picker shows it instead of asking for a file that is already here.
    this.setTranscript(result.txt)
    if (hasCheckpoint) {
      this.path("#result-checkpoint", result.checkpoint)
    }
    this.startMeeting(false)
  }

  /** An imported transcript is a result too: augmentation starts from it. */
  renderImportedTranscript(result: ImportedTranscript): void {
    this.element("#results-panel").hidden = false
    this.element("#result-summary").textContent = "가져온 전사문 · AI 증강 준비 완료"
    this.element("#result-srt-row").hidden = true
    this.element("#result-checkpoint-row").hidden = true
    this.path("#result-txt", result.txt)
    this.meetingName = meetingName(result.txt)
    // The summary belongs to the previous meeting's run, not this transcript.
    this.transcriptionSummary = false
    this.element("#job-panel").hidden = true
    this.startMeeting(true)
  }

  renderMinutes(minutes: string): void {
    // The finished minutes row is the completion state; the progress block
    // hands over to it instead of lingering at 100%.
    this.element("#augment-progress").hidden = true
    this.element("#augment-panel").hidden = false
    this.element("#result-minutes-row").hidden = false
    this.path("#result-minutes", minutes)
    this.minutesReady = true
    this.refreshStages()
    this.refreshActions()
  }

  clearError(): void {
    const banner = this.element("#app-error")
    banner.textContent = ""
    banner.hidden = true
  }

  showError(message: string): void {
    // Transient action errors land in the persistent app banner: the in-panel
    // error slots live inside progress cards that are hidden while idle.
    const banner = this.element("#app-error")
    banner.textContent = message
    banner.hidden = false
  }

  element<T extends HTMLElement = HTMLElement>(selector: string): T {
    const cached = this.elements.get(selector)
    if (cached !== undefined) return cached as T
    const element = this.root.querySelector<T>(selector)
    if (element === null) {
      throw new Error(`필수 UI 요소를 찾지 못했습니다: ${selector}`)
    }
    this.elements.set(selector, element)
    return element
  }

  private numberValue(selector: string): number {
    return Number.parseInt(this.element<HTMLInputElement>(selector).value, 10)
  }

  /** Picker badges pair the engine role with its readiness, never color alone. */
  private setComputeDevice(status: EnvironmentStatus): void {
    this.element("#engine-device").hidden = !(
      status.availableDevices.length > 0 && status.enginePreset === "whisperx"
    )
    for (const input of this.root.querySelectorAll<HTMLInputElement>(
      'input[name="compute-device"]',
    )) {
      input.checked = input.value === status.computeDevice
    }
    const cuda = this.element<HTMLInputElement>('input[name="compute-device"][value="cuda"]')
    cuda.disabled = !status.cudaDriverDetected
    this.element("#engine-device-cuda-note").textContent = status.cudaDriverDetected
      ? ""
      : "NVIDIA 드라이버 필요"
  }

  private setEngineBadge(selector: string, role: string, ready: boolean): void {
    this.element(selector).textContent = `${role} · ${ready ? "준비됨" : "준비 필요"}`
    this.element(selector).dataset["state"] = ready ? "ready" : "pending"
  }

  private statusRow(selector: string, ready: boolean, label: string): void {
    const row = this.element(selector)
    row.dataset["state"] = ready ? "ready" : "pending"
    row.querySelector<HTMLElement>("[data-status-label]")?.replaceChildren(label)
    row
      .querySelector<HTMLElement>("[data-status-value]")
      ?.replaceChildren(ready ? "준비됨" : "대기")
  }

  private path(selector: string, value: string): void {
    const element = this.element(selector)
    element.textContent = value
    element.setAttribute("title", value)
  }

  /** A new result is a new meeting: the previous minutes no longer describe it. */
  private startMeeting(imported: boolean): void {
    this.element("#result-minutes-row").hidden = true
    this.element("#augment-panel").hidden = false
    this.element("#augment-waiting").hidden = true
    this.hasResult = true
    this.importedResult = imported
    this.minutesReady = false
    this.audioSinceResult = false
    this.refreshStages()
    this.refreshActions()
  }

  /** Only the card of the job that ran is visible, so only its title follows the outcome. */
  private renderJobTitle(status: JobStatus): void {
    if (this.jobKind === "setup") {
      this.element("#setup-job-title").textContent = setupTitles[status]
    } else if (this.jobKind === "transcription") {
      this.element("#job-title").textContent = transcriptionTitles[status]
    } else if (this.jobKind === "refinement") {
      this.element("#augment-job-title").textContent = refinementTitles[status]
    }
  }

  private renderProgress(selector: string, percent: number): void {
    const progress = this.element<HTMLElement>(selector)
    progress.setAttribute("aria-valuenow", String(Math.round(percent)))
    progress.style.setProperty("--progress", `${Math.max(0, Math.min(100, percent))}%`)
  }

  private applyOnboarding(): void {
    // 준비가 끝난 사용자에게는 엔진·모델 준비 패널을 감춘다. 이번 세션에서
    // 준비를 직접 실행했다면 완료 메시지를 볼 수 있도록 그대로 둔다.
    const onboarded = this.engineReady && this.jobKind !== "setup"
    this.element("#setup-panel").hidden = onboarded
    this.refreshStages()
    this.refreshAugment()
  }

  /** The rail mirrors the three user stages; engine setup is a pre-gate, not a stage. */
  private refreshStages(): void {
    const busyTranscribing = this.jobBusy && this.jobKind === "transcription"
    const transcribing = busyTranscribing || this.recordingActive
    this.setStage(
      "#step-transcribe",
      this.hasResult ? "complete" : transcribing || this.engineReady ? "current" : "pending",
    )
    this.setStage("#step-results", this.hasResult ? "current" : "pending")
    this.setStage("#step-augment", this.minutesReady ? "complete" : "pending")
    this.refreshTask()
  }

  /** The top bar names the meeting being worked on and where it stands. */
  private refreshTask(): void {
    const [title, status] = this.task()
    this.element("#task-title").textContent = title
    this.element("#task-status").textContent = status
  }

  private task(): readonly [string, string] {
    if (!this.environmentChecked) return ["새 회의 전사", "로컬 환경을 확인하는 중입니다."]
    if (!this.engineReady) {
      return [
        "로컬 엔진을 준비해 주세요",
        this.jobBusy && this.jobKind === "setup"
          ? "로컬 환경을 준비하고 있습니다"
          : "처음 한 번만 필요합니다 · 모델 약 3GB",
      ]
    }
    const title = this.meetingName ?? "새 회의 전사"
    if (this.recordingActive) return [title, "녹음 중"]
    if (this.jobBusy && this.jobKind === "transcription") {
      const phase = phaseLabels[this.jobPhase]
      return [title, phase === undefined ? "전사 중" : `전사 중 · ${phase} 단계`]
    }
    if (this.jobBusy && this.jobKind === "refinement") return [title, "회의록 작성 중"]
    if (this.hasResult && !this.audioSinceResult) {
      if (this.minutesReady) return [title, "회의록 완료"]
      return [title, this.importedResult ? "전사문 가져옴 · 회의록 작성 대기" : "전사 완료 · 회의록 작성 대기"]
    }
    if (this.meetingName !== null) return [title, "전사 준비됨"]
    return [title, "녹음하거나 오디오 파일을 선택하세요"]
  }

  /** data-state drives styling; aria-current carries the same fact to assistive tech. */
  private setStage(selector: string, state: string): void {
    const step = this.element(selector)
    step.dataset["state"] = state
    const label = step.querySelector<HTMLElement>("[data-step-state]")
    if (label !== null) label.textContent = stageLabels[state] ?? state
    if (state === "current") {
      step.setAttribute("aria-current", "step")
    } else {
      step.removeAttribute("aria-current")
    }
  }

  private refreshAugment(): void {
    this.element("#augment-waiting").hidden = this.hasResult
    this.refreshStages()
  }

  private refreshActions(): void {
    this.element<HTMLButtonElement>("#prepare-button").disabled =
      this.jobBusy || this.recordingActive
    this.element<HTMLButtonElement>("#start-button").disabled =
      !this.engineReady || this.jobBusy || this.recordingActive
    // After a result the main task is the result itself, until new audio arrives.
    const restart = this.hasResult && !this.audioSinceResult
    setPrimary(this.element("#start-button"), !restart)
    this.element("#start-label").textContent = restart ? "새 전사 시작" : "전사 시작"
    this.element("#open-minutes-button").hidden = !this.minutesReady
    setPrimary(this.element("#refine-button"), !this.minutesReady)
    this.element("#refine-label").textContent = this.minutesReady ? "다시 증강" : "AI 증강 실행"
    this.attendees.setBusy(this.jobBusy || this.recordingActive)
    this.element<HTMLButtonElement>("#record-button").disabled = this.jobBusy
    this.element<HTMLButtonElement>("#audio-selection").disabled =
      this.jobBusy || this.recordingActive
    this.element<HTMLButtonElement>("#transcript-selection").disabled =
      this.jobBusy || this.recordingActive
    this.element<HTMLButtonElement>("#output-button").disabled =
      this.jobBusy || this.recordingActive
    this.element<HTMLButtonElement>("#refine-button").disabled =
      !this.hasResult || !this.assistantKeyReady || this.jobBusy || this.recordingActive
  }
}

const stageLabels: Readonly<Record<string, string>> = {
  complete: "완료",
  current: "현재 단계",
  pending: "대기",
}

const phaseLabels: Readonly<Record<string, string>> = {
  transcribing: "전사",
  aligning: "정렬",
  diarizing: "화자분리",
  writing: "결과 저장",
}

type JobTitles = Readonly<Record<JobStatus, string>>

const setupTitles: JobTitles = {
  idle: "로컬 환경을 준비하고 있습니다",
  running: "로컬 환경을 준비하고 있습니다",
  completed: "로컬 환경을 준비했습니다",
  failed: "로컬 환경을 준비하지 못했습니다",
  cancelled: "로컬 환경 준비를 취소했습니다",
}

const transcriptionTitles: JobTitles = {
  idle: "회의를 전사하고 있습니다",
  running: "회의를 전사하고 있습니다",
  completed: "전사를 마쳤습니다",
  failed: "전사하지 못했습니다",
  cancelled: "전사를 취소했습니다",
}

const refinementTitles: JobTitles = {
  idle: "회의록을 작성하고 있습니다",
  running: "회의록을 작성하고 있습니다",
  completed: "회의록을 작성했습니다",
  failed: "회의록을 작성하지 못했습니다",
  cancelled: "회의록 작성을 취소했습니다",
}

function setPrimary(button: HTMLElement, primary: boolean): void {
  button.classList.toggle("primary-button", primary)
  button.classList.toggle("secondary-button", !primary)
  setActionButtonVariant(button, primary ? "brandSolid" : "neutralOutline")
}

/** "/in/2026-10-03 주간 회의.m4a" → "2026-10-03 주간 회의" */
function meetingName(path: string): string {
  const file = path.split(/[\\/]/).at(-1) ?? path
  const dot = file.lastIndexOf(".")
  return dot > 0 ? file.slice(0, dot) : file
}

function busyLabel(kind: BusyKind): string {
  if (kind === "setup") return "로컬 환경 준비 중"
  if (kind === "transcription") return "회의 전사 중"
  return kind === "refinement" ? "회의록 만드는 중" : ""
}

const phaseOrder = [
  "engine",
  "models",
  "transcribing",
  "aligning",
  "diarizing",
  "refining",
  "writing",
  "ready",
]

function phaseState(phase: string, current: string): string {
  const phaseIndex = phaseOrder.indexOf(phase)
  const currentIndex = phaseOrder.indexOf(current)
  if (phaseIndex < currentIndex) return "complete"
  if (phaseIndex === currentIndex) return "current"
  return "pending"
}

function formatElapsed(seconds: number): string {
  const minutes = Math.floor(seconds / 60)
  const remainder = seconds % 60
  return `${String(minutes).padStart(2, "0")}:${String(remainder).padStart(2, "0")}`
}
