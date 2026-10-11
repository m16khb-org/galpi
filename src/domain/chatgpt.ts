export type AssistantAuthMode = "apiKey" | "chatGpt"

type ChatGptAccountState = "signedOut" | "signInRequired" | "signedIn"

/** Only the stored fact and the e-mail cross the IPC border — never a token. */
export interface ChatGptAccount {
  readonly state: ChatGptAccountState
  readonly email: string | null
}

export interface ChatGptSettings {
  readonly authMode: AssistantAuthMode
  readonly model: string | null
  /** Null leaves the effort to the model's own default. */
  readonly reasoningEffort: string | null
  readonly welcomeAcknowledged: boolean
  readonly account: ChatGptAccount
}

/** The part of the ChatGPT settings the window owns and autosaves. */
export interface ChatGptPreferences {
  readonly authMode: AssistantAuthMode
  readonly model: string | null
  readonly reasoningEffort: string | null
  readonly welcomeAcknowledged: boolean
}

export interface ChatGptModel {
  readonly slug: string
  readonly displayName: string
  /** The efforts the model accepts in the server's order; empty when the list did not say. */
  readonly reasoningEfforts: readonly string[]
  readonly defaultReasoningEffort: string | null
}

/** Korean labels for the `reasoning.effort` values the Responses API defines. */
export const EFFORT_LABELS: Readonly<Record<string, string>> = {
  none: "끔",
  minimal: "최소",
  low: "낮음",
  medium: "중간",
  high: "높음",
  xhigh: "매우 높음",
  max: "최대",
}

/** Offered when the model list does not say which efforts a model accepts. */
const FALLBACK_EFFORTS = ["low", "medium", "high"] as const

/** The efforts to offer for `model`: what the list says, or the widely accepted three. */
export function effortChoices(model: ChatGptModel | undefined): readonly string[] {
  if (model === undefined) return []
  return model.reasoningEfforts.length > 0 ? model.reasoningEfforts : FALLBACK_EFFORTS
}

/** A saved effort survives only while the model still accepts it; otherwise the model decides. */
export function pickEffort(model: ChatGptModel | undefined, saved: string | null): string | null {
  return saved !== null && effortChoices(model).includes(saved) ? saved : null
}

export type ChatGptSignInPhase = "awaitingBrowser" | "exchanging"

export interface ChatGptSignOutResult {
  readonly settings: ChatGptSettings
  /** False when the server-side revocation could not be confirmed; local data is gone either way. */
  readonly remoteRevocationConfirmed: boolean
}

/** Whether the augment stage can start with the credentials the chosen mode needs. */
export function assistantReady(
  mode: AssistantAuthMode,
  apiKeyStored: boolean,
  chatGpt: ChatGptAccount,
): boolean {
  return mode === "chatGpt" ? chatGpt.state === "signedIn" : apiKeyStored
}

/** The server's model order is the account's own default; a still-listed saved choice wins. */
export function pickDefaultModel(
  models: readonly ChatGptModel[],
  saved: string | null,
): string | null {
  if (saved !== null && models.some((model) => model.slug === saved)) return saved
  return models[0]?.slug ?? null
}

const CONSENT_DENIED_MESSAGE =
  "ChatGPT 요금제 사용 동의가 거부되었습니다. 회의록 정제에 ChatGPT를 쓰려면 'ChatGPT로 계속하기'를 다시 눌러 동의해 주세요."

/** Consent denial gets fixed wording; every other failure keeps the host's Korean message. */
export function signInFailureMessage(code: string | null, hostMessage: string): string {
  return code === "CHATGPT_CONSENT_DENIED" ? CONSENT_DENIED_MESSAGE : hostMessage
}
