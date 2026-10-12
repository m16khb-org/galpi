import { clientLocked, type DriveState } from "../application/drive-machine"
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

const CLIENT_SETUP = "Google 클라이언트 설정"
const CLIENT_NEEDED = "Drive를 연결하려면 먼저 Google 클라이언트를 저장해야 합니다."

/** The settings button is never hidden or disabled: the last resort outside the sheet. */
const SETTINGS = '[data-action="open-settings"]'

/**
 * When a Drive control hides itself (connect turns into cancel, the card goes
 * away), focus moves to the first visible, enabled control that took its place.
 * Start actions are disabled while a job or a recording runs, hence the fallback.
 */
const FOCUS_HANDOFF: Readonly<Record<string, readonly string[]>> = {
  "drive-client-remove-button": ["#drive-client-id"],
  "drive-client-save-button": ["#drive-client-remove-button"],
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
    const { connection, activity, client } = state
    const connecting = activity === "connecting"
    const connected = connection.status === "connected"
    const disconnected = connection.status === "disconnected"
    // With no client Drive cannot connect: the button leads to the setup instead.
    const needsClient = client.status === "none"
    // Without the client state nothing is offered, as without the connection state.
    const offered = disconnected && client.status !== "unknown"

    this.element("#drive-onboarding").hidden = !(
      disconnected &&
      !connection.onboardingDismissed &&
      client.status !== "unknown"
    )
    const [line, tone] = connectionLine(state)
    this.status("#drive-settings-account", line, tone)
    // The card already explains itself; its status line reports progress or trouble,
    // and the missing client that stands between the user and the connection.
    const eventful = connecting || state.message !== null || needsClient
    this.status("#drive-onboarding-status", eventful ? line : "", tone)
    this.element("#drive-connect-label").textContent = needsClient ? CLIENT_SETUP : "Drive 연결"
    this.element("#drive-connect-button").hidden = connecting
    this.element("#drive-dismiss-button").hidden = connecting
    this.element("#drive-onboarding-cancel-button").hidden = !connecting

    const chip = this.element("#drive-chip")
    chip.hidden = !connected
    if (connected) this.element("#drive-account").textContent = connection.email
    // The chip truncates a long address; the full one stays a hover away.
    chip.title = connected ? connection.email : ""

    this.element("#drive-settings-state").textContent = connected
      ? "연결됨"
      : disconnected
        ? "연결 안 됨"
        : "확인 중"
    // The sheet shows the client setup right below, so this button waits for a client.
    this.element("#drive-settings-connect-button").hidden = !offered || needsClient || connecting
    this.element("#drive-settings-cancel-button").hidden = !connecting
    this.element("#drive-disconnect-button").hidden = !connected

    this.renderSync(state)
    this.renderRecording(state)
    this.renderClient(state)
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

  /** What the user typed, untrimmed; the host judges the format. */
  clientForm(): { readonly clientId: string; readonly clientSecret: string } {
    return {
      clientId: this.element<HTMLInputElement>("#drive-client-id").value,
      clientSecret: this.element<HTMLInputElement>("#drive-client-secret").value,
    }
  }

  private renderClient(state: DriveState): void {
    const { client, clientNotice } = state
    const saved = client.status === "saved"
    const locked = clientLocked(state)
    const idInput = this.element<HTMLInputElement>("#drive-client-id")
    const secretInput = this.element<HTMLInputElement>("#drive-client-secret")
    const save = this.element<HTMLButtonElement>("#drive-client-save-button")
    const remove = this.element<HTMLButtonElement>("#drive-client-remove-button")

    // Without the connection state the lock is unknown, so nothing is offered.
    this.element("#drive-client").hidden =
      client.status === "unknown" || state.connection.status === "unknown"
    // A saved client shows no fields: the host keeps the secret, and changing it means removing it.
    this.element("#drive-client-form").hidden = saved
    remove.hidden = !saved
    if (saved) {
      idInput.value = ""
      secretInput.value = ""
    }
    for (const control of [idInput, secretInput, save, remove]) control.disabled = locked
    this.element("#drive-client-lock").hidden = !locked

    const status = this.element("#drive-client-status")
    if (clientNotice?.kind === "saving") {
      this.clientStatus(status, "저장하는 중입니다.", "pending")
    } else if (clientNotice?.kind === "saved") {
      this.clientStatus(status, "저장됨", "ready")
    } else if (client.status === "saved") {
      this.clientStatus(status, `Google 클라이언트를 쓰는 중입니다 · ${client.clientId}`, "ready")
    } else {
      this.clientStatus(status, "Google 클라이언트가 아직 없습니다.", "default")
    }

    const failed = clientNotice?.kind === "failed" ? clientNotice : null
    this.element("#drive-client-error").textContent = failed?.message ?? ""
    for (const [field, input] of [
      ["clientId", idInput],
      ["clientSecret", secretInput],
    ] as const) {
      const invalid = failed?.field === field
      if (invalid) input.setAttribute("aria-invalid", "true")
      else input.removeAttribute("aria-invalid")
      input.setAttribute(
        "aria-describedby",
        invalid ? "drive-client-help drive-client-error" : "drive-client-help",
      )
    }
  }

  private clientStatus(element: HTMLElement, text: string, tone: DriveTone | "default"): void {
    element.textContent = text
    element.dataset["state"] = tone
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

function connectionLine({
  connection,
  activity,
  message,
  client,
}: DriveState): readonly [string, DriveTone] {
  if (activity === "connecting") return [CONNECTING, "pending"]
  if (message !== null) return [message, "error"]
  if (connection.status === "connected") {
    return [`${connection.email} 계정의 Drive에 연결했습니다.`, "ready"]
  }
  if (connection.status === "disconnected" && client.status === "none") {
    return [CLIENT_NEEDED, "pending"]
  }
  return connection.status === "disconnected"
    ? ["Drive에 연결하지 않았습니다.", "pending"]
    : ["Drive 연결 상태를 확인하는 중입니다.", "pending"]
}
