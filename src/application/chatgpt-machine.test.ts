import { describe, expect, test } from "bun:test"

import {
  type ChatGptFlowState,
  type ChatGptFlowEvent,
  initialChatGptFlow,
  isChatGptFlowActive,
  reduceChatGptFlow,
} from "./chatgpt-machine"

function run(events: readonly ChatGptFlowEvent[]): ChatGptFlowState {
  return events.reduce(reduceChatGptFlow, initialChatGptFlow)
}

describe("reduceChatGptFlow", () => {
  test("walks idle → awaitingBrowser → exchanging → idle on success", () => {
    // Given / When
    const waiting = run([{ type: "started" }])
    const exchanging = reduceChatGptFlow(waiting, { type: "phase", phase: "exchanging" })
    const done = reduceChatGptFlow(exchanging, { type: "succeeded" })

    // Then
    expect(waiting.status).toBe("awaitingBrowser")
    expect(exchanging.status).toBe("exchanging")
    expect(done).toEqual(initialChatGptFlow)
  })

  test("keeps the failure code and message until the next attempt", () => {
    // Given: the host rejected the sign-in
    const failed = run([
      { type: "started" },
      { type: "failed", code: "CHATGPT_CONSENT_DENIED", message: "거부되었습니다." },
    ])

    // Then
    expect(failed).toEqual({
      status: "failed",
      code: "CHATGPT_CONSENT_DENIED",
      message: "거부되었습니다.",
    })
    expect(isChatGptFlowActive(failed)).toBe(false)

    // When: the user tries again
    const retry = reduceChatGptFlow(failed, { type: "started" })

    // Then: the previous failure is gone
    expect(retry).toEqual({ status: "awaitingBrowser", code: null, message: null })
  })

  test("returns to idle without a failure when the user cancels", () => {
    // Given / When
    const cancelled = run([{ type: "started" }, { type: "cancelled" }])

    // Then
    expect(cancelled).toEqual(initialChatGptFlow)
  })

  test("ignores a second start while a sign-in is already running", () => {
    // Given
    const exchanging = run([{ type: "started" }, { type: "phase", phase: "exchanging" }])

    // When
    const next = reduceChatGptFlow(exchanging, { type: "started" })

    // Then: the running attempt keeps its phase
    expect(next).toBe(exchanging)
  })

  test("ignores a late phase event once the sign-in has ended", () => {
    // Given: the event arrives after the command already settled
    const idle = run([{ type: "started" }, { type: "succeeded" }])
    const failed = run([{ type: "started" }, { type: "failed", code: "X", message: "m" }])

    // When / Then
    expect(reduceChatGptFlow(idle, { type: "phase", phase: "exchanging" })).toBe(idle)
    expect(reduceChatGptFlow(failed, { type: "phase", phase: "exchanging" })).toBe(failed)
  })

  test("reports active only while waiting for the browser or exchanging", () => {
    expect(isChatGptFlowActive(initialChatGptFlow)).toBe(false)
    expect(isChatGptFlowActive(run([{ type: "started" }]))).toBe(true)
    expect(
      isChatGptFlowActive(run([{ type: "started" }, { type: "phase", phase: "exchanging" }])),
    ).toBe(true)
  })
})
