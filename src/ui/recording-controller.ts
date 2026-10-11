import {
  beginRecordingState,
  cancelRecordingState,
  canStartRecording,
  completeRecordingState,
  failRecordingState,
  initialRecordingState,
  type RecordingViewState,
  startRecordingState,
  stopRecordingState,
  tickRecording,
} from "../application/recording-machine"
import { type BackendPort, errorMessage, type RecordingFailure } from "../domain/backend"
import { defaultRecordingSource, type RecordingSource } from "../domain/recording"
import type { AppView } from "./app-view"

export class RecordingController {
  private state: RecordingViewState = initialRecordingState
  private timer: number | null = null
  private readonly pendingFailures = new Map<string, RecordingFailure>()
  // The webview suspends interval callbacks while the window is hidden, so a
  // visibility change is the first moment the frozen counter can catch up.
  private readonly refresh = (): void => this.tick()
  private source: RecordingSource = defaultRecordingSource
  /** The latest save of the source choice; it never rejects. */
  private sourceSaved: Promise<void> = Promise.resolve()

  constructor(
    private readonly backend: BackendPort,
    private readonly view: AppView,
    private readonly selectAudio: (path: string) => void,
    /** Told once per recording that stopped cleanly, after the audio is selected. */
    private readonly onRecorded: (recordingId: string) => void = () => undefined,
    private readonly now: () => number = () => Date.now(),
  ) {}

  render(): void {
    this.view.setRecording(this.state)
  }

  /** Puts the stored choice into the control; a read failure keeps the default. */
  async loadSource(): Promise<void> {
    try {
      this.source = await this.backend.loadRecordingSource()
    } catch (error) {
      this.view.showError(errorMessage(error))
    }
    this.view.setRecordingSource(this.source)
  }

  /** Shows the choice at once and saves it; a save that fails puts the old choice back. */
  chooseSource(source: RecordingSource): Promise<void> {
    const previous = this.source
    this.source = source
    this.view.setRecordingSource(source)
    this.sourceSaved = this.backend.saveRecordingSource(source).catch((error: unknown) => {
      // The control must not promise a source the next launch will not remember.
      if (this.source === source) {
        this.source = previous
        this.view.setRecordingSource(previous)
      }
      this.view.showError(errorMessage(error))
    })
    return this.sourceSaved
  }

  async start(outputRoot: string | null): Promise<void> {
    if (!canStartRecording(this.state)) return
    if (outputRoot === null) {
      this.view.showError("녹음을 저장할 출력 폴더를 먼저 선택해 주세요.")
      return
    }
    this.pendingFailures.clear()
    this.state = beginRecordingState()
    this.render()
    // A recording never races the save of the choice it starts with.
    await this.sourceSaved
    try {
      const started = await this.backend.startRecording(outputRoot, this.source)
      this.state = startRecordingState(started.recordingId, started.path, this.now())
      this.render()
      const earlyFailure = this.pendingFailures.get(started.recordingId)
      this.pendingFailures.clear()
      if (earlyFailure !== undefined) {
        await this.cleanupFailure(earlyFailure)
        return
      }
      this.timer = window.setInterval(() => this.tick(), 1_000)
      document.addEventListener("visibilitychange", this.refresh)
      window.addEventListener("focus", this.refresh)
    } catch (error) {
      this.pendingFailures.clear()
      this.state = failRecordingState(errorMessage(error))
      this.render()
    }
  }

  async stop(): Promise<void> {
    if (this.state.status !== "recording") return
    const recordingId = this.state.recordingId
    if (recordingId === null) return
    this.clearTimer()
    this.state = stopRecordingState(this.state)
    this.render()
    let recorded = false
    try {
      const result = await this.backend.stopRecording(recordingId)
      this.selectAudio(result.path)
      this.state = completeRecordingState(
        this.state,
        result.path,
        result.durationSeconds,
        result.droppedFrames,
      )
      recorded = true
    } catch (error) {
      this.state = failRecordingState(errorMessage(error))
    }
    this.render()
    // Last, and only for a recording that exists: what follows never decides its outcome.
    if (recorded) this.onRecorded(recordingId)
  }

  async cancel(): Promise<void> {
    if (this.state.status !== "recording") return
    const recordingId = this.state.recordingId
    if (recordingId === null) return
    this.clearTimer()
    this.state = cancelRecordingState(this.state)
    this.render()
    try {
      await this.backend.cancelRecording(recordingId)
      this.state = initialRecordingState
    } catch (error) {
      this.state = failRecordingState(errorMessage(error))
    }
    this.render()
  }

  async handleFailure(event: RecordingFailure): Promise<void> {
    if (this.state.status === "starting") {
      this.pendingFailures.set(event.recordingId, event)
      return
    }
    if (event.recordingId !== this.state.recordingId) return
    await this.cleanupFailure(event)
  }

  private async cleanupFailure(event: RecordingFailure): Promise<void> {
    this.clearTimer()
    this.state = cancelRecordingState({
      ...this.state,
      recordingId: event.recordingId,
    })
    this.render()
    try {
      await this.backend.cancelRecording(event.recordingId)
    } catch (error) {
      this.state = failRecordingState(
        `${event.message} 부분 녹음 파일 정리에도 실패했습니다: ${errorMessage(error)}`,
      )
      this.render()
      return
    }
    this.state = failRecordingState(event.message)
    this.render()
  }

  dispose(): void {
    this.clearTimer()
    if (this.state.recordingId !== null) {
      void this.backend
        .cancelRecording(this.state.recordingId)
        .catch((error: unknown) => this.view.showError(errorMessage(error)))
    }
  }

  private tick(): void {
    const next = tickRecording(this.state, this.now())
    if (next === this.state) return
    // A tick only ever advances the clock, so the rest of the recorder — its
    // buttons, its path, its status line — is left exactly as it is.
    const clockOnly = next.status === this.state.status && next.message === this.state.message
    this.state = next
    if (clockOnly) {
      this.view.setRecordingTime(next.elapsedSeconds)
      return
    }
    this.render()
  }

  private clearTimer(): void {
    if (this.timer === null) return
    window.clearInterval(this.timer)
    this.timer = null
    document.removeEventListener("visibilitychange", this.refresh)
    window.removeEventListener("focus", this.refresh)
  }
}
