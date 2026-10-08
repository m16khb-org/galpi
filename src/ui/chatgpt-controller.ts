import {
  type ChatGptFlowEvent,
  type ChatGptFlowState,
  initialChatGptFlow,
  isChatGptFlowActive,
  reduceChatGptFlow,
} from "../application/chatgpt-machine"
import { type BackendPort, errorCode, errorMessage } from "../domain/backend"
import {
  assistantReady,
  type ChatGptAccount,
  type ChatGptPreferences,
  type ChatGptSettings,
  pickDefaultModel,
} from "../domain/chatgpt"
import type { AppView } from "./app-view"

const SIGNED_OUT: ChatGptAccount = { state: "signedOut", email: null }
const USAGE_LIMIT_CODE = "CHATGPT_USAGE_LIMIT_EXCEEDED"

/**
 * ChatGPT sign-in, account status and model list for the settings sheet.
 *
 * The host owns the account; this window only mirrors it and autosaves the
 * preferences it owns (`authMode`, `model`, `welcomeAcknowledged`). A failed
 * sign-in is shown and left alone — nothing here retries it.
 */
export class ChatGptController {
  private flow: ChatGptFlowState = initialChatGptFlow
  private settings: ChatGptSettings | null = null
  private unlisten: (() => void) | null = null

  constructor(
    private readonly backend: BackendPort,
    private readonly view: AppView,
    private readonly requestSave: () => void,
  ) {}

  /** Subscribe before any sign-in call so the first phase event cannot be missed. */
  async subscribe(): Promise<void> {
    this.unlisten = await this.backend.listenToChatGptEvents((phase) =>
      this.dispatch({ type: "phase", phase }),
    )
  }

  dispose(): void {
    this.unlisten?.()
    this.unlisten = null
  }

  /** Whether the augment stage has the credentials the chosen mode needs. */
  ready(apiKeyStored: boolean): boolean {
    return assistantReady(
      this.view.chatGptSettings.authMode(),
      apiKeyStored,
      this.settings?.account ?? SIGNED_OUT,
    )
  }

  async load(): Promise<void> {
    this.apply(await this.backend.loadChatGptSettings())
  }

  /** Opening the sheet reloads the account from the host; models come later via `showModels`. */
  async open(): Promise<void> {
    try {
      await this.load()
    } catch (error) {
      this.view.chatGptSettings.setMessage(errorMessage(error), "error")
    }
  }

  /**
   * The model list is a remote call (and maybe a token refresh), so it runs
   * outside the sheet's busy window and only while the ChatGPT panel shows.
   */
  async showModels(): Promise<void> {
    if (this.signedIn() && this.view.chatGptSettings.authMode() === "chatGpt") {
      await this.refreshModels()
    }
  }

  /** The preferences to save, or null when they match what the host already has. */
  changedPreferences(): ChatGptPreferences | null {
    const saved = this.settings
    if (saved === null) return null
    const current: ChatGptPreferences = {
      authMode: this.view.chatGptSettings.authMode(),
      model: this.view.chatGptSettings.selectedModel() ?? saved.model,
      welcomeAcknowledged: this.view.chatGptSettings.welcomeAcknowledged(),
    }
    const unchanged =
      current.authMode === saved.authMode &&
      current.model === saved.model &&
      current.welcomeAcknowledged === saved.welcomeAcknowledged
    return unchanged ? null : current
  }

  async persist(): Promise<void> {
    const preferences = this.changedPreferences()
    if (preferences === null || this.settings === null) return
    const switchedToChatGpt =
      preferences.authMode === "chatGpt" && this.settings.authMode !== "chatGpt"
    await this.backend.saveChatGptPreferences(preferences)
    this.settings = { ...this.settings, ...preferences }
    if (switchedToChatGpt) void this.showModels()
  }

  acknowledgeWelcome(): void {
    this.view.chatGptSettings.setWelcomeAcknowledged(true)
    this.requestSave()
  }

  async signIn(): Promise<void> {
    if (isChatGptFlowActive(this.flow)) return
    this.dispatch({ type: "started" })
    try {
      this.apply(await this.backend.signInWithChatGpt())
      this.dispatch({ type: "succeeded" })
    } catch (error) {
      if (errorCode(error) === "CANCELLED") {
        this.dispatch({ type: "cancelled" })
        this.view.chatGptSettings.setMessage("ChatGPT 로그인을 취소했습니다.", "ready")
      } else {
        this.dispatch({ type: "failed", code: errorCode(error), message: errorMessage(error) })
      }
      return
    }
    await this.refreshModels()
  }

  async cancelSignIn(): Promise<void> {
    if (!isChatGptFlowActive(this.flow)) return
    try {
      await this.backend.cancelChatGptSignIn()
    } catch (error) {
      this.view.chatGptSettings.setMessage(errorMessage(error), "error")
    }
  }

  async signOut(): Promise<void> {
    const view = this.view.chatGptSettings
    view.setBusy(true)
    try {
      const result = await this.backend.signOutOfChatGpt()
      this.apply(result.settings)
      view.setModels([], null)
      // The ChatGPT panel hides with the return to API key mode, so the outcome
      // goes to the sheet-wide status line that stays on screen and is announced.
      this.view.tokenSettings.showMessage(
        result.remoteRevocationConfirmed
          ? "ChatGPT에서 로그아웃했습니다. API 키 모드로 돌아갑니다."
          : "ChatGPT에서 로그아웃했습니다. 서버 쪽 연결 해제는 확인하지 못했으니, 필요하면 ChatGPT 설정의 연결된 앱에서 갈피를 해제해 주세요.",
        result.remoteRevocationConfirmed ? "ready" : "error",
      )
    } catch (error) {
      view.setMessage(errorMessage(error), "error")
    } finally {
      view.setBusy(false)
    }
  }

  async openUsagePage(): Promise<void> {
    try {
      await this.backend.openChatGptUsagePage()
    } catch (error) {
      this.view.showError(errorMessage(error))
    }
  }

  /** A refinement that ran into the plan's usage limit points the user at the usage page. */
  noteFailure(error: unknown): void {
    this.view.chatGptSettings.showLimitHint(errorCode(error) === USAGE_LIMIT_CODE)
  }

  clearLimitHint(): void {
    this.view.chatGptSettings.showLimitHint(false)
  }

  private async refreshModels(): Promise<void> {
    const view = this.view.chatGptSettings
    try {
      const models = await this.backend.listChatGptModels()
      // A sign-out while the request was in flight makes the list stale.
      if (!this.signedIn()) return
      const picked = pickDefaultModel(models, this.settings?.model ?? null)
      view.setModels(models, picked)
      // A saved choice the account no longer lists is replaced, and that is worth saving.
      if (picked !== (this.settings?.model ?? null)) this.requestSave()
    } catch (error) {
      if (!this.signedIn()) return
      view.setModels([], null)
      view.setMessage(errorMessage(error), "error")
    }
  }

  private signedIn(): boolean {
    return this.settings?.account.state === "signedIn"
  }

  private dispatch(event: ChatGptFlowEvent): void {
    this.flow = reduceChatGptFlow(this.flow, event)
    this.view.chatGptSettings.setFlow(this.flow)
  }

  private apply(settings: ChatGptSettings): void {
    this.settings = settings
    const view = this.view.chatGptSettings
    view.setAuthMode(settings.authMode)
    view.setAccount(settings.account)
    view.setWelcomeAcknowledged(settings.welcomeAcknowledged)
    this.view.setAssistantKeyReady(this.ready(this.view.assistantSettings.settings().apiKeyStored))
  }
}
