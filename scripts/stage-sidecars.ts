import { createHash } from "node:crypto"
import { chmod, cp, mkdir, mkdtemp, rm } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { resolveSidecarTarget, SidecarStageError, UV_VERSION } from "./sidecar-targets"

const sidecar = resolveSidecarTarget(process.platform, process.arch, process.env)
const BINARY_DIR = join(import.meta.dir, "..", "src-tauri", "binaries")
const BINARY_PATH = join(BINARY_DIR, sidecar.stagedName)
const WORKER_SOURCE = join(import.meta.dir, "..", "worker")
const WORKER_DESTINATION = join(import.meta.dir, "..", "src-tauri", "resources", "worker")

async function run(command: readonly string[]): Promise<void> {
  const child = Bun.spawn([...command], {
    stdout: "inherit",
    stderr: "inherit",
  })
  const exitCode = await child.exited
  if (exitCode !== 0) {
    throw new SidecarStageError(`${command[0]} exited with code ${exitCode}`)
  }
}

function sha256(bytes: Uint8Array): string {
  return createHash("sha256").update(bytes).digest("hex")
}

function extractCommand(archivePath: string, workDir: string): readonly string[] {
  if (sidecar.archiveName.endsWith(".tar.gz")) return ["tar", "-xzf", archivePath, "-C", workDir]
  if (sidecar.archiveName.endsWith(".zip")) {
    return process.platform === "win32"
      ? [
          "powershell",
          "-NoProfile",
          "-Command",
          `Expand-Archive -LiteralPath '${archivePath}' -DestinationPath '${workDir}' -Force`,
        ]
      : ["unzip", "-q", "-o", archivePath, "-d", workDir]
  }
  throw new SidecarStageError(`unsupported archive type: ${sidecar.archiveName}`)
}

async function stageUv(): Promise<void> {
  if (await Bun.file(BINARY_PATH).exists()) {
    const checksum = sha256(new Uint8Array(await Bun.file(BINARY_PATH).arrayBuffer()))
    if (checksum === sidecar.binarySha256) return
    await rm(BINARY_PATH)
  }

  await mkdir(BINARY_DIR, { recursive: true })
  const workDir = await mkdtemp(join(tmpdir(), "galpi-uv-"))
  const archivePath = join(workDir, sidecar.archiveName)
  const releaseUrl = `https://github.com/astral-sh/uv/releases/download/${UV_VERSION}/${sidecar.archiveName}`

  const response = await fetch(releaseUrl)
  if (!response.ok) {
    throw new SidecarStageError(`uv download failed: ${response.status} ${releaseUrl}`)
  }
  const archive = new Uint8Array(await response.arrayBuffer())
  const checksum = sha256(archive)
  if (checksum !== sidecar.archiveSha256) {
    throw new SidecarStageError(`uv archive checksum mismatch: ${checksum}`)
  }
  await Bun.write(archivePath, archive)

  await run(extractCommand(archivePath, workDir))
  await Bun.write(BINARY_PATH, Bun.file(join(workDir, sidecar.innerPath)))
  if (process.platform !== "win32") await chmod(BINARY_PATH, 0o755)
  await rm(workDir, { recursive: true, force: true })
}

async function stageWorker(): Promise<void> {
  await rm(WORKER_DESTINATION, { recursive: true, force: true })
  await mkdir(WORKER_DESTINATION, { recursive: true })
  await cp(join(WORKER_SOURCE, "galpi_worker"), join(WORKER_DESTINATION, "galpi_worker"), {
    recursive: true,
    force: true,
    filter: (source) => !source.includes("__pycache__") && !source.endsWith(".pyc"),
  })
  // The locks are what the installer actually reads; the loose requirements
  // files travel with them so the pins stay readable next to their source.
  const requirements = [
    "requirements.txt",
    "requirements.lock",
    "requirements-qwen3.txt",
    "requirements-qwen3.lock",
    "requirements-windows-cpu.lock",
    "requirements-windows-cuda.lock",
  ]
  for (const file of requirements) {
    await cp(join(WORKER_SOURCE, file), join(WORKER_DESTINATION, file))
  }
}

await stageUv()
await stageWorker()
