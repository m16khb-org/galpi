import { afterAll, beforeAll, describe, expect, test } from "bun:test"
import { Window } from "happy-dom"

import type { RecordingViewState } from "../application/recording-machine"
import type { BackendPort, RecordingResult, RecordingStatus } from "../domain/backend"
import type { AppView } from "./app-view"
import { RecordingController } from "./recording-controller"

// The controller drives `window.setInterval` and the visibility listener
// directly, so the timer path only runs with a DOM installed globally.
const previous = {
  window: Reflect.get(globalThis, "window") as unknown,
  document: Reflect.get(globalThis, "document") as unknown,
}
let dom: Window

beforeAll(() => {
  dom = new Window()
  Object.assign(globalThis, { window: dom, document: dom.document })
})

afterAll(() => {
  Object.assign(globalThis, previous)
  void dom.happyDOM.close()
})

const started: RecordingStatus = {
  recordingId: "recording-1",
  path: "/tmp/meeting.wav.part",
  sampleRate: 48_000,
  channels: 1,
}

function harness(
  result: RecordingResult,
  clock: () => number,
  stopped: () => Promise<RecordingResult> = async () => result,
) {
  const states: RecordingViewState[] = []
  // A tick that only advances the clock takes the narrow path, so the elapsed
  // time reaches the view through either call.
  const times: number[] = []
  const events: string[] = []
  const backend = {
    startRecording: async () => started,
    stopRecording: stopped,
    cancelRecording: async () => undefined,
  } as unknown as BackendPort
  const view = {
    setRecording: (state: RecordingViewState) => states.push(state),
    setRecordingTime: (elapsedSeconds: number) => times.push(elapsedSeconds),
    showError: () => undefined,
  } as unknown as AppView
  return {
    states,
    times,
    events,
    controller: new RecordingController(
      backend,
      view,
      (path) => events.push(`select:${path}`),
      (recordingId) => events.push(`recorded:${recordingId}`),
      clock,
    ),
  }
}

describe("RecordingController elapsed time", () => {
  test("catches up on foreground return after background ticks are dropped", async () => {
    // Given a recording started while the window was visible
    let now = 10_000
    const { controller, times } = harness(
      { ...started, path: "/tmp/meeting.wav", frames: 0, droppedFrames: 0, durationSeconds: 0 },
      () => now,
    )
    await controller.start("/tmp")

    // When the app spends 90 seconds in the background and comes back
    now = 100_000
    dom.document.dispatchEvent(new dom.Event("visibilitychange"))

    // Then the counter reflects the real recording time, not one lost tick
    expect(times.at(-1)).toBe(90)
  })

  test("settles the finished time on the recorded file duration", async () => {
    let now = 0
    const { controller, states } = harness(
      {
        ...started,
        path: "/tmp/meeting.wav",
        frames: 4_320_000,
        droppedFrames: 0,
        durationSeconds: 90.2,
      },
      () => now,
    )
    await controller.start("/tmp")
    now = 3_000

    await controller.stop()

    expect(states.at(-1)).toMatchObject({ status: "completed", elapsedSeconds: 90 })
  })
})

describe("RecordingController start with a source", () => {
  test("records with the chosen source only after that choice has been saved", async () => {
    // Given: the user picked the combined source and the host is still saving it
    const calls: string[] = []
    let finishSave!: () => void
    const saving = new Promise<void>((resolve) => {
      finishSave = resolve
    })
    const backend = {
      saveRecordingSource: async (source: string) => {
        calls.push(`save:${source}`)
        await saving
        calls.push("saved")
      },
      startRecording: async (_outputRoot: string, source: string) => {
        calls.push(`start:${source}`)
        return started
      },
      cancelRecording: async () => undefined,
    } as unknown as BackendPort
    const view = {
      setRecording: () => undefined,
      setRecordingSource: () => undefined,
      setRecordingTime: () => undefined,
      showError: () => undefined,
    } as unknown as AppView
    const controller = new RecordingController(backend, view, () => undefined)
    const choosing = controller.chooseSource("microphoneAndSystem")

    // When: recording starts before the save finishes
    const starting = controller.start("/tmp")
    await Promise.resolve()

    // Then: it waits, and starts with the combined source once the save is done
    expect(calls).toEqual(["save:microphoneAndSystem"])
    finishSave()
    await choosing
    await starting
    expect(calls).toEqual(["save:microphoneAndSystem", "saved", "start:microphoneAndSystem"])
    await controller.cancel()
  })
})

describe("RecordingController after a recording", () => {
  const finished: RecordingResult = {
    ...started,
    path: "/tmp/meeting.wav",
    frames: 48_000,
    droppedFrames: 0,
    durationSeconds: 1,
  }

  test("announces a cleanly stopped recording once, after its audio is selected", async () => {
    // Given
    const { controller, events } = harness(finished, () => 0)
    await controller.start("/tmp")

    // When
    await controller.stop()
    await controller.stop()

    // Then
    expect(events).toEqual(["select:/tmp/meeting.wav", "recorded:recording-1"])
  })

  test("announces nothing when stopping fails or the recording is discarded", async () => {
    // Given a recording whose file cannot be finished
    const failing = harness(
      finished,
      () => 0,
      async () => {
        throw new Error("disk full")
      },
    )
    await failing.controller.start("/tmp")

    // When
    await failing.controller.stop()

    // Then
    expect(failing.events).toEqual([])
    expect(failing.states.at(-1)?.status).toBe("failed")

    // Given another recording that is discarded
    const discarded = harness(finished, () => 0)
    await discarded.controller.start("/tmp")

    // When
    await discarded.controller.cancel()

    // Then
    expect(discarded.events).toEqual([])
  })
})
