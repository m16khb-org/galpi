import { describe, expect, test } from "bun:test"
import type { RecordingViewState } from "../application/recording-machine"
import type { BackendPort, RecordingStatus } from "../domain/backend"
import type { RecordingSource } from "../domain/recording"
import type { AppView } from "./app-view"
import { RecordingController } from "./recording-controller"

describe("RecordingController", () => {
  test("cleans up a native failure emitted before start resolves", async () => {
    let resolveStart!: (status: RecordingStatus) => void
    const startReply = new Promise<RecordingStatus>((resolve) => {
      resolveStart = resolve
    })
    const cancelled: string[] = []
    const states: RecordingViewState[] = []
    const backend = {
      startRecording: () => startReply,
      cancelRecording: async (recordingId: string) => {
        cancelled.push(recordingId)
      },
    } as unknown as BackendPort
    const view = {
      setRecording: (state: RecordingViewState) => states.push(state),
      showError: () => undefined,
    } as unknown as AppView
    const controller = new RecordingController(backend, view, () => undefined)

    const start = controller.start("/tmp")
    await controller.handleFailure({
      recordingId: "recording-1",
      code: "AUDIO_OVERRUN",
      message: "writer overrun",
    })
    resolveStart({
      recordingId: "recording-1",
      path: "/tmp/meeting.wav.part",
      sampleRate: 48_000,
      channels: 1,
    })
    await start

    expect(cancelled).toEqual(["recording-1"])
    expect(states.at(-1)?.status).toBe("failed")
    expect(states.at(-1)?.message).toBe("writer overrun")
  })
})

describe("RecordingController source choice", () => {
  function sourceHarness(
    saveRecordingSource: (source: RecordingSource) => Promise<void>,
    loadRecordingSource: () => Promise<RecordingSource> = async () => "microphoneAndSystem",
  ) {
    const shown: RecordingSource[] = []
    const errors: string[] = []
    const backend = { loadRecordingSource, saveRecordingSource } as unknown as BackendPort
    const view = {
      setRecordingSource: (source: RecordingSource) => shown.push(source),
      showError: (message: string) => errors.push(message),
    } as unknown as AppView
    return { shown, errors, controller: new RecordingController(backend, view, () => undefined) }
  }

  test("shows the stored source when the workspace loads", async () => {
    // Given
    const { controller, shown } = sourceHarness(async () => undefined)

    // When
    await controller.loadSource()

    // Then
    expect(shown).toEqual(["microphoneAndSystem"])
  })

  test("keeps the default and reports it when the stored source cannot be read", async () => {
    // Given
    const { controller, shown, errors } = sourceHarness(
      async () => undefined,
      async () => {
        throw { code: "IO_ERROR", message: "settings unreadable" }
      },
    )

    // When
    await controller.loadSource()

    // Then
    expect(shown).toEqual(["microphone"])
    expect(errors).toEqual(["settings unreadable"])
  })

  test("puts the previous source back and reports it when saving the choice fails", async () => {
    // Given: the first save succeeds and the second is rejected by the host
    let saves = 0
    const { shown, errors, controller } = sourceHarness(async () => {
      saves += 1
      if (saves === 2) throw { code: "IO_ERROR", message: "settings not writable" }
    })
    await controller.chooseSource("microphoneAndSystem")

    // When
    await controller.chooseSource("microphone")

    // Then: the control shows each choice at once, then falls back to the saved one
    expect(shown).toEqual(["microphoneAndSystem", "microphone", "microphoneAndSystem"])
    expect(errors).toEqual(["settings not writable"])
  })
})
