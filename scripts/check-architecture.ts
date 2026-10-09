import type { Dirent } from "node:fs"
import { readdir, readFile } from "node:fs/promises"
import { join } from "node:path"
import { isInsideDirectory } from "./architecture-paths"

const RUST_ROOT = join(import.meta.dir, "..", "src-tauri", "src")
const TS_ROOT = join(import.meta.dir, "..", "src")

interface Fence {
  readonly root: string
  readonly forbidden: readonly string[]
  readonly extension: ".rs" | ".ts"
}

// Platform branching lives only in outbound adapters.
const PLATFORM_CFG = ["cfg(windows)", "cfg(unix)", "target_os", "cfg!("] as const

const fences: readonly Fence[] = [
  {
    root: join(RUST_ROOT, "domain"),
    extension: ".rs",
    forbidden: [
      "crate::application",
      "crate::adapters",
      "crate::composition",
      "tauri::",
      ...PLATFORM_CFG,
    ],
  },
  {
    root: join(RUST_ROOT, "application"),
    extension: ".rs",
    forbidden: ["crate::adapters", "crate::composition", "tauri::", ...PLATFORM_CFG],
  },
  {
    root: join(RUST_ROOT, "adapters", "inbound"),
    extension: ".rs",
    forbidden: ["adapters::outbound", "crate::composition", ...PLATFORM_CFG],
  },
  {
    root: join(RUST_ROOT, "adapters", "outbound"),
    extension: ".rs",
    forbidden: ["adapters::inbound", "crate::composition"],
  },
  // Frontend mirrors the same dependency rule: contracts live in `domain`,
  // the Tauri implementation stays in `adapters`, and `ui` stays framework-free.
  {
    root: join(TS_ROOT, "domain"),
    extension: ".ts",
    forbidden: ["../application/", "../ui/", "../adapters/"],
  },
  {
    root: join(TS_ROOT, "application"),
    extension: ".ts",
    forbidden: ["../ui/", "../adapters/", "@tauri-apps/"],
  },
  {
    root: join(TS_ROOT, "ui"),
    extension: ".ts",
    forbidden: ["../adapters/", "@tauri-apps/"],
  },
  {
    root: join(TS_ROOT, "adapters"),
    extension: ".ts",
    forbidden: ["../ui/", "../application/"],
  },
]

class ArchitectureError extends Error {
  readonly name = "ArchitectureError"
}

async function sourceFiles(root: string, extension: string): Promise<readonly string[]> {
  const entries: readonly Dirent[] = await readdir(root, { withFileTypes: true })
  const nested = await Promise.all(
    entries.map(async (entry): Promise<readonly string[]> => {
      const path = join(root, entry.name)
      if (entry.isDirectory()) {
        return sourceFiles(path, extension)
      }
      return entry.isFile() && path.endsWith(extension) ? [path] : []
    }),
  )
  return nested.flat()
}

async function checkFence(fence: Fence): Promise<readonly string[]> {
  const violations: string[] = []
  for (const path of await sourceFiles(fence.root, fence.extension)) {
    const source = await readFile(path, "utf8")
    for (const forbidden of fence.forbidden) {
      if (source.includes(forbidden)) {
        violations.push(`${path}: forbidden dependency ${forbidden}`)
      }
    }
  }
  return violations
}

async function checkFrameworkLocality(): Promise<readonly string[]> {
  const violations: string[] = []
  const inbound = join(RUST_ROOT, "adapters", "inbound", "tauri.rs")
  const composition = join(RUST_ROOT, "composition.rs")
  const processAdapter = join(RUST_ROOT, "adapters", "outbound", "process.rs")
  const processAdapterDirectory = join(RUST_ROOT, "adapters", "outbound", "process")
  for (const path of await sourceFiles(RUST_ROOT, ".rs")) {
    const source = await readFile(path, "utf8")
    if (source.includes("#[tauri::command]") && path !== inbound) {
      violations.push(`${path}: Tauri command belongs in the inbound adapter`)
    }
    if (
      (source.includes("generate_handler!") ||
        source.includes(".manage(") ||
        source.includes(".plugin(")) &&
      path !== composition
    ) {
      violations.push(`${path}: Tauri composition belongs in composition.rs`)
    }
    const usesNix = /(^|[\s({])nix::/m.test(source)
    if (
      (source.includes("tokio::process") || usesNix) &&
      path !== processAdapter &&
      !isInsideDirectory(path, processAdapterDirectory)
    ) {
      violations.push(`${path}: process primitives belong in the process adapter`)
    }
  }
  return violations
}

const violations = [
  ...(await Promise.all(fences.map(checkFence))).flat(),
  ...(await checkFrameworkLocality()),
]
if (violations.length > 0) {
  throw new ArchitectureError(violations.join("\n"))
}
