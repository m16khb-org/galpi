import { describe, expect, test } from "bun:test"

import {
  type AccessEvent,
  type AccessState,
  canSignIn,
  initialAccess,
  reduceAccess,
} from "./access-machine"

const signedIn = { state: "signedIn", email: "user@example.com", offline: false } as const

function run(events: readonly AccessEvent[]): AccessState {
  return events.reduce(reduceAccess, initialAccess)
}

describe("reduceAccess", () => {
  test("a stored sign-in opens the app and a missing one asks to sign in", () => {
    expect(run([{ type: "loaded", access: signedIn }])).toEqual({
      status: "signedIn",
      email: "user@example.com",
      offline: false,
    })
    expect(run([{ type: "loaded", access: { state: "signedOut" } }])).toEqual({
      status: "signedOut",
      error: null,
    })
  })

  test("a failed check offers both a new check and a fresh sign-in", () => {
    // Given
    const failed = run([{ type: "loadFailed", message: "읽지 못했습니다." }])

    // Then
    expect(failed).toEqual({ status: "failed", message: "읽지 못했습니다.", retryable: true })
    expect(canSignIn(failed)).toBe(true)
    expect(reduceAccess(failed, { type: "checkStarted" })).toEqual(initialAccess)
    expect(reduceAccess(failed, { type: "signInStarted" })).toEqual({ status: "signingIn" })
  })

  test("an unreachable runtime leaves no way forward but a restart", () => {
    // Given
    const failed = run([{ type: "runtimeUnavailable", message: "다시 실행해 주세요." }])

    // When
    const after = [
      { type: "checkStarted" },
      { type: "signInStarted" },
      { type: "loaded", access: signedIn },
    ].reduce((state, event) => reduceAccess(state, event as AccessEvent), failed)

    // Then
    expect(canSignIn(failed)).toBe(false)
    expect(after).toEqual({ status: "failed", message: "다시 실행해 주세요.", retryable: false })
  })

  test("a sign-in ends signed in, signed out with its error, or signed out when cancelled", () => {
    const signingIn = run([
      { type: "loaded", access: { state: "signedOut" } },
      { type: "signInStarted" },
    ])

    expect(signingIn).toEqual({ status: "signingIn" })
    expect(canSignIn(signingIn)).toBe(false)
    expect(reduceAccess(signingIn, { type: "signInSucceeded", access: signedIn }).status).toBe(
      "signedIn",
    )
    expect(reduceAccess(signingIn, { type: "signInFailed", message: "실패" })).toEqual({
      status: "signedOut",
      error: "실패",
    })
    expect(reduceAccess(signingIn, { type: "signInCancelled" })).toEqual({
      status: "signedOut",
      error: null,
    })
  })

  test("a sign-out returns to the login screen even when it fails", () => {
    const signingOut = run([{ type: "loaded", access: signedIn }, { type: "signOutStarted" }])

    expect(signingOut).toEqual({ status: "signingOut" })
    expect(canSignIn(signingOut)).toBe(false)
    expect(reduceAccess(signingOut, { type: "signOutSucceeded" })).toEqual({
      status: "signedOut",
      error: null,
    })
    expect(reduceAccess(signingOut, { type: "signOutFailed", message: "지우지 못했습니다." })).toEqual(
      { status: "signedOut", error: "지우지 못했습니다." },
    )
  })

  test("outcomes that answer no pending attempt are ignored", () => {
    const open = run([{ type: "loaded", access: signedIn }])

    expect(reduceAccess(open, { type: "signInFailed", message: "늦은 실패" })).toBe(open)
    expect(reduceAccess(open, { type: "loaded", access: { state: "signedOut" } })).toBe(open)
    expect(reduceAccess(open, { type: "signInStarted" })).toBe(open)
  })
})
