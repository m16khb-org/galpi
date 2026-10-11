/**
 * What a recording captures. `microphoneAndSystem` adds the computer's own
 * output (the remote side of a video call) to the same file.
 */
export type RecordingSource = "microphone" | "microphoneAndSystem"

export const defaultRecordingSource: RecordingSource = "microphone"
