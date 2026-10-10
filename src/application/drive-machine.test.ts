import { describe, expect, test } from "bun:test"

import type { DriveSyncResult } from "../domain/drive"
import { type DriveEvent, type DriveState, initialDrive, reduceDrive } from "./drive-machine"

const connected = { state: "connected", email: "dev@example.com" } as const
const disconnected = { state: "disconnected", onboardingDismissed: false } as const
const dismissed = { state: "disconnected", onboardingDismissed: true } as const

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
    expect(online.connection).toEqual({ status: "connected", email: "dev@example.com" })
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
    })
  })

  test("signing out forgets everything", () => {
    const busy = run([{ type: "importStarted" }, { type: "syncStarted", jobId: "job-1" }], online)

    expect(reduceDrive(busy, { type: "signedOut" })).toEqual(initialDrive)
  })
})
