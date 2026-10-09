import {
  formatAliases,
  type Participant,
  parseAliases,
  usableParticipants,
} from "../domain/participant"
import { required } from "./dom"
import { actionButtonClass } from "./seed"

const ROWS_SELECTOR = "#participant-rows"

/** Roster editor: one saved participant per row, saved with the rest of the settings sheet. */
export class ParticipantSettingsView {
  private readonly root: HTMLElement
  private readonly onChange: () => void

  constructor(root: HTMLElement, onChange: () => void) {
    this.root = root
    this.onChange = onChange
    const rows = this.element(ROWS_SELECTOR)
    rows.addEventListener("input", () => this.refreshCount())
    rows.addEventListener("change", () => this.onChange())
  }

  setRoster(participants: readonly Participant[]): void {
    const rows = this.element(ROWS_SELECTOR)
    rows.replaceChildren()
    for (const participant of participants) {
      rows.append(this.buildRow(participant))
    }
    this.refreshCount()
  }

  roster(): Participant[] {
    return usableParticipants(
      [...this.root.querySelectorAll<HTMLElement>(".participant-row")].map((row) => ({
        id: row.dataset["participantId"] ?? crypto.randomUUID(),
        name: this.field(row, ".participant-name").value,
        team: this.field(row, ".participant-team").value,
        role: this.field(row, ".participant-role").value,
        description: this.field(row, ".participant-description").value,
        aliases: parseAliases(this.field(row, ".participant-aliases").value),
      })),
    )
  }

  addRow(): void {
    const row = this.buildRow({
      id: crypto.randomUUID(),
      name: "",
      team: null,
      role: null,
      description: null,
      aliases: [],
    })
    this.element(ROWS_SELECTOR).append(row)
    this.refreshCount()
    row.querySelector<HTMLInputElement>(".participant-name")?.focus()
  }

  setBusy(busy: boolean): void {
    for (const input of this.root.querySelectorAll<HTMLInputElement>(".participant-row input")) {
      input.disabled = busy
    }
    this.element<HTMLButtonElement>('[data-action="add-participant"]').disabled = busy
  }

  private buildRow(participant: Participant): HTMLElement {
    const document = this.root.ownerDocument
    const row = document.createElement("div")
    row.className = "participant-row"
    row.dataset["participantId"] = participant.id
    const fields = document.createElement("div")
    fields.className = "participant-fields"
    fields.append(
      this.buildInput("participant-name", "이름", participant.name),
      this.buildInput("participant-team", "팀 (선택)", participant.team ?? ""),
      this.buildInput("participant-role", "역할 (선택)", participant.role ?? ""),
      this.buildInput(
        "participant-aliases",
        "별칭 (쉼표로 구분)",
        formatAliases(participant.aliases),
      ),
    )
    const remove = document.createElement("button")
    remove.type = "button"
    remove.className = `participant-remove ${actionButtonClass("ghost", "medium", "iconOnly")}`
    remove.setAttribute("aria-label", `${participant.name || "참석자"} 삭제`)
    remove.innerHTML = '<i class="ph ph-x"></i>'
    remove.addEventListener("click", () => {
      row.remove()
      this.refreshCount()
      this.onChange()
    })
    fields.append(remove)
    const description = document.createElement("textarea")
    description.className = "participant-description"
    description.rows = 2
    description.placeholder = "담당 업무 등 설명 (선택)"
    description.value = participant.description ?? ""
    description.autocomplete = "off"
    description.spellcheck = false
    description.setAttribute("aria-label", `${participant.name || "참석자"} 설명`)
    row.append(fields, description)
    return row
  }

  private buildInput(className: string, placeholder: string, value: string): HTMLInputElement {
    const input = this.root.ownerDocument.createElement("input")
    input.type = "text"
    input.className = className
    input.placeholder = placeholder
    input.value = value
    input.autocomplete = "off"
    input.spellcheck = false
    input.setAttribute("aria-label", placeholder)
    return input
  }

  private refreshCount(): void {
    const named = this.roster().length
    const state = this.element("#participants-count-state")
    state.textContent = named === 0 ? "비어 있음" : `${named}명`
    state.dataset["state"] = named === 0 ? "pending" : "ready"
    this.element("#participant-rows-empty").hidden =
      this.root.querySelectorAll(".participant-row").length > 0
  }

  private field<T extends HTMLInputElement | HTMLTextAreaElement>(
    row: HTMLElement,
    selector: string,
  ): T {
    const input = row.querySelector<T>(selector)
    if (input === null) throw new Error(`참석자 입력란이 없습니다: ${selector}`)
    return input
  }

  private element<T extends HTMLElement = HTMLElement>(selector: string): T {
    return required<T>(this.root, selector)
  }
}
