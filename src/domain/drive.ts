import type { ArtifactKind } from "./backend"

/** What the host tells the window about Drive. No token ever crosses the IPC border. */
export type DriveStatus =
  | { readonly state: "disconnected"; readonly onboardingDismissed: boolean }
  | {
      readonly state: "connected"
      readonly email: string
      /** Upload each finished microphone recording right away; ends with the connection. */
      readonly saveRecordings: boolean
    }

/**
 * The Google OAuth client the user saved for Drive; the app ships none. The
 * client secret never crosses the IPC border, so there is no field for it;
 * `clientId` is a string exactly when `configured` is true.
 */
export interface DriveClientStatus {
  readonly configured: boolean
  readonly clientId: string | null
}

/** A recording the Picker chose, downloaded and ready to transcribe from `inputPath`. */
export interface DriveImport {
  readonly jobId: string
  readonly inputPath: string
  readonly driveFileId: string
  readonly name: string
}

export type DriveSyncState = "uploaded" | "updated" | "unchanged" | "conflict" | "failed"

export type DriveSyncFile =
  | {
      readonly kind: ArtifactKind
      readonly state: "failed"
      readonly message: string
      readonly webViewLink: string | null
    }
  | {
      readonly kind: ArtifactKind
      readonly state: Exclude<DriveSyncState, "failed">
      readonly webViewLink: string | null
    }

export interface DriveSyncResult {
  readonly folderLink: string | null
  readonly files: readonly DriveSyncFile[]
}

export type DriveTone = "ready" | "pending" | "error"

const KIND_LABELS: Readonly<Record<ArtifactKind, string>> = {
  srt: "자막",
  speaker_text: "화자별 텍스트",
  checkpoint: "정렬 체크포인트",
  minutes: "회의록",
}

const STATE_LABELS: Readonly<Record<Exclude<DriveSyncState, "failed">, string>> = {
  uploaded: "올림",
  updated: "갱신함",
  unchanged: "변경 없음",
  conflict: "Drive에서 바뀌어 건너뜀",
}

const DRIVE_CONFLICT_MESSAGE = "Drive의 파일이 다른 곳에서 바뀌어 덮어쓰지 않았습니다."

/** One line per file: what it is and how it fared. */
export function syncFileLine(file: DriveSyncFile): string {
  const label = KIND_LABELS[file.kind]
  return file.state === "failed"
    ? `${label} · 올리지 못함: ${file.message}`
    : `${label} · ${STATE_LABELS[file.state]}`
}

/** The headline of a sync: failures first, then conflicts, then what changed. */
export function syncSummary(result: DriveSyncResult): {
  readonly text: string
  readonly tone: DriveTone
  readonly conflict: boolean
} {
  const conflict = result.files.some((file) => file.state === "conflict")
  if (result.files.some((file) => file.state === "failed")) {
    return { text: "일부 결과를 Drive에 올리지 못했습니다.", tone: "error", conflict }
  }
  if (conflict) return { text: DRIVE_CONFLICT_MESSAGE, tone: "pending", conflict }
  const changed = result.files.some((file) => file.state === "uploaded" || file.state === "updated")
  return {
    text: changed ? "Drive의 Galpi 폴더에 올렸습니다." : "Drive에 이미 최신 결과가 있습니다.",
    tone: "ready",
    conflict,
  }
}

/** How uploading one finished recording ended. */
export type DriveRecordingState = "uploaded" | "updated" | "unchanged" | "conflict"

export interface DriveRecordingUpload {
  readonly state: DriveRecordingState
  readonly folderLink: string | null
  readonly webViewLink: string | null
}

export const RECORDING_UPLOADING = "녹음 파일을 Drive에 올리는 중입니다."

/** The headline under the recorder once the upload is over. */
export function recordingUploadSummary(upload: DriveRecordingUpload): {
  readonly text: string
  readonly tone: DriveTone
} {
  switch (upload.state) {
    case "uploaded":
    case "updated":
      return { text: "녹음 파일을 Drive의 Galpi 폴더에 저장했습니다.", tone: "ready" }
    case "unchanged":
      return { text: "이 녹음은 Drive에 이미 저장되어 있습니다.", tone: "ready" }
    case "conflict":
      return {
        text: "Drive의 녹음 파일이 다른 곳에서 바뀌어 덮어쓰지 않았습니다.",
        tone: "pending",
      }
  }
}

export function recordingUploadFailure(message: string): string {
  return `녹음 파일을 Drive에 올리지 못했습니다. ${message}`
}
