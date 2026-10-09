export const UV_VERSION = "0.12.5"

export class SidecarStageError extends Error {
  readonly name = "SidecarStageError"
}

export interface SidecarTarget {
  readonly target: string
  readonly archiveName: string
  readonly archiveSha256: string
  readonly binarySha256: string
  /** Path of the uv binary inside the extracted archive. */
  readonly innerPath: string
  readonly stagedName: string
}

const TARGETS: Readonly<Record<string, SidecarTarget>> = {
  "aarch64-apple-darwin": {
    target: "aarch64-apple-darwin",
    archiveName: "uv-aarch64-apple-darwin.tar.gz",
    archiveSha256: "5bb0e5fe008a773c3dbcb97ff79cd89e1241464fe9d2f986d52ad8f1b037bd62",
    binarySha256: "ad3564874e19defa0debefcf48e8381ac1d087c584190c1323c247bd351dd25f",
    innerPath: "uv-aarch64-apple-darwin/uv",
    stagedName: "uv-aarch64-apple-darwin",
  },
  "x86_64-unknown-linux-gnu": {
    target: "x86_64-unknown-linux-gnu",
    archiveName: "uv-x86_64-unknown-linux-gnu.tar.gz",
    archiveSha256: "68a509da24b06b4223a1c0175fb5eb5bc79342b76cbeff0cfe51ac3f5b17b6b2",
    binarySha256: "b65f23a420c4acc96427efb30e5ed9bc0f7e25d2d712000f6ede77c1a0de5f46",
    innerPath: "uv-x86_64-unknown-linux-gnu/uv",
    stagedName: "uv-x86_64-unknown-linux-gnu",
  },
  "x86_64-pc-windows-msvc": {
    target: "x86_64-pc-windows-msvc",
    archiveName: "uv-x86_64-pc-windows-msvc.zip",
    archiveSha256: "4c4d49d8738847d9b71ba319e49a5688c93eac0fe6204b1df24e98528dddf39a",
    binarySha256: "8da6cedef60c27ac997ebf400fbfc6d373c5b0a7ae6a299b9d52be7fe63723fb",
    innerPath: "uv.exe",
    stagedName: "uv-x86_64-pc-windows-msvc.exe",
  },
}

function hostTriple(platform: string, arch: string): string | undefined {
  if (platform === "darwin" && arch === "arm64") return "aarch64-apple-darwin"
  if (platform === "win32" && arch === "x64") return "x86_64-pc-windows-msvc"
  if (platform === "linux" && arch === "x64") return "x86_64-unknown-linux-gnu"
  return undefined
}

export function resolveSidecarTarget(
  platform: string,
  arch: string,
  env: Readonly<Record<string, string | undefined>> = {},
): SidecarTarget {
  const override = env["GALPI_SIDECAR_TARGET"]
  const triple = override ? override : hostTriple(platform, arch)
  const resolved = triple ? TARGETS[triple] : undefined
  if (!resolved) {
    throw new SidecarStageError(
      `unsupported sidecar target: ${triple ?? `${platform}/${arch}`} (supported: ${Object.keys(TARGETS).join(", ")})`,
    )
  }
  return resolved
}
