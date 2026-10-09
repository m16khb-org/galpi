import { type ChatGptFlowState, isChatGptFlowActive } from "../application/chatgpt-machine"
import {
  type AssistantAuthMode,
  type ChatGptAccount,
  type ChatGptModel,
  signInFailureMessage,
} from "../domain/chatgpt"
import { required } from "./dom"

const MODE_SELECTOR = 'input[name="assistant-auth-mode"]'
const MODEL_SELECTOR = "#settings-chatgpt-model"

const FLOW_MESSAGES = {
  awaitingBrowser:
    "브라우저에서 ChatGPT 로그인을 마쳐 주세요. 로그인 창이 보이지 않으면 취소 후 다시 시도하세요.",
  exchanging: "로그인을 마무리하는 중입니다.",
} as const

const ACCOUNT_BADGES = {
  signedOut: "로그인 안 됨",
  signInRequired: "로그인 필요",
  signedIn: "로그인됨",
} as const

type ChatGptMessageKind = "ready" | "saving" | "error"

/**
 * The ChatGPT half of the AI augmentation section: mode switch, sign-in
 * controls, account status and the account's model list. It holds no token —
 * the window only ever learns whether an account is signed in and its e-mail.
 */
export class ChatGptSettingsView {
  private readonly root: HTMLElement
  private account: ChatGptAccount = { state: "signedOut", email: null }
  private flow: ChatGptFlowState = { status: "idle", code: null, message: null }
  private welcomeSeen = false
  private busy = false

  constructor(root: HTMLElement) {
    this.root = root
    // Panels follow the radio on their own; saving the choice is the controller's job.
    for (const input of this.modeInputs()) {
      input.addEventListener("change", () => this.render())
    }
    this.render()
  }

  authMode(): AssistantAuthMode {
    return this.modeInputs().find((input) => input.checked)?.value === "chatGpt"
      ? "chatGpt"
      : "apiKey"
  }

  setAuthMode(mode: AssistantAuthMode): void {
    for (const input of this.modeInputs()) input.checked = input.value === mode
    this.render()
  }

  /** The slug the user picked, or null while the account's list is not loaded. */
  selectedModel(): string | null {
    const value = this.element<HTMLSelectElement>(MODEL_SELECTOR).value
    return value.length > 0 ? value : null
  }

  welcomeAcknowledged(): boolean {
    return this.welcomeSeen
  }

  setWelcomeAcknowledged(acknowledged: boolean): void {
    this.welcomeSeen = acknowledged
    this.render()
  }

  setAccount(account: ChatGptAccount): void {
    this.account = account
    this.render()
  }

  /** Fill the select in the server's order; `selected` is applied only if the list carries it. */
  setModels(models: readonly ChatGptModel[], selected: string | null): void {
    const select = this.element<HTMLSelectElement>(MODEL_SELECTOR)
    select.replaceChildren()
    if (models.length === 0) {
      const placeholder = select.ownerDocument.createElement("option")
      placeholder.value = ""
      placeholder.textContent = "모델 목록 없음"
      placeholder.disabled = true
      placeholder.selected = true
      select.appendChild(placeholder)
      return
    }
    for (const model of models) {
      const option = select.ownerDocument.createElement("option")
      option.value = model.slug
      option.textContent = model.displayName
      select.appendChild(option)
    }
    select.value = selected !== null && models.some((m) => m.slug === selected) ? selected : ""
  }

  setFlow(flow: ChatGptFlowState): void {
    this.flow = flow
    if (flow.status === "failed") {
      this.setMessage(signInFailureMessage(flow.code, flow.message ?? ""), "error")
    } else if (flow.status === "idle") {
      this.setMessage("", "ready")
    } else {
      this.setMessage(FLOW_MESSAGES[flow.status], "saving")
    }
    this.render()
  }

  setMessage(text: string, kind: ChatGptMessageKind): void {
    const message = this.element("#chatgpt-message")
    message.textContent = text
    message.dataset["state"] = kind
  }

  /** The usage-limit hint lives in the augment panel, next to the action that hit the limit. */
  showLimitHint(visible: boolean): void {
    this.element("#chatgpt-limit-hint").hidden = !visible
  }

  setBusy(busy: boolean): void {
    this.busy = busy
    this.render()
  }

  private render(): void {
    const mode = this.authMode()
    const { state } = this.account
    const signedIn = state === "signedIn"
    const flowActive = isChatGptFlowActive(this.flow)

    this.element("#assistant-api-key-panel").hidden = mode !== "apiKey"
    this.element("#assistant-chatgpt-panel").hidden = mode !== "chatGpt"
    this.element("#assistant-configured-state").hidden = mode === "chatGpt"
    const badge = this.element("#chatgpt-account-state")
    badge.hidden = mode !== "chatGpt"
    badge.textContent = ACCOUNT_BADGES[state]
    badge.dataset["state"] = signedIn ? "ready" : "pending"

    const status = this.element("#chatgpt-status")
    status.dataset["state"] = state
    status.textContent = statusText(this.account)

    const signIn = this.element<HTMLButtonElement>("#chatgpt-sign-in-button")
    signIn.hidden = signedIn || flowActive
    signIn.disabled = this.busy
    signIn.textContent = state === "signInRequired" ? "다시 로그인" : "ChatGPT로 계속하기"
    this.element<HTMLButtonElement>("#chatgpt-cancel-button").hidden = !flowActive
    const signOut = this.element<HTMLButtonElement>("#chatgpt-sign-out-button")
    signOut.hidden = state === "signedOut" || flowActive
    signOut.disabled = this.busy

    this.element("#chatgpt-welcome").hidden = !signedIn || this.welcomeSeen
    this.element("#chatgpt-model-field").hidden = !signedIn || flowActive
    this.element<HTMLSelectElement>(MODEL_SELECTOR).disabled = this.busy
    for (const input of this.modeInputs()) input.disabled = this.busy || flowActive
  }

  private modeInputs(): HTMLInputElement[] {
    return [...this.root.querySelectorAll<HTMLInputElement>(MODE_SELECTOR)]
  }

  private element<T extends HTMLElement = HTMLElement>(selector: string): T {
    return required<T>(this.root, selector)
  }
}

function statusText(account: ChatGptAccount): string {
  switch (account.state) {
    case "signedIn":
      return account.email === null ? "ChatGPT로 로그인됨" : `${account.email}로 로그인됨`
    case "signInRequired":
      return account.email === null
        ? "ChatGPT 로그인이 만료되었습니다. 다시 로그인해 주세요."
        : `${account.email} 계정의 로그인이 만료되었습니다. 다시 로그인해 주세요.`
    case "signedOut":
      return "ChatGPT에 로그인하지 않았습니다."
  }
}
