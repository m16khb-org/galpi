import type { AssistantSettings } from "../domain/job"
import { hideClearUnlessStored, nextTokenVisibility, tokenDisplayValue } from "./token-settings"
import { required } from "./dom"

const KEY_SELECTOR = "#settings-assistant-key"
const MODEL_SELECTOR = "#settings-assistant-model"
const BASE_URL_SELECTOR = "#settings-assistant-base-url"
const EFFORT_SELECTOR = "#settings-assistant-effort"
const BACKGROUND_SELECTOR = "#settings-assistant-background"

export const DEFAULT_ASSISTANT_MODEL = "glm-5.3-flash"

/** The assistant sheet owns credentials and context; the roster and glossary are edited beside it. */
export type AssistantCredentials = Omit<AssistantSettings, "participants" | "glossary">

export class AssistantSettingsView {
  private readonly root: HTMLElement
  /// A key this window has seen — only ever one the user just typed.
  private persistedKey: string | null = null
  /// Whether the host holds a key, whose value never crosses the IPC border.
  private stored = false
  private visible = false

  constructor(root: HTMLElement) {
    this.root = root
  }

  settings(): AssistantCredentials {
    const background = this.element<HTMLTextAreaElement>(BACKGROUND_SELECTOR).value.trim()
    const baseUrl = this.element<HTMLInputElement>(BASE_URL_SELECTOR).value.trim()
    const model = this.element<HTMLInputElement>(MODEL_SELECTOR).value.trim()
    return {
      // The key travels on its own; this only reports whether one exists.
      apiKeyStored: this.stored || this.pendingKey() !== null,
      model: model.length > 0 ? model : DEFAULT_ASSISTANT_MODEL,
      baseUrl: baseUrl.length > 0 ? baseUrl : null,
      reasoningEffort: this.element<HTMLSelectElement>(EFFORT_SELECTOR).value || null,
      background: background.length > 0 ? background : null,
    }
  }

  /// A key the user has typed and not yet saved.
  ///
  /// Null once a key is stored: the host keeps the value and the field shows
  /// only a mask, so there is nothing here to save again. Autosave fires on
  /// every edit anywhere in the sheet, and saving a key it does not hold is
  /// what erased the stored one.
  pendingKey(): string | null {
    if (this.stored) return null
    const value = this.element<HTMLInputElement>(KEY_SELECTOR).value.trim()
    return value.length > 0 ? value : null
  }

  setSettings(settings: AssistantCredentials): void {
    const model = settings.model ?? DEFAULT_ASSISTANT_MODEL
    this.element<HTMLInputElement>(MODEL_SELECTOR).value = model
    this.element<HTMLInputElement>(BASE_URL_SELECTOR).value = settings.baseUrl ?? ""
    this.element<HTMLSelectElement>(EFFORT_SELECTOR).value =
      settings.reasoningEffort ?? (model.toLowerCase().startsWith("glm") ? "max" : "")
    this.element<HTMLTextAreaElement>(BACKGROUND_SELECTOR).value = settings.background ?? ""
    this.setStored(settings.apiKeyStored)
  }

  /// Hold on to a key the user just typed, so the eye toggle can show it back.
  setKey(apiKey: string | null): void {
    this.persistedKey = apiKey
    this.render(apiKey !== null, apiKey)
  }

  /// Show that a key is saved without knowing what it says.
  ///
  /// Reading the value is a keychain access, and macOS asks the user about
  /// each one, so opening settings must not need it. Changing a stored key
  /// means clearing it and entering the new one.
  setStored(stored: boolean): void {
    this.persistedKey = null
    this.render(stored, null)
  }

  clearKey(): void {
    this.setKey(null)
  }

  private render(stored: boolean, apiKey: string | null): void {
    const input = this.element<HTMLInputElement>(KEY_SELECTOR)
    this.stored = stored
    this.visible = false
    input.value = stored ? tokenDisplayValue(apiKey ?? "", false) : ""
    input.readOnly = stored
    input.dataset["visible"] = "false"
    this.renderVisibility(false)
    this.setConfigured(stored)
    // A value the window does not hold cannot be revealed.
    this.element<HTMLButtonElement>("#toggle-assistant-visibility").hidden = apiKey === null
  }

  toggleVisibility(): void {
    if (this.persistedKey === null) return
    const input = this.element<HTMLInputElement>(KEY_SELECTOR)
    this.visible = nextTokenVisibility(this.visible)
    input.value = tokenDisplayValue(this.persistedKey, this.visible)
    input.dataset["visible"] = String(this.visible)
    this.renderVisibility(this.visible)
  }

  setConfigured(configured: boolean): void {
    const state = this.element("#assistant-configured-state")
    state.textContent = configured ? "API 키 저장됨" : "API 키 없음"
    state.dataset["state"] = configured ? "ready" : "pending"
    hideClearUnlessStored(this.element("#clear-assistant-key-button"), configured, KEY_SELECTOR)
  }

  setBusy(busy: boolean): void {
    this.element<HTMLButtonElement>('[data-action="clear-assistant-key"]').disabled = busy
    this.element<HTMLInputElement>(KEY_SELECTOR).disabled = busy
    this.element<HTMLInputElement>(MODEL_SELECTOR).disabled = busy
    this.element<HTMLInputElement>(BASE_URL_SELECTOR).disabled = busy
    this.element<HTMLSelectElement>(EFFORT_SELECTOR).disabled = busy
    this.element<HTMLTextAreaElement>(BACKGROUND_SELECTOR).disabled = busy
    this.element<HTMLButtonElement>('[data-action="toggle-assistant-visibility"]').disabled = busy
  }

  private renderVisibility(visible: boolean): void {
    const button = this.element<HTMLButtonElement>("#toggle-assistant-visibility")
    button.setAttribute("aria-label", visible ? "API 키 숨기기" : "API 키 표시")
    button
      .querySelector<HTMLElement>("i")
      ?.setAttribute("class", visible ? "ph ph-eye-slash" : "ph ph-eye")
  }

  private element<T extends HTMLElement = HTMLElement>(selector: string): T {
    return required<T>(this.root, selector)
  }
}
