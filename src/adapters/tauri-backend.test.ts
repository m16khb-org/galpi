import { describe, expect, mock, test } from "bun:test"

import { toJobEvent } from "./tauri-backend"

describe("toJobEvent", () => {
  test("renames the worker's engine_version onto the domain field", () => {
    // Given: the host spells the field the way the worker protocol does
    const payload = { jobId: "job-1", type: "prepared", engine_version: "3.8.6" }

    // When
    const event = toJobEvent(payload)

    // Then
    expect(event).toEqual({ jobId: "job-1", type: "prepared", engineVersion: "3.8.6" })
  })

  test("passes a batched log through untouched", () => {
    // Given: the host groups worker output into one event
    const payload = { jobId: "job-1", type: "log" as const, stream: "stderr", message: "a\nb\nc" }

    // When / Then
    expect(toJobEvent(payload)).toEqual(payload)
  })

  test("turns an unrecognized payload into a visible log line", () => {
    // Given: a host newer than this window, emitting an event it does not know
    const payload = { jobId: "job-1", type: "teleported", destination: "mars" }

    // When
    const event = toJobEvent(payload)

    // Then: the job it belongs to still shows that something arrived
    expect(event.type).toBe("log")
    expect(event.jobId).toBe("job-1")
    if (event.type === "log") {
      expect(event.stream).toBe("frontend")
      expect(event.message).toContain("teleported")
    }
  })

  test("survives a payload that is not an object at all", () => {
    // Given / When
    const event = toJobEvent("nonsense")

    // Then: no throw, and the event is attributed to no job
    expect(event.type).toBe("log")
    expect(event.jobId).toBe("")
  })
})

describe("TauriBackend compute device", () => {
  test("parses the platform fields and invokes save_compute_device", async () => {
    // Given: a host reply carrying the Windows capability fields
    const calls: Array<[string, unknown]> = []
    mock.module("@tauri-apps/api/core", () => ({
      invoke: async (command: string, args?: unknown) => {
        calls.push([command, args])
        if (command !== "diagnose_environment") return undefined
        return {
          enginePreset: "whisperx",
          engineReady: true,
          modelsReady: true,
          ffmpegReady: true,
          qwen3Ready: false,
          whisperxReady: true,
          dataDirectory: "C:\\Galpi",
          defaultOutputDirectory: "C:\\Galpi\\out",
          engineVersion: "3.8.6",
          computeDevice: "cuda",
          availablePresets: ["whisperx"],
          availableDevices: ["cpu", "cuda"],
          cudaDriverDetected: true,
        }
      },
    }))
    const { TauriBackend } = await import("./tauri-backend")
    const backend = new TauriBackend()

    // When
    const status = await backend.diagnose()
    await backend.saveComputeDevice("cpu")

    // Then
    expect(status.computeDevice).toBe("cuda")
    expect(status.availablePresets).toEqual(["whisperx"])
    expect(status.availableDevices).toEqual(["cpu", "cuda"])
    expect(status.cudaDriverDetected).toBe(true)
    expect(calls).toContainEqual(["save_compute_device", { device: "cpu" }])
  })
})
