import { type GlossaryEntry, usableGlossary } from "../domain/glossary"
import { required } from "./dom"
import { actionButtonClass } from "./seed"

const ROWS_SELECTOR = "#glossary-rows"

/** Glossary editor: one saved term per row, applied to every refinement. */
export class GlossarySettingsView {
  private readonly root: HTMLElement
  private readonly onChange: () => void

  constructor(root: HTMLElement, onChange: () => void) {
    this.root = root
    this.onChange = onChange
    const rows = this.element(ROWS_SELECTOR)
    rows.addEventListener("input", () => this.refreshCount())
    rows.addEventListener("change", () => this.onChange())
  }

  setEntries(entries: readonly GlossaryEntry[]): void {
    const rows = this.element(ROWS_SELECTOR)
    rows.replaceChildren()
    for (const entry of entries) {
      rows.append(this.buildRow(entry))
    }
    this.refreshCount()
  }

  entries(): GlossaryEntry[] {
    return usableGlossary(
      [...this.root.querySelectorAll<HTMLElement>(".glossary-row")].map((row) => ({
        id: row.dataset["entryId"] ?? crypto.randomUUID(),
        term: this.field(row, ".glossary-term").value,
        description: this.field(row, ".glossary-description").value,
      })),
    )
  }

  addRow(): void {
    const row = this.buildRow({ id: crypto.randomUUID(), term: "", description: null })
    this.element(ROWS_SELECTOR).append(row)
    this.refreshCount()
    row.querySelector<HTMLInputElement>(".glossary-term")?.focus()
  }

  private buildRow(entry: GlossaryEntry): HTMLElement {
    const document = this.root.ownerDocument
    const row = document.createElement("div")
    row.className = "glossary-row"
    row.dataset["entryId"] = entry.id
    const term = document.createElement("input")
    term.type = "text"
    term.className = "glossary-term"
    term.placeholder = "용어"
    term.value = entry.term
    term.autocomplete = "off"
    term.spellcheck = false
    term.setAttribute("aria-label", "용어")
    const description = document.createElement("input")
    description.type = "text"
    description.className = "glossary-description"
    description.placeholder = "뜻 / 설명 (선택)"
    description.value = entry.description ?? ""
    description.autocomplete = "off"
    description.spellcheck = false
    description.setAttribute("aria-label", `${entry.term || "용어"} 설명`)
    const remove = document.createElement("button")
    remove.type = "button"
    remove.className = `glossary-remove ${actionButtonClass("ghost", "medium", "iconOnly")}`
    remove.setAttribute("aria-label", `${entry.term || "용어"} 삭제`)
    remove.innerHTML = '<i class="ph ph-x"></i>'
    remove.addEventListener("click", () => {
      row.remove()
      this.refreshCount()
      this.onChange()
    })
    row.append(term, description, remove)
    return row
  }

  private refreshCount(): void {
    const named = this.entries().length
    const state = this.element("#glossary-count-state")
    state.textContent = named === 0 ? "비어 있음" : `${named}개`
    state.dataset["state"] = named === 0 ? "pending" : "ready"
    this.element("#glossary-rows-empty").hidden =
      this.root.querySelectorAll(".glossary-row").length > 0
  }

  private field<T extends HTMLInputElement | HTMLTextAreaElement>(
    row: HTMLElement,
    selector: string,
  ): T {
    const input = row.querySelector<T>(selector)
    if (input === null) throw new Error(`단어집 입력란이 없습니다: ${selector}`)
    return input
  }

  private element<T extends HTMLElement = HTMLElement>(selector: string): T {
    return required<T>(this.root, selector)
  }
}
