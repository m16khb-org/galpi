import { describe, expect, test } from "bun:test"
import { readFile } from "node:fs/promises"
import { join } from "node:path"
import { resolveSidecarTarget, SidecarStageError } from "./sidecar-targets"

describe("resolveSidecarTarget", () => {
  test("darwin arm64 resolves the macOS tarball", () => {
    // Given/When
    const target = resolveSidecarTarget("darwin", "arm64")
    // Then
    expect(target.target).toBe("aarch64-apple-darwin")
    expect(target.archiveName).toBe("uv-aarch64-apple-darwin.tar.gz")
    expect(target.innerPath).toBe("uv-aarch64-apple-darwin/uv")
    expect(target.stagedName).toBe("uv-aarch64-apple-darwin")
    expect(target.archiveSha256).toBe(
      "5bb0e5fe008a773c3dbcb97ff79cd89e1241464fe9d2f986d52ad8f1b037bd62",
    )
    expect(target.binarySha256).toBe(
      "ad3564874e19defa0debefcf48e8381ac1d087c584190c1323c247bd351dd25f",
    )
  })

  test("win32 x64 resolves the Windows zip", () => {
    const target = resolveSidecarTarget("win32", "x64")
    expect(target.target).toBe("x86_64-pc-windows-msvc")
    expect(target.archiveName).toBe("uv-x86_64-pc-windows-msvc.zip")
    expect(target.innerPath).toBe("uv.exe")
    expect(target.stagedName).toBe("uv-x86_64-pc-windows-msvc.exe")
    expect(target.archiveSha256).toBe(
      "4c4d49d8738847d9b71ba319e49a5688c93eac0fe6204b1df24e98528dddf39a",
    )
    expect(target.binarySha256).toBe(
      "8da6cedef60c27ac997ebf400fbfc6d373c5b0a7ae6a299b9d52be7fe63723fb",
    )
  })

  test("GALPI_SIDECAR_TARGET overrides the host", () => {
    const target = resolveSidecarTarget("darwin", "arm64", {
      GALPI_SIDECAR_TARGET: "x86_64-pc-windows-msvc",
    })
    expect(target.stagedName).toBe("uv-x86_64-pc-windows-msvc.exe")
  })

  test("unsupported host or override throws SidecarStageError", () => {
    expect(() => resolveSidecarTarget("linux", "x64")).toThrow(SidecarStageError)
    expect(() => resolveSidecarTarget("win32", "arm64")).toThrow(SidecarStageError)
    expect(() => resolveSidecarTarget("darwin", "arm64", { GALPI_SIDECAR_TARGET: "bogus" })).toThrow(
      SidecarStageError,
    )
  })

  test("Rust platform.rs knows every staged name", async () => {
    const source = await readFile(
      join(import.meta.dir, "..", "src-tauri", "src", "adapters", "outbound", "platform.rs"),
      "utf8",
    )
    for (const [platform, arch] of [
      ["darwin", "arm64"],
      ["win32", "x64"],
    ] as const) {
      expect(source).toContain(resolveSidecarTarget(platform, arch).stagedName)
    }
  })
})
