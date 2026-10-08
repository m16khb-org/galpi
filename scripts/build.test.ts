import { expect, test } from "bun:test"
import { bundleCommands } from "./build"

test("darwin builds the app bundle then the dmg", () => {
  expect(bundleCommands("darwin")).toEqual([
    ["cargo", "tauri", "build", "--bundles", "app", "--ci"],
    ["bun", "run", "dmg:build"],
  ])
})

test("win32 builds only the NSIS installer", () => {
  expect(bundleCommands("win32")).toEqual([["cargo", "tauri", "build", "--bundles", "nsis", "--ci"]])
})

test("other platforms fail clearly", () => {
  expect(() => bundleCommands("linux")).toThrow(/unsupported build platform: linux/)
})
