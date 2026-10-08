import { expect, test } from "bun:test"
import { isInsideDirectory } from "./architecture-paths"

test("detects children with POSIX separators", () => {
  expect(isInsideDirectory("/a/outbound/process/spawn.rs", "/a/outbound/process")).toBe(true)
  expect(isInsideDirectory("/a/outbound/process.rs", "/a/outbound/process")).toBe(false)
})

test("detects children with Windows separators", () => {
  expect(isInsideDirectory("C:\\a\\outbound\\process\\spawn.rs", "C:\\a\\outbound\\process")).toBe(
    true,
  )
  expect(isInsideDirectory("C:\\a\\outbound\\process_x\\a.rs", "C:\\a\\outbound\\process")).toBe(
    false,
  )
})

test("mixed separators still match", () => {
  expect(isInsideDirectory("C:\\a\\outbound/process/spawn.rs", "C:/a/outbound\\process")).toBe(true)
})
