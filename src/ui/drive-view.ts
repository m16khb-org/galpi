import type { DriveState } from "../application/drive-machine"
import {
  type DriveTone,
  RECORDING_UPLOADING,
  recordingUploadFailure,
  recordingUploadSummary,
  syncFileLine,
  syncSummary,
} from "../domain/drive"
import { required } from "./dom"

const CONNECTING =
  "브라우저에서 Google Drive 권한을 허용해 주세요. 창이 보이지 않으면 취소 후 다시 시도하세요."

/** The settings button is never hidden or disabled: the last resort outside the sheet. */
const SETTINGS = '[data-action="open-settings"]'

/**
 * When a Drive control hides itself (connect turns into cancel, the card goes
 * away), focus moves to the first visible, enabled control that took its place.
 * Start actions are disabled while a job or a recording runs, hence the fallback.
 */
const FOCUS_HANDOFF: Readonly<Record<string, readonly string[]>> = {
  "drive-connect-button": ["#drive-onboarding-cancel-button", "#drive-import-button", SETTINGS],
  "drive-onboarding-cancel-button": ["#drive-connect-button", "#drive-import-button", SETTINGS],
  "drive-dismiss-button": ["#audio-selection", SETTINGS],
  "drive-settings-connect-button": ["#drive-settings-cancel-button", "#drive-disconnect-button"],
  "drive-settings-cancel-button": ["#drive-settings-connect-button", "#drive-disconnect-button"],
  "drive-disconnect-button": ["#drive-settings-connect-button"],
  "recording-drive-retry-button": ["#recording-drive-folder-button", "#record-button", SETTINGS],
}

/**
 * Every Drive surface: the onboarding card, the top-bar chip, the settings
 * section, the sync row under the results, and the upload line under the
 * recorder. Status always pairs its color with text. The import button lives
 * with the other start actions in `AppView`, which knows when a job or a
 * recording is running.
 */
export class DriveView {
  private readonly root: HTMLElement
  private toggleHadFocus = false

  constructor(root: HTMLElement) {
    this.root = root
  }

  /** Call after the import button's visibility is current; focus may move to it. */
  render(state: DriveState): void {
    const focused = this.root.ownerDocument.activeElement
    const { connection, activity } = state
    const connecting = activity === "connecting"
    const connected = connection.status === "connected"
    const disconnected = connection.status === "disconnected"

    this.element("#drive-onboarding").hidden = !(disconnected && !connection.onboardingDismissed)
    const [line, tone] = connectionLine(state)
    this.status("#drive-settings-account", line, tone)
    // The card already explains itself; its status line only reports progress or trouble.
    const eventful = connecting || state.message !== null
    this.status("#drive-onboarding-status", eventful ? line : "", tone)
    this.element("#drive-connect-button").hidden = connecting
    this.element("#drive-dismiss-button").hidden = connecting
    this.element("#drive-onboarding-cancel-button").hidden = !connecting

    const chip = this.element("#drive-chip")
    chip.hidden = !connected
    if (connected) this.element("#drive-email").textContent = connection.email
    // The chip truncates a long address; the full one stays a hover away.
    chip.title = connected ? connection.email : ""

    this.element("#drive-settings-state").textContent = connected
      ? "연결됨"
      : disconnected
        ? "연결 안 됨"
        : "확인 중"
    this.element("#drive-settings-connect-button").hidden = !disconnected || connecting
    this.element("#drive-settings-cancel-button").hidden = !connecting
    this.element("#drive-disconnect-button").hidden = !connected

    this.renderSync(state)
    this.renderRecording(state)
    this.renderSaveRecordings(state)
    this.handOffFocus(focused)
  }

  private handOffFocus(focused: Element | null): void {
    if (focused === null || focused.closest("[hidden]") === null) return
    const next = (FOCUS_HANDOFF[focused.id] ?? [])
      .map((selector) => this.root.querySelector<HTMLElement>(selector))
      .find(
        (candidate) =>
          candidate !== null &&
          candidate.closest("[hidden]") === null &&
          !candidate.matches(":disabled"),
      )
    next?.focus()
  }

  private renderSync({ sync }: DriveState): void {
    this.element("#drive-sync-row").hidden = sync === null
    const files = this.element("#drive-sync-files")
    const folder = this.element<HTMLButtonElement>("#drive-open-folder-button")
    const overwrite = this.element<HTMLButtonElement>("#drive-overwrite-button")
    if (sync === null) {
      // The next meeting's sync starts from an empty row, not the last one's files.
      files.replaceChildren()
      folder.hidden = true
      overwrite.hidden = true
      return
    }
    if (sync.status === "syncing") {
      this.status("#drive-sync-status", "Drive에 결과를 올리는 중입니다.", "pending")
      overwrite.disabled = true
      return
    }
    if (sync.status === "failed") {
      this.status("#drive-sync-status", sync.message, "error")
      files.replaceChildren()
      folder.hidden = true
      overwrite.hidden = true
      return
    }
    const summary = syncSummary(sync.result)
    this.status("#drive-sync-status", summary.text, summary.tone)
    files.replaceChildren(
      ...sync.result.files.map((file) => {
        const item = this.root.ownerDocument.createElement("li")
        item.textContent = syncFileLine(file)
        item.dataset["state"] = file.state
        return item
      }),
    )
    folder.hidden = sync.result.folderLink === null
    overwrite.hidden = !summary.conflict
    overwrite.disabled = false
  }

  private renderRecording({ recording, connection }: DriveState): void {
    const folder = this.element<HTMLButtonElement>("#recording-drive-folder-button")
    const retry = this.element<HTMLButtonElement>("#recording-drive-retry-button")
    folder.hidden = true
    retry.hidden = true
    if (recording === null) {
      this.status("#recording-drive-status", "", "pending")
    } else if (recording.status === "uploading") {
      this.status("#recording-drive-status", RECORDING_UPLOADING, "pending")
    } else if (recording.status === "failed") {
      this.status("#recording-drive-status", recordingUploadFailure(recording.message), "error")
      // After a lost connection there is nothing to retry against.
      retry.hidden = connection.status !== "connected"
    } else {
      const summary = recordingUploadSummary(recording.result)
      this.status("#recording-drive-status", summary.text, summary.tone)
      folder.hidden = recording.result.folderLink === null
    }
  }

  private renderSaveRecordings({ connection, savingRecordings }: DriveState): void {
    const toggle = this.element<HTMLInputElement>("#drive-save-recordings")
    const document = this.root.ownerDocument
    if (savingRecordings && document.activeElement === toggle) this.toggleHadFocus = true
    // While the write is in flight the switch shows what the user chose; the
    // stored value takes over when it answers, so a refusal puts it back.
    if (!savingRecordings) {
      toggle.checked = connection.status === "connected" && connection.saveRecordings
    }
    toggle.disabled = connection.status !== "connected" || savingRecordings
    // A switch that disables itself while saving gives focus up; take it back afterwards.
    if (!toggle.disabled && this.toggleHadFocus) {
      this.toggleHadFocus = false
      if (document.activeElement === null || document.activeElement === document.body) {
        toggle.focus()
      }
    }
  }

  private status(selector: string, text: string, tone: DriveTone): void {
    const element = this.element(selector)
    element.textContent = text
    element.dataset["state"] = tone
  }

  private element<T extends HTMLElement = HTMLElement>(selector: string): T {
    return required<T>(this.root, selector)
  }
}

function connectionLine({ connection, activity, message }: DriveState): readonly [string, DriveTone] {
  if (activity === "connecting") return [CONNECTING, "pending"]
  if (message !== null) return [message, "error"]
  if (connection.status === "connected") {
    return [`${connection.email} 계정의 Drive에 연결했습니다.`, "ready"]
  }
  return connection.status === "disconnected"
    ? ["Drive에 연결하지 않았습니다.", "pending"]
    : ["Drive 연결 상태를 확인하는 중입니다.", "pending"]
}
