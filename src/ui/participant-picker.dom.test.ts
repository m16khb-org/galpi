import { beforeEach, describe, expect, test } from "bun:test"
import { Window } from "happy-dom"

import type { Participant } from "../domain/participant"
import { AppView } from "./app-view"
import { ParticipantPickerView } from "./participant-picker"

function createWindow(): { window: Window; root: HTMLElement } {
  const window = new Window()
  const root = window.document.createElement("div") as unknown as HTMLElement
  window.document.body.appendChild(root as unknown as never)
  return { window, root }
}

const roster: Participant[] = [
  {
    id: "hb",
    name: "하빈",
    team: null,
    role: "팀리더",
    description: "녹음 파이프라인 담당",
    aliases: ["프로님"],
  },
  { id: "jw", name: "지우", team: null, role: null, description: null, aliases: [] },
]

function at<T>(items: readonly T[], index: number): T {
  const item = items[index]
  if (item === undefined) throw new Error(`인덱스 ${index} 요소가 없습니다`)
  return item
}

function query<T extends HTMLElement>(root: HTMLElement, selector: string): T {
  const element = root.querySelector<T>(selector)
  if (element === null) throw new Error(`화면 요소가 없습니다: ${selector}`)
  return element
}

function fireChange(window: Window, input: HTMLInputElement): void {
  input.dispatchEvent(new window.Event("change") as unknown as Event)
}

describe("ParticipantPickerView (real DOM)", () => {
  let window: Window
  let root: HTMLElement
  let counts: number[]
  let view: ParticipantPickerView

  beforeEach(() => {
    ;({ window, root } = createWindow())
    counts = []
    const app = new AppView(root)
    view = new ParticipantPickerView(root, (count) => {
      counts.push(count)
      app.applyAttendeeCount(count)
    })
  })

  test("renders one chip per roster entry and an empty-roster hint", () => {
    // When
    view.setRoster(roster)

    // Then
    expect(root.querySelectorAll(".participant-chip").length).toBe(2)
    expect(root.querySelector(".participant-chip span")?.textContent).toBe("하빈 · 팀리더")
    expect(root.querySelector(".participant-chip")?.getAttribute("title")).toBe(
      "녹음 파이프라인 담당",
    )

    // When
    view.setRoster([])

    // Then
    expect((root.querySelector("#attendee-empty") as HTMLElement).hidden).toBe(false)
    expect(root.querySelector("#attendee-count")?.textContent).toBe("명부 없음")
  })

  test("selecting chips reports the count and fills the speaker hint", () => {
    // Given
    view.setRoster(roster)

    // When
    const chips = [...root.querySelectorAll<HTMLInputElement>(".participant-chip input")]
    const first = at(chips, 0)
    first.checked = true
    fireChange(window, first)
    const second = at(chips, 1)
    second.checked = true
    fireChange(window, second)

    // Then
    expect(counts).toEqual([1, 2])
    expect(root.querySelector("#attendee-count")?.textContent).toBe("2명 선택")
    expect(
      (root.querySelector('input[name="speaker-mode"][value="range"]') as HTMLInputElement).checked,
    ).toBe(true)
    expect((root.querySelector("#min-speakers") as HTMLInputElement).value).toBe("1")
    expect((root.querySelector("#max-speakers") as HTMLInputElement).value).toBe("3")
    expect(root.querySelector("#speaker-hint-note")?.textContent).toContain("1~3명으로 정했습니다")
  })

  test("clearing the selection returns the hint to auto", () => {
    // Given
    view.setRoster(roster)
    const chip = query<HTMLInputElement>(root, ".participant-chip input")
    chip.checked = true
    fireChange(window, chip)

    // When
    view.clear()

    // Then
    expect(view.selectedIds()).toEqual([])
    expect((root.querySelector("#attendee-clear") as HTMLElement).hidden).toBe(true)
    expect(
      (root.querySelector('input[name="speaker-mode"][value="auto"]') as HTMLInputElement).checked,
    ).toBe(true)
  })

  test("a selection survives roster reloads and drops deleted members", () => {
    // Given
    view.setRoster(roster)
    for (const chip of root.querySelectorAll<HTMLInputElement>(".participant-chip input")) {
      chip.checked = true
      fireChange(window, chip)
    }

    // When: 지우 leaves the roster before the next meeting
    view.setRoster([at(roster, 0)])

    // Then
    expect(view.selectedIds()).toEqual(["hb"])
  })
})
