import { beforeEach, describe, expect, test } from "bun:test"
import { Window } from "happy-dom"

import { appTemplate } from "./app-template"
import { SettingsDialogView } from "./settings-dialog"

function createView(): { view: SettingsDialogView; body: HTMLElement } {
  const window = new Window()
  const root = window.document.createElement("div") as unknown as HTMLElement
  root.innerHTML = appTemplate
  window.document.body.appendChild(root as unknown as never)
  const view = new SettingsDialogView(root)
  return { view, body: root.closest("body") as HTMLElement }
}

describe("SettingsDialogView (real DOM)", () => {
  let view: SettingsDialogView
  let body: HTMLElement

  beforeEach(() => {
    ;({ view, body } = createView())
  })

  test("moves focus into the dialog when it opens", () => {
    // Given: the user opened settings from the topbar gear button
    const trigger = body.querySelector(".settings-button") as HTMLElement
    trigger.focus()

    // When: the dialog opens
    view.show()

    // Then: focus lands on the close button inside the modal — it stays
    // enabled while the fields are disabled during load (BUG-001)
    const close = body.querySelector(".settings-close-button") as HTMLElement
    expect(body.ownerDocument.activeElement).toBe(close)
    expect(body.ownerDocument.activeElement?.closest("#settings-dialog")).not.toBeNull()
  })

  test("returns focus to the invoking trigger when the dialog closes", () => {
    // Given: the user opened settings from the topbar gear button
    const trigger = body.querySelector(".settings-button") as HTMLElement
    trigger.focus()
    view.show()
    ;(body.querySelector("#settings-assistant-model") as HTMLElement).focus()

    // When: the dialog closes (button, Escape, and programmatic paths share close())
    view.close()

    // Then: the keyboard user lands back on the invoker, not <body>
    expect(body.ownerDocument.activeElement).toBe(trigger)
  })

  test("takes the page behind the dialog out of the tab order while it is open", () => {
    // Given / When
    view.show()

    // Then: every sibling of the dialog is inert, and open again after close
    const dialog = body.querySelector("#settings-dialog") as HTMLElement
    const siblings = [...(dialog.parentElement?.children ?? [])].filter(
      (sibling) => sibling !== dialog,
    ) as HTMLElement[]
    expect(siblings.length).toBeGreaterThan(0)
    expect(siblings.every((sibling) => sibling.inert)).toBeTrue()
    view.close()
    expect(siblings.some((sibling) => sibling.inert)).toBeFalse()
  })

  test("shows a status message with its state", () => {
    // When
    view.showMessage("저장하지 못했습니다.", "error")

    // Then
    const message = body.querySelector("#settings-message") as HTMLElement
    expect(message.textContent).toBe("저장하지 못했습니다.")
    expect(message.dataset["state"]).toBe("error")
  })
})
