import { describe, expect, test } from "bun:test"

import { errorCode, errorDetail, errorMessage } from "./backend"

describe("errorMessage", () => {
  test("passes through AppError copy from the native boundary", () => {
    // Given: Tauri commands reject with the serialized AppError {code, message}
    const appError = { code: "IO_ERROR", message: "출력 폴더를 만들 수 없습니다." }

    // Then: the user-facing Korean message is preserved
    expect(errorMessage(appError)).toBe("출력 폴더를 만들 수 없습니다.")
  })

  test("replaces raw runtime faults with stable Korean copy", () => {
    // Given: a browser/runtime TypeError with English internals
    const fault = new TypeError("Cannot read properties of undefined (reading 'invoke')")

    // Then: the user never sees the raw diagnostic as status copy
    expect(errorMessage(fault)).toBe("예기치 못한 오류가 발생했습니다.")
    expect(errorMessage("string error")).toBe("예기치 못한 오류가 발생했습니다.")
    expect(errorMessage(undefined)).toBe("예기치 못한 오류가 발생했습니다.")
  })

  test("rejects lookalikes without a machine-readable code", () => {
    // Given: an object with only a message (e.g. a parsed event payload)
    const lookalike = { message: "not an AppError" }

    expect(errorMessage(lookalike)).toBe("예기치 못한 오류가 발생했습니다.")
  })
})

describe("errorDetail", () => {
  test("keeps the raw diagnostic for the log disclosure", () => {
    const fault = new TypeError("Cannot read properties of undefined (reading 'invoke')")

    expect(errorDetail(fault)).toBe("Cannot read properties of undefined (reading 'invoke')")
    expect(errorDetail({ code: "IO_ERROR", message: "io" })).toBe("io")
    expect(errorDetail(42)).toBe("42")
  })
})

describe("errorCode", () => {
  test("reads the stable code of a native AppError", () => {
    // Given: the host rejects with {code, message}
    const appError = { code: "CHATGPT_CONSENT_DENIED", message: "동의가 거부되었습니다." }

    // Then
    expect(errorCode(appError)).toBe("CHATGPT_CONSENT_DENIED")
  })

  test("is null for anything that is not an AppError", () => {
    // Given: runtime faults and lookalikes carry no stable code
    expect(errorCode(new TypeError("boom"))).toBeNull()
    expect(errorCode({ message: "no code" })).toBeNull()
    expect(errorCode({ code: 7, message: "numeric code" })).toBeNull()
    expect(errorCode("string error")).toBeNull()
    expect(errorCode(undefined)).toBeNull()
  })
})
