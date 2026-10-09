import { required } from "./dom"

/** The settings sheet as a modal: open/close, focus handling and the status message. */
export class SettingsDialogView {
  private readonly root: HTMLElement
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

  showMessage(message: string, state: "ready" | "saving" | "error" = "ready"): void {
    const element = this.element("#settings-message")
    element.textContent = message
    element.dataset["state"] = state
  }

  private element<T extends HTMLElement = HTMLElement>(selector: string): T {
    return required<T>(this.root, selector)
  }
}
