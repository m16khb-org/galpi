import { describe, expect, test } from "bun:test"

import type { DriveRecordingUpload, DriveSyncResult } from "../domain/drive"
import {
  clientLocked,
  type DriveEvent,
  type DriveState,
  initialDrive,
  reduceDrive,
} from "./drive-machine"

const connected = { state: "connected", email: "dev@example.com", saveRecordings: false } as const
const disconnected = { state: "disconnected", onboardingDismissed: false } as const
const dismissed = { state: "disconnected", onboardingDismissed: true } as const

const uploaded: DriveRecordingUpload = {
  state: "uploaded",
  folderLink: "https://drive.google.com/drive/folders/m1",
  webViewLink: null,
}

const result: DriveSyncResult = {
  folderLink: "https://drive.google.com/drive/folders/m1",
  files: [{ kind: "speaker_text", state: "uploaded", webViewLink: null }],
}

function run(events: readonly DriveEvent[], from: DriveState = initialDrive): DriveState {
  return events.reduce(reduceDrive, from)
}

const offline = run([{ type: "statusLoaded", status: disconnected }])
const online = run([{ type: "statusLoaded", status: connected }])

describe("reduceDrive", () => {
  test("the loaded status decides between the onboarding card and the connection", () => {
    expect(initialDrive.connection).toEqual({ status: "unknown" })
    expect(offline.connection).toEqual({ status: "disconnected", onboardingDismissed: false })
    expect(online.connection).toEqual({
      status: "connected",
      email: "dev@example.com",
      saveRecordings: false,
    })
  })

  test("connecting succeeds, fails with a reason, or is cancelled back to idle", () => {
    // Given
    const connecting = reduceDrive(offline, { type: "connectStarted" })

    // Then
    expect(connecting.activity).toBe("connecting")
    expect(reduceDrive(connecting, { type: "connectSucceeded", status: connected })).toEqual({
      ...online,
      activity: "idle",
    })
    expect(reduceDrive(connecting, { type: "connectFailed", message: "실패" })).toEqual({
      ...offline,
      message: "실패",
    })
    expect(reduceDrive(connecting, { type: "connectCancelled" })).toEqual(offline)
  })

  test("only a disconnected, idle Drive can start connecting", () => {
    const connecting = reduceDrive(offline, { type: "connectStarted" })

    expect(reduceDrive(online, { type: "connectStarted" })).toBe(online)
    expect(reduceDrive(initialDrive, { type: "connectStarted" })).toBe(initialDrive)
    expect(reduceDrive(connecting, { type: "connectStarted" })).toBe(connecting)
  })

  test("connect outcomes with no attempt waiting are stale", () => {
    expect(reduceDrive(offline, { type: "connectSucceeded", status: connected })).toBe(offline)
    expect(reduceDrive(offline, { type: "connectFailed", message: "늦음" })).toBe(offline)
    expect(reduceDrive(offline, { type: "connectCancelled" })).toBe(offline)
  })

  test("dismissing hides the card, disconnecting forgets the sync, failures are kept", () => {
    const synced = run(
      [
        { type: "syncStarted", jobId: "job-1" },
        { type: "syncFinished", jobId: "job-1", result },
      ],
      online,
    )

    expect(reduceDrive(offline, { type: "onboardingDismissed", status: dismissed }).connection).toEqual(
      { status: "disconnected", onboardingDismissed: true },
    )
    expect(reduceDrive(synced, { type: "disconnected", status: dismissed })).toEqual({
      ...initialDrive,
      connection: { status: "disconnected", onboardingDismissed: true },
    })
    expect(reduceDrive(online, { type: "failed", message: "지우지 못함" }).message).toBe(
      "지우지 못함",
    )
  })

  test("an import runs only while connected and idle, and either outcome ends it", () => {
    const importing = reduceDrive(online, { type: "importStarted" })

    expect(importing.activity).toBe("importing")
    expect(reduceDrive(importing, { type: "imported" }).activity).toBe("idle")
    expect(reduceDrive(importing, { type: "importFailed" }).activity).toBe("idle")
    expect(reduceDrive(offline, { type: "importStarted" })).toBe(offline)
    expect(reduceDrive(importing, { type: "importStarted" })).toBe(importing)
  })

  test("a sync reports the latest meeting and drops answers for any other", () => {
    // Given
    const syncing = reduceDrive(online, { type: "syncStarted", jobId: "job-2" })

    // Then
    expect(syncing.sync).toEqual({ status: "syncing", jobId: "job-2" })
    expect(reduceDrive(syncing, { type: "syncFinished", jobId: "job-2", result }).sync).toEqual({
      status: "done",
      jobId: "job-2",
      result,
    })
    expect(reduceDrive(syncing, { type: "syncFailed", jobId: "job-2", message: "끊김" }).sync).toEqual(
      { status: "failed", jobId: "job-2", message: "끊김" },
    )
    expect(reduceDrive(syncing, { type: "syncFinished", jobId: "job-1", result })).toBe(syncing)
    expect(reduceDrive(syncing, { type: "syncCleared" }).sync).toBeNull()
    expect(reduceDrive(offline, { type: "syncStarted", jobId: "job-2" })).toBe(offline)
  })

  test("a lost connection brings the card back and fails the pending sync", () => {
    // Given: a dismissed card and a sync in flight
    const syncing = run(
      [
        { type: "statusLoaded", status: connected },
        { type: "syncStarted", jobId: "job-1" },
      ],
      run([{ type: "statusLoaded", status: dismissed }]),
    )

    // When
    const lost = reduceDrive(syncing, { type: "reconnectRequired", message: "다시 연결해 주세요." })

    // Then
    expect(lost).toEqual({
      connection: { status: "disconnected", onboardingDismissed: false },
      activity: "idle",
      message: "다시 연결해 주세요.",
      sync: { status: "failed", jobId: "job-1", message: "다시 연결해 주세요." },
      savingRecordings: false,
      recording: null,
      client: { status: "unknown" },
      clientNotice: null,
    })
  })

  test("the recordings choice saves only while connected, one write at a time", () => {
    // Given
    const saving = reduceDrive(online, { type: "saveRecordingsStarted" })
    const saved = reduceDrive(saving, {
      type: "saveRecordingsSaved",
      status: { ...connected, saveRecordings: true },
    })

    // Then
    expect(saving.savingRecordings).toBe(true)
    expect(reduceDrive(saving, { type: "saveRecordingsStarted" })).toBe(saving)
    expect(reduceDrive(offline, { type: "saveRecordingsStarted" })).toBe(offline)
    expect(saved.savingRecordings).toBe(false)
    expect(saved.connection).toEqual({
      status: "connected",
      email: "dev@example.com",
      saveRecordings: true,
    })
    expect(reduceDrive(saving, { type: "saveRecordingsFailed" })).toEqual(online)
    expect(reduceDrive(online, { type: "saveRecordingsSaved", status: connected })).toBe(online)
    expect(reduceDrive(online, { type: "saveRecordingsFailed" })).toBe(online)
  })

  test("a recording upload reports the latest recording and drops answers for any other", () => {
    // Given
    const uploading = reduceDrive(online, { type: "recordingUploadStarted", recordingId: "rec-2" })

    // Then
    expect(uploading.recording).toEqual({ status: "uploading", recordingId: "rec-2" })
    expect(
      reduceDrive(uploading, {
        type: "recordingUploadFinished",
        recordingId: "rec-2",
        result: uploaded,
      }).recording,
    ).toEqual({ status: "done", recordingId: "rec-2", result: uploaded })
    expect(
      reduceDrive(uploading, {
        type: "recordingUploadFailed",
        recordingId: "rec-2",
        message: "끊김",
      }).recording,
    ).toEqual({ status: "failed", recordingId: "rec-2", message: "끊김" })
    expect(
      reduceDrive(uploading, {
        type: "recordingUploadFinished",
        recordingId: "rec-1",
        result: uploaded,
      }),
    ).toBe(uploading)
    expect(reduceDrive(uploading, { type: "recordingUploadCleared" }).recording).toBeNull()
    expect(reduceDrive(offline, { type: "recordingUploadStarted", recordingId: "rec-2" })).toBe(
      offline,
    )
  })

  test("a failed recording upload can start again, a finished one cannot be failed later", () => {
    // Given
    const failed = run(
      [
        { type: "recordingUploadStarted", recordingId: "rec-1" },
        { type: "recordingUploadFailed", recordingId: "rec-1", message: "끊김" },
      ],
      online,
    )
    const retrying = reduceDrive(failed, { type: "recordingUploadStarted", recordingId: "rec-1" })
    const done = reduceDrive(retrying, {
      type: "recordingUploadFinished",
      recordingId: "rec-1",
      result: uploaded,
    })

    // Then
    expect(retrying.recording).toEqual({ status: "uploading", recordingId: "rec-1" })
    expect(
      reduceDrive(done, { type: "recordingUploadFailed", recordingId: "rec-1", message: "늦음" }),
    ).toBe(done)
  })

  test("disconnecting or losing the connection ends the recording upload", () => {
    // Given
    const uploading = reduceDrive(online, { type: "recordingUploadStarted", recordingId: "rec-1" })

    // When / Then
    expect(reduceDrive(uploading, { type: "disconnected", status: dismissed }).recording).toBeNull()
    expect(
      reduceDrive(uploading, { type: "reconnectRequired", message: "다시 연결해 주세요." })
        .recording,
    ).toEqual({ status: "failed", recordingId: "rec-1", message: "다시 연결해 주세요." })
  })
})

describe("Drive client machine", () => {
  const noClient = { configured: false, clientId: null } as const
  const savedClient = { configured: true, clientId: "123-abc.apps.googleusercontent.com" } as const
  const ready = run([{ type: "clientLoaded", status: noClient }], offline)

  test("Drive client is locked while connected or waiting for consent, and only then", () => {
    // Given / When / Then
    expect(clientLocked(ready)).toBe(false)
    expect(clientLocked(run([{ type: "connectStarted" }], ready))).toBe(true)
    expect(clientLocked(run([{ type: "connectStarted" }, { type: "connectCancelled" }], ready))).toBe(
      false,
    )
    expect(clientLocked(run([{ type: "statusLoaded", status: connected }], ready))).toBe(true)
  })

  test("Drive client write starts only when the client is known and unlocked", () => {
    // Given
    const unknown = run([{ type: "clientSaveStarted" }], offline)
    const locked = run([{ type: "clientSaveStarted" }], run([{ type: "statusLoaded", status: connected }], ready))

    // When
    const started = run([{ type: "clientSaveStarted" }], ready)

    // Then
    expect(unknown.clientNotice).toBeNull()
    expect(locked.clientNotice).toBeNull()
    expect(started.clientNotice).toEqual({ kind: "saving" })
  })

  test("Drive client save stores the loaded id and reports 저장됨; a removal reports nothing", () => {
    // Given
    const saving = run([{ type: "clientSaveStarted" }], ready)

    // When
    const saved = reduceDrive(saving, { type: "clientSaved", status: savedClient })
    const removed = run(
      [{ type: "clientSaveStarted" }, { type: "clientSaved", status: noClient }],
      saved,
    )

    // Then
    expect(saved.client).toEqual({ status: "saved", clientId: savedClient.clientId })
    expect(saved.clientNotice).toEqual({ kind: "saved" })
    expect(removed.client).toEqual({ status: "none" })
    expect(removed.clientNotice).toBeNull()
  })

  test("Drive client loaded as saved or missing, and unknown until then", () => {
    // Given / When / Then
    expect(offline.client).toEqual({ status: "unknown" })
    expect(ready.client).toEqual({ status: "none" })
    expect(run([{ type: "clientLoaded", status: savedClient }], offline).client).toEqual({
      status: "saved",
      clientId: savedClient.clientId,
    })
  })

  test("Drive client is missing for a legacy connection that the host dropped", () => {
    // Given / When: the host disconnected it on startup, so the window sees both answers
    const legacy = run(
      [
        { type: "statusLoaded", status: dismissed },
        { type: "clientLoaded", status: noClient },
      ],
      initialDrive,
    )

    // Then: disconnected with no client, and the client is free to change
    expect(legacy.connection).toEqual({ status: "disconnected", onboardingDismissed: true })
    expect(legacy.client).toEqual({ status: "none" })
    expect(clientLocked(legacy)).toBe(false)
  })

  test("Drive client failure keeps the client and names the field; an answer nobody awaits is ignored", () => {
    // Given
    const saving = run([{ type: "clientSaveStarted" }], ready)
    const failure = {
      type: "clientFailed",
      field: "clientId",
      message: "클라이언트 ID는 숫자-문자.apps.googleusercontent.com 형태여야 합니다.",
    } as const

    // When
    const failed = reduceDrive(saving, failure)

    // Then
    expect(failed.client).toEqual({ status: "none" })
    expect(failed.clientNotice).toEqual({
      kind: "failed",
      field: "clientId",
      message: failure.message,
    })
    expect(reduceDrive(ready, failure)).toBe(ready)
    expect(reduceDrive(ready, { type: "clientSaved", status: savedClient })).toBe(ready)
  })
})
