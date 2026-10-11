import {
  type DriveEvent,
  type DriveState,
  initialDrive,
  reduceDrive,
} from "../application/drive-machine"
import { type BackendPort, errorCode, errorMessage } from "../domain/backend"
import type { DriveImport } from "../domain/drive"
import type { ImportedTranscript } from "../domain/job"
import type { AppView } from "./app-view"

/** What the Picker was asked for: a recording to transcribe or a transcript to augment. */
export type DriveImportKind = "recording" | "transcript"

/**
 * What the app does around a Drive import, which runs as a job: the job
 * panel, busy state, and audio or transcript selection belong to `AppController`.
 */
export interface DriveHooks {
  readonly onImportStarted: (jobId: string, kind: DriveImportKind) => void
  readonly onImported: (imported: DriveImport) => void
  readonly onTranscriptImported: (imported: ImportedTranscript) => void
  readonly onImportFailed: (error: unknown) => void
  readonly onImportFinished: () => void
  /** A Drive command answered `AUTH_REQUIRED`: the host closed the session. */
  readonly onSessionExpired: () => void
  readonly onError: (message: string) => void
}

/**
 * Drive connection, Picker import, and result sync as the window sees them.
 *
 * The host owns the tokens and the connection; this controller mirrors the
 * status and never retries a failed command.
 */
export class DriveController {
  private state: DriveState = initialDrive

  constructor(
    private readonly backend: BackendPort,
    private readonly view: AppView,
    private readonly hooks: DriveHooks,
  ) {}

  async load(): Promise<void> {
    try {
      this.dispatch({ type: "statusLoaded", status: await this.backend.loadDriveStatus() })
    } catch {
      // Drive is optional: an unreadable status leaves every Drive control hidden.
    }
  }

  /** Forget everything on sign-out; the next sign-in loads the status again. */
  reset(): void {
    this.dispatch({ type: "signedOut" })
  }

  async connect(): Promise<void> {
    if (this.dispatch({ type: "connectStarted" }).activity !== "connecting") return
    try {
      this.dispatch({ type: "connectSucceeded", status: await this.backend.connectDrive() })
    } catch (error) {
      if (errorCode(error) === "CANCELLED") {
        this.dispatch({ type: "connectCancelled" })
        return
      }
      this.dispatch({ type: "connectFailed", message: errorMessage(error) })
      if (errorCode(error) === "AUTH_REQUIRED") this.hooks.onSessionExpired()
    }
  }

  async cancel(): Promise<void> {
    if (this.state.activity !== "connecting") return
    try {
      await this.backend.cancelDriveOperation()
    } catch {
      // The consent keeps waiting; its own result still settles the card.
    }
  }

  async disconnect(): Promise<void> {
    if (this.state.connection.status !== "connected") return
    try {
      this.dispatch({ type: "disconnected", status: await this.backend.disconnectDrive() })
    } catch (error) {
      this.dispatch({ type: "failed", message: errorMessage(error) })
    }
  }

  async dismiss(): Promise<void> {
    try {
      this.dispatch({
        type: "onboardingDismissed",
        status: await this.backend.dismissDriveOnboarding(),
      })
    } catch (error) {
      this.dispatch({ type: "failed", message: errorMessage(error) })
    }
  }

  async importFromDrive(): Promise<void> {
    if (this.dispatch({ type: "importStarted" }).activity !== "importing") return
    const jobId = crypto.randomUUID()
    this.hooks.onImportStarted(jobId, "recording")
    try {
      const imported = await this.backend.importFromDrive(jobId)
      this.dispatch({ type: "imported" })
      this.hooks.onImported(imported)
    } catch (error) {
      this.dispatch({ type: "importFailed" })
      this.hooks.onImportFailed(error)
      this.noteLostConnection(error)
    } finally {
      this.hooks.onImportFinished()
    }
  }

  /**
   * Pick a transcript in Drive, download it as a job, and import it into
   * `outputRoot` as an augment target. Same job slot and cancel as `importFromDrive`.
   */
  async importTranscriptFromDrive(outputRoot: string): Promise<void> {
    if (this.dispatch({ type: "importStarted" }).activity !== "importing") return
    const jobId = crypto.randomUUID()
    this.hooks.onImportStarted(jobId, "transcript")
    try {
      const imported = await this.backend.importTranscriptFromDrive(jobId, outputRoot)
      this.dispatch({ type: "imported" })
      this.hooks.onTranscriptImported(imported)
    } catch (error) {
      this.dispatch({ type: "importFailed" })
      this.hooks.onImportFailed(error)
      this.noteLostConnection(error)
    } finally {
      this.hooks.onImportFinished()
    }
  }

  /** Upload a finished meeting's results; does nothing unless Drive is connected. */
  syncAfter(meetingJobId: string): void {
    if (this.state.connection.status !== "connected") return
    void this.sync(meetingJobId, false)
  }

  /** Replace the Drive copies that changed elsewhere, after the user asked to. */
  async overwrite(): Promise<void> {
    const sync = this.state.sync
    if (sync?.status !== "done") return
    await this.sync(sync.jobId, true)
  }

  async openFolder(): Promise<void> {
    const sync = this.state.sync
    if (sync?.status !== "done" || sync.result.folderLink === null) return
    try {
      await this.backend.openDriveLink(sync.result.folderLink)
    } catch (error) {
      this.hooks.onError(errorMessage(error))
    }
  }

  /** A new meeting starts: the previous meeting's sync no longer describes it. */
  clearSync(): void {
    this.dispatch({ type: "syncCleared" })
  }

  /** The settings switch: save the choice, then show what the host stored. */
  async setSaveRecordings(enabled: boolean): Promise<void> {
    if (this.state.connection.status !== "connected" || this.state.savingRecordings) return
    const settings = this.view.tokenSettings
    this.dispatch({ type: "saveRecordingsStarted" })
    settings.showMessage("변경사항을 자동 저장하는 중입니다.", "saving")
    try {
      const status = await this.backend.setDriveSaveRecordings(enabled)
      this.dispatch({ type: "saveRecordingsSaved", status })
      settings.showMessage("변경사항을 자동 저장했습니다.")
    } catch (error) {
      this.dispatch({ type: "saveRecordingsFailed" })
      settings.showMessage(`${errorMessage(error)} · 스위치는 저장된 값으로 돌아갑니다.`, "error")
      this.noteLostConnection(error)
    }
  }

  /** Upload a just-finished recording when the user asked for it; otherwise does nothing. */
  uploadRecordingAfter(recordingId: string): void {
    const { connection } = this.state
    if (connection.status !== "connected" || !connection.saveRecordings) return
    void this.uploadRecording(recordingId)
  }

  /** Try the failed upload again, after the user asked to. */
  async retryRecordingUpload(): Promise<void> {
    const recording = this.state.recording
    if (recording?.status !== "failed") return
    await this.uploadRecording(recording.recordingId)
  }

  async openRecordingFolder(): Promise<void> {
    const recording = this.state.recording
    if (recording?.status !== "done" || recording.result.folderLink === null) return
    try {
      await this.backend.openDriveLink(recording.result.folderLink)
    } catch (error) {
      this.hooks.onError(errorMessage(error))
    }
  }

  /** A new recording starts: the previous recording's upload no longer describes it. */
  clearRecordingUpload(): void {
    this.dispatch({ type: "recordingUploadCleared" })
  }

  private async uploadRecording(recordingId: string): Promise<void> {
    const started = this.dispatch({ type: "recordingUploadStarted", recordingId })
    if (started.recording?.recordingId !== recordingId) return
    try {
      const result = await this.backend.uploadRecordingToDrive(recordingId)
      this.dispatch({ type: "recordingUploadFinished", recordingId, result })
    } catch (error) {
      this.dispatch({ type: "recordingUploadFailed", recordingId, message: errorMessage(error) })
      this.noteLostConnection(error)
    }
  }

  private async sync(jobId: string, overwrite: boolean): Promise<void> {
    this.dispatch({ type: "syncStarted", jobId })
    try {
      const result = await this.backend.syncMeetingToDrive(jobId, overwrite)
      this.dispatch({ type: "syncFinished", jobId, result })
    } catch (error) {
      this.dispatch({ type: "syncFailed", jobId, message: errorMessage(error) })
      this.noteLostConnection(error)
    }
  }

  /**
   * Follow the host when a failure means the session or the connection is gone.
   * A user who just disconnected already sees the disconnected state.
   */
  private noteLostConnection(error: unknown): void {
    const code = errorCode(error)
    if (code === "AUTH_REQUIRED") {
      this.hooks.onSessionExpired()
    } else if (
      (code === "DRIVE_RECONNECT_REQUIRED" || code === "DRIVE_NOT_CONNECTED") &&
      this.state.connection.status === "connected"
    ) {
      this.dispatch({ type: "reconnectRequired", message: errorMessage(error) })
    }
  }

  private dispatch(event: DriveEvent): DriveState {
    this.state = reduceDrive(this.state, event)
    this.view.setDriveConnected(this.state.connection.status === "connected")
    this.view.drive.render(this.state)
    return this.state
  }
}
