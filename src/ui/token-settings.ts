import { required } from "./dom"
const MASKED_TOKEN = "••••••••••••"

export function nextTokenVisibility(visible: boolean): boolean {
  return !visible
}

export function tokenDisplayValue(token: string, visible: boolean): string {
  return visible ? token : MASKED_TOKEN
}

export class TokenSettingsView {
  private readonly root: HTMLElement
  /// A token this window has seen — only ever one the user just typed.
  private persistedToken: string | null = null
  /// Whether the host holds a token, whose value never crosses the IPC border.
  private stored = false
  private visible = false
  private previouslyFocused: HTMLElement | null = null

  constructor(root: HTMLElement) {
    this.root = root
    this.root.ownerDocument.addEventListener("keydown", (event) => {
      if (event.key === "Escape" && !this.element("#settings-dialog").hidden) this.close()
    })
    // A stored token is replaced by typing over it: the mask clears when the
    // field takes focus and comes back if the user leaves it empty.
    const input = this.element<HTMLInputElement>("#settings-hf-token")
    input.addEventListener("focus", () => {
      if (input.value === MASKED_TOKEN) input.value = ""
    })
    input.addEventListener("blur", () => {
      if (this.stored && input.value.trim() === "") this.render(true, this.persistedToken)
    })
  }

  show(): void {
    // APG dialog pattern: remember the invoker so close() can return the
    // keyboard user to where they were instead of dropping to <body>.
    this.previouslyFocused = this.root.ownerDocument.activeElement as HTMLElement | null
    const dialog = this.element("#settings-dialog")
    dialog.hidden = false
    this.setBackgroundInert(true)
    this.showMessage("")
    // APG dialog pattern: move focus into the dialog on open so keyboard and
    // screen-reader users land inside the modal, not on the invoker behind it.
    // The close button is the target because it stays enabled while setBusy()
    // disables the fields during load (BUG-001).
    this.element<HTMLButtonElement>(".settings-close-button").focus()
  }

  close(): void {
    this.element("#settings-dialog").hidden = true
    this.setBackgroundInert(false)
    const restore = this.previouslyFocused
    this.previouslyFocused = null
    if (restore?.isConnected) restore.focus()
  }

  /// Take the page behind the modal out of the tab order while it is open.
  ///
  /// `aria-modal` tells a screen reader the rest of the page is unavailable but
  /// does nothing for Tab, so without this a keyboard user walks straight out
  /// of the dialog and into the controls behind it.
  private setBackgroundInert(inert: boolean): void {
    const dialog = this.element("#settings-dialog")
    const siblings = dialog.parentElement?.querySelectorAll<HTMLElement>(":scope > *") ?? []
    for (const sibling of siblings) {
      if (sibling !== dialog) sibling.inert = inert
    }
  }

  token(): string {
    return this.persistedToken ?? this.element<HTMLInputElement>("#settings-hf-token").value
  }

  /// A token the user has typed and not yet saved.
  ///
  /// The mask and the token this window already saved are not new input, so
  /// an autosave triggered elsewhere in the sheet never rewrites the secret.
  pendingToken(): string | null {
    const value = this.element<HTMLInputElement>("#settings-hf-token").value.trim()
    if (value.length === 0 || value === MASKED_TOKEN || value === this.persistedToken) return null
    return value
  }

  setToken(token: string | null): void {
    this.persistedToken = token
    this.render(token !== null, token)
  }

  /// Show that a token is saved without knowing what it says.
  ///
  /// Reading the value is a keychain access, and macOS asks the user about
  /// each one, so opening settings must not need it. Typing over the mask
  /// replaces the stored token.
  setStored(stored: boolean): void {
    this.persistedToken = null
    this.render(stored, null)
  }

  private render(stored: boolean, token: string | null): void {
    const input = this.element<HTMLInputElement>("#settings-hf-token")
    this.stored = stored
    this.visible = false
    input.value = stored ? tokenDisplayValue(token ?? "", false) : ""
    input.dataset["visible"] = "false"
    this.renderVisibility(false)
    this.setConfigured(stored)
    // A value the window does not hold cannot be revealed.
    this.element<HTMLButtonElement>("#toggle-token-visibility").hidden = token === null
  }

  clearToken(): void {
    this.setToken(null)
  }

  toggleVisibility(): void {
    if (this.persistedToken === null) return
    const input = this.element<HTMLInputElement>("#settings-hf-token")
    this.visible = nextTokenVisibility(this.visible)
    if (this.persistedToken !== null) {
      input.value = tokenDisplayValue(this.persistedToken, this.visible)
    }
    input.dataset["visible"] = String(this.visible)
    this.renderVisibility(this.visible)
  }

  setConfigured(configured: boolean): void {
    const state = this.element("#token-configured-state")
    state.textContent = configured ? "저장됨" : "저장된 토큰 없음"
    state.dataset["state"] = configured ? "ready" : "pending"
  }

  setBusy(busy: boolean): void {
    this.element<HTMLInputElement>("#settings-hf-token").disabled = busy
    for (const action of ["clear-token", "toggle-token-visibility"]) {
      this.element<HTMLButtonElement>(`[data-action="${action}"]`).disabled = busy
    }
  }

  showMessage(message: string, state: "ready" | "saving" | "error" = "ready"): void {
    const element = this.element("#settings-message")
    element.textContent = message
    element.dataset["state"] = state
  }

  private renderVisibility(visible: boolean): void {
    const button = this.element<HTMLButtonElement>("#toggle-token-visibility")
    button.setAttribute(
      "aria-label",
      visible ? "Hugging Face 토큰 숨기기" : "Hugging Face 토큰 표시",
    )
    button.querySelector<HTMLElement>("i")?.setAttribute(
      "class",
      visible ? "ph ph-eye-slash" : "ph ph-eye",
    )
  }

  private element<T extends HTMLElement = HTMLElement>(selector: string): T {
    return required<T>(this.root, selector)
  }
}
