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
  /// Null once a token is stored: the host keeps the value in the keychain and
  /// the field shows only a mask, so there is nothing here to save again.
  pendingToken(): string | null {
    if (this.stored) return null
    const value = this.element<HTMLInputElement>("#settings-hf-token").value.trim()
    return value.length > 0 ? value : null
  }

  setToken(token: string | null): void {
    this.persistedToken = token
    this.render(token !== null, token)
  }

  /// Show that a token is saved without knowing what it says.
  ///
  /// Reading the value is a keychain access, and macOS asks the user about
  /// each one, so opening settings must not need it. Changing a stored token
  /// means clearing it and entering the new one.
  setStored(stored: boolean): void {
    this.persistedToken = null
    this.render(stored, null)
  }

  private render(stored: boolean, token: string | null): void {
    const input = this.element<HTMLInputElement>("#settings-hf-token")
    this.stored = stored
    this.visible = false
    input.value = stored ? tokenDisplayValue(token ?? "", false) : ""
    input.readOnly = stored
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
    hideClearUnlessStored(this.element("#clear-token-button"), configured, "#settings-hf-token")
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

/**
 * Clearing is offered only for a credential that is stored. When the clear
 * itself hides the button, focus lands on the now-empty field instead of the page.
 */
export function hideClearUnlessStored(button: HTMLElement, stored: boolean, field: string): void {
  const hadFocus = button.ownerDocument.activeElement === button
  button.hidden = !stored
  if (hadFocus && !stored) button.ownerDocument.querySelector<HTMLElement>(field)?.focus()
}
