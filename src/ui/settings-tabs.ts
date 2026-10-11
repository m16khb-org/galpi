import { required } from "./dom"

export type SettingsTab = "engine" | "meeting" | "assistant" | "drive"

const TAB_SELECTOR = '[role="tab"][data-settings-tab]'

/**
 * The settings sheet's tab list, following the APG tabs pattern with
 * automatic activation: an arrow key moves focus and shows that tab's panel.
 *
 * Unselected panels are `hidden`, so their fields stay in the DOM and keep
 * autosaving. A contextual "open settings" link names its tab with
 * `data-settings-tab`; the gear button reopens the tab last shown.
 */
export class SettingsTabsView {
  private readonly root: HTMLElement

  constructor(root: HTMLElement) {
    this.root = root
    for (const tab of this.tabs()) {
      tab.addEventListener("click", () => this.select(tabName(tab)))
      tab.addEventListener("keydown", (event) => this.onKeydown(event, tab))
    }
    // Registered before the controller's open handler, so the sheet opens on the asked tab.
    for (const link of root.querySelectorAll<HTMLElement>(
      '[data-action="open-settings"][data-settings-tab]',
    )) {
      link.addEventListener("click", () => this.select(tabName(link)))
    }
  }

  selected(): SettingsTab {
    const tab = this.tabs().find((candidate) => candidate.getAttribute("aria-selected") === "true")
    return tab === undefined ? "engine" : tabName(tab)
  }

  select(name: SettingsTab, focus = false): void {
    for (const tab of this.tabs()) {
      const active = tabName(tab) === name
      tab.setAttribute("aria-selected", String(active))
      tab.tabIndex = active ? 0 : -1
      const panel = required(this.root, `#${tab.getAttribute("aria-controls")}`)
      panel.hidden = !active
      if (active && focus) tab.focus()
    }
    // A new tab starts at its top rather than at the previous tab's scroll.
    required(this.root, ".settings-body").scrollTop = 0
  }

  private onKeydown(event: KeyboardEvent, tab: HTMLElement): void {
    const tabs = this.tabs()
    const index = tabs.indexOf(tab)
    const target =
      event.key === "ArrowRight"
        ? tabs[(index + 1) % tabs.length]
        : event.key === "ArrowLeft"
          ? tabs[(index - 1 + tabs.length) % tabs.length]
          : event.key === "Home"
            ? tabs[0]
            : event.key === "End"
              ? tabs.at(-1)
              : undefined
    if (target === undefined) return
    event.preventDefault()
    this.select(tabName(target), true)
  }

  private tabs(): HTMLElement[] {
    return [...this.root.querySelectorAll<HTMLElement>(TAB_SELECTOR)]
  }
}

function tabName(element: HTMLElement): SettingsTab {
  const name = element.dataset["settingsTab"]
  if (name === "meeting" || name === "assistant" || name === "drive") return name
  return "engine"
}
