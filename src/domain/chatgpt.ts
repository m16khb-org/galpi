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
  readonly welcomeAcknowledged: boolean
  readonly account: ChatGptAccount
}

/** The part of the ChatGPT settings the window owns and autosaves. */
export interface ChatGptPreferences {
  readonly authMode: AssistantAuthMode
  readonly model: string | null
  readonly welcomeAcknowledged: boolean
}

export interface ChatGptModel {
  readonly slug: string
  readonly displayName: string
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
