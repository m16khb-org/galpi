import { beforeEach, describe, expect, test } from "bun:test"
import { Window } from "happy-dom"

import type { EnvironmentStatus, TranscriptionResult } from "../domain/job"
import { initialJobState } from "../application/job-machine"
import { AppView } from "./app-view"

const styles = await Bun.file(new URL("../styles.css", import.meta.url)).text()

function createWindow(): Window {
  const window = new Window()
  const sheet = window.document.createElement("style")
  sheet.textContent = styles
  window.document.head.appendChild(sheet)
  return window
}

function createView(): { view: AppView; root: HTMLElement; window: Window } {
  const window = createWindow()
  const root = window.document.createElement("div") as unknown as HTMLElement
  window.document.body.appendChild(root as unknown as never)
  return { view: new AppView(root), root, window }
}

function environment(ready: boolean): EnvironmentStatus {
  return {
    enginePreset: "qwen3",
    engineReady: ready,
    modelsReady: ready,
    ffmpegReady: ready,
    qwen3Ready: ready,
    whisperxReady: false,
    dataDirectory: "/tmp/galpi",
    defaultOutputDirectory: "/tmp/galpi/out",
    engineVersion: "Qwen3-ASR-1.7B · 1",
  }
}

const result: TranscriptionResult = {
  jobId: "id",
  srt: "/tmp/out/meeting.srt",
  txt: "/tmp/out/meeting_화자별.txt",
  checkpoint: "/tmp/out/meeting.aligned.v2.json",
  outputDirectory: "/tmp/out",
  segments: 12,
  filtered: 1,
}

function hidden(root: HTMLElement, selector: string): boolean {
  return (root.querySelector(selector) as HTMLElement).hidden === true
}

function railState(root: HTMLElement, selector: string): string {
  return (root.querySelector(selector) as HTMLElement).dataset["state"] ?? ""
}

function ariaCurrent(root: HTMLElement, selector: string): string | null {
  return root.querySelector(selector)?.getAttribute("aria-current") ?? null
}

describe("AppView stage flow (real DOM)", () => {
  let view: AppView
  let root: HTMLElement

  beforeEach(() => {
    ;({ view, root } = createView())
  })

  test("keeps the preparation panel visible while the environment is not ready", () => {
    // When
    view.setEnvironment(environment(false))

    // Then: preparation is a pre-gate panel, never a rail stage
    expect(hidden(root, "#setup-panel")).toBe(false)
    expect(root.querySelectorAll(".step-list li").length).toBe(3)
    expect(root.querySelector("#step-transcribe span")?.textContent).toBe("01")
    expect(railState(root, "#step-transcribe")).toBe("pending")
    expect(railState(root, "#step-results")).toBe("pending")
    expect(railState(root, "#step-augment")).toBe("pending")
    expect(ariaCurrent(root, "#step-transcribe")).toBeNull()
    expect(ariaCurrent(root, "#step-results")).toBeNull()
    expect(ariaCurrent(root, "#step-augment")).toBeNull()
  })

  test("hides the preparation panel once the environment is ready", () => {
    // When
    view.setEnvironment(environment(true))

    // Then
    expect(hidden(root, "#setup-panel")).toBe(true)
    expect(railState(root, "#step-transcribe")).toBe("current")
    expect(ariaCurrent(root, "#step-transcribe")).toBe("step")
    expect(ariaCurrent(root, "#step-results")).toBeNull()
  })

  test("keeps the preparation panel visible right after preparing in this session", () => {
    // Given
    view.setEnvironment(environment(false))
    view.setBusy("setup")

    // When
    view.setEnvironment(environment(true))
    view.setBusy(null)

    // Then
    expect(hidden(root, "#setup-panel")).toBe(false)
  })

  test("hides the preparation panel once a prepared user starts transcription", () => {
    // Given
    view.setEnvironment(environment(false))
    view.setBusy("setup")
    view.setEnvironment(environment(true))

    // When
    view.setBusy("transcription")

    // Then
    expect(hidden(root, "#setup-panel")).toBe(true)
  })

  test("marks stage 01 complete and stage 02 current when results render", () => {
    // Given
    view.setEnvironment(environment(true))

    // When
    view.renderResult(result)

    // Then
    expect(railState(root, "#step-transcribe")).toBe("complete")
    expect(railState(root, "#step-results")).toBe("current")
    expect(railState(root, "#step-augment")).toBe("pending")
    expect(ariaCurrent(root, "#step-transcribe")).toBeNull()
    expect(ariaCurrent(root, "#step-results")).toBe("step")
    expect(hidden(root, "#results-panel")).toBe(false)
    expect(hidden(root, "#augment-panel")).toBe(false)
    expect(hidden(root, "#augment-waiting")).toBe(true)
    expect(hidden(root, "#result-minutes-row")).toBe(true)
  })

  test("hands the finished transcript to the augment file picker", () => {
    // Given
    view.setEnvironment(environment(true))

    // When
    view.renderResult(result)

    // Then: augmentation starts from the transcript the run just published
    const picker = root.querySelector("#transcript-selection") as HTMLElement
    expect(root.querySelector("#transcript-path")?.textContent).toBe(result.txt)
    expect(picker.dataset["selected"]).toBe("true")
  })

  test("refinement progress renders inside the augment panel, not the top job panel", () => {
    // Given: a rendered result with a saved key
    view.setEnvironment(environment(true))
    view.setAssistantKeyReady(true)
    view.renderResult(result)

    // When: augmentation starts
    view.setBusy("refinement")
    view.renderJob({
      ...initialJobState,
      status: "running",
      phase: "refining",
      percent: 55,
      message: "glm-5.3 모델로 회의록을 작성하는 중입니다. 12,345자",
    })

    // Then: the augment panel carries its own progress block
    expect(hidden(root, "#augment-progress")).toBe(false)
    expect(root.querySelector("#augment-job-percent")?.textContent).toBe("55%")
    expect(root.querySelector("#augment-job-message")?.textContent).toContain("12,345자")
    expect(root.querySelector("#augment-job-progress")?.getAttribute("aria-valuenow")).toBe(
      "55",
    )
    expect(hidden(root, "#augment-cancel-button")).toBe(false)
    // The transcription card stays folded into its summary during refinement
    const jobPanel = root.querySelector("#job-panel") as HTMLElement
    expect(jobPanel.dataset["status"]).toBe("completed")
    expect(hidden(root, "#cancel-button")).toBe(true)
  })

  test("refinement errors surface in the augment panel and survive the busy reset", () => {
    // Given
    view.setEnvironment(environment(true))
    view.setAssistantKeyReady(true)
    view.renderResult(result)
    view.setBusy("refinement")

    // When: the refinement fails
    view.renderJob({
      ...initialJobState,
      status: "failed",
      phase: "refining",
      error: "assistant request failed (401)",
    })
    view.setBusy(null)

    // Then
    expect(hidden(root, "#augment-progress")).toBe(false)
    expect(root.querySelector("#augment-error-message")?.textContent).toContain("401")
    expect(hidden(root, "#augment-error-message")).toBe(false)
    expect(hidden(root, "#augment-cancel-button")).toBe(true)
  })

  test("completion hands the augment block over to the minutes row", () => {
    // Given: refinement running in-panel
    view.setEnvironment(environment(true))
    view.setAssistantKeyReady(true)
    view.renderResult(result)
    view.setBusy("refinement")

    // When
    view.renderMinutes("/tmp/out/meeting_회의록.md")
    view.setBusy(null)

    // Then
    expect(hidden(root, "#augment-progress")).toBe(true)
    expect(hidden(root, "#result-minutes-row")).toBe(false)
    expect(railState(root, "#step-augment")).toBe("complete")
  })

  test("marks stage 03 complete when minutes render", () => {
    // Given
    view.setEnvironment(environment(true))
    view.renderResult(result)

    // When
    view.renderMinutes("/tmp/out/meeting_회의록.md")

    // Then
    expect(railState(root, "#step-augment")).toBe("complete")
    expect(hidden(root, "#result-minutes-row")).toBe(false)
    expect(ariaCurrent(root, "#step-results")).toBe("step")
    expect(ariaCurrent(root, "#step-augment")).toBeNull()
  })

  test("shows the waiting hint before a transcription exists", () => {
    // Given / When
    view.setEnvironment(environment(true))

    // Then
    expect(hidden(root, "#augment-waiting")).toBe(false)
  })

  test("the augment button requires a result, busyness, and a saved key", () => {
    // Given: no key saved yet
    view.setEnvironment(environment(true))

    // Then
    expect((root.querySelector("#refine-button") as HTMLButtonElement).disabled).toBe(true)

    // When: a key arrives but there is no result
    view.setAssistantKeyReady(true)

    // Then: still disabled without a transcription
    expect((root.querySelector("#refine-button") as HTMLButtonElement).disabled).toBe(true)

    // When: a result renders
    view.renderResult(result)

    // Then: enabled with key + result + idle
    expect((root.querySelector("#refine-button") as HTMLButtonElement).disabled).toBe(false)
  })

  test("the augment key hint disappears once a key is saved", () => {
    // Given
    view.setEnvironment(environment(true))

    // When
    view.setAssistantKeyReady(true)

    // Then
    expect(hidden(root, "#augment-key-hint")).toBe(true)
  })
})

function text(root: HTMLElement, selector: string): string {
  return root.querySelector(selector)?.textContent ?? ""
}

function stepLabel(root: HTMLElement, step: string): string {
  return text(root, `${step} [data-step-state]`)
}

function isPrimary(root: HTMLElement, selector: string): boolean {
  return root.querySelector(selector)?.classList.contains("primary-button") === true
}

describe("AppView state presentation (real DOM)", () => {
  let view: AppView
  let root: HTMLElement

  beforeEach(() => {
    ;({ view, root } = createView())
  })

  test("rail steps name their state in text, not color alone", () => {
    // Given
    view.setEnvironment(environment(true))

    // Then
    expect(stepLabel(root, "#step-transcribe")).toBe("현재 단계")
    expect(stepLabel(root, "#step-results")).toBe("대기")

    // When
    view.renderResult(result)

    // Then
    expect(stepLabel(root, "#step-transcribe")).toBe("완료")
    expect(stepLabel(root, "#step-results")).toBe("현재 단계")
    expect(stepLabel(root, "#step-augment")).toBe("대기")
  })

  test("the transcription card title follows the job outcome", () => {
    // Given
    view.setEnvironment(environment(true))
    view.setBusy("transcription")

    for (const [status, title] of [
      ["running", "회의를 전사하고 있습니다"],
      ["completed", "전사를 마쳤습니다"],
      ["failed", "전사하지 못했습니다"],
      ["cancelled", "전사를 취소했습니다"],
    ] as const) {
      // When
      view.renderJob({ ...initialJobState, status, phase: "aligning" })

      // Then
      expect(text(root, "#job-title")).toBe(title)
      expect((root.querySelector("#job-panel") as HTMLElement).dataset["status"]).toBe(status)
    }
  })

  test("a finished transcription summarizes itself and steps the start button down", () => {
    // Given
    view.setEnvironment(environment(true))
    view.setAudio("/tmp/in/2026-10-03 주간 회의.m4a")
    expect(isPrimary(root, "#start-button")).toBe(true)

    // When
    view.setBusy("transcription")
    view.renderJob({ ...initialJobState, status: "completed", phase: "writing", percent: 100 })
    view.renderResult(result)
    view.setBusy(null)

    // Then
    expect(text(root, "#job-summary-text")).toBe("전사 완료 · 12개 발화 보존 · 1개 환각 제거")
    expect(isPrimary(root, "#start-button")).toBe(false)
    expect(text(root, "#start-label")).toBe("새 전사 시작")

    // When: a new recording or file arrives, starting is the main task again
    view.setAudio("/tmp/in/다음 회의.m4a")

    // Then
    expect(isPrimary(root, "#start-button")).toBe(true)
    expect(text(root, "#start-label")).toBe("전사 시작")
  })

  test("the top bar names the current task and its status", () => {
    // When: the environment still needs preparing
    view.setEnvironment(environment(false))

    // Then
    expect(text(root, "#task-title")).toBe("로컬 엔진을 준비해 주세요")

    // When: ready and idle
    view.setEnvironment(environment(true))

    // Then
    expect(text(root, "#task-title")).toBe("새 회의 전사")
    expect(text(root, "#task-status")).toBe("녹음하거나 오디오 파일을 선택하세요")

    // When: a file is chosen, the meeting takes over the heading
    view.setAudio("/tmp/in/2026-10-03 주간 회의.m4a")

    // Then
    expect(text(root, "#task-title")).toBe("2026-10-03 주간 회의")
    expect(text(root, "#task-status")).toBe("전사 준비됨")

    // When
    view.setBusy("transcription")

    // Then
    expect(text(root, "#task-status")).toBe("전사 중")

    // When
    view.renderJob({ ...initialJobState, status: "running", phase: "aligning", percent: 46 })

    // Then
    expect(text(root, "#task-status")).toBe("전사 중 · 정렬 단계")

    // When
    view.renderResult(result)
    view.setBusy(null)

    // Then
    expect(text(root, "#task-status")).toBe("전사 완료 · 회의록 작성 대기")

    // When
    view.renderMinutes("/tmp/out/meeting_회의록.md")

    // Then
    expect(text(root, "#task-status")).toBe("회의록 완료")
  })

  test("finished minutes make opening them the primary action", () => {
    // Given
    view.setEnvironment(environment(true))
    view.setAssistantKeyReady(true)
    view.renderResult(result)
    expect(hidden(root, "#open-minutes-button")).toBe(true)
    expect(isPrimary(root, "#refine-button")).toBe(true)

    // When
    view.renderMinutes("/tmp/out/meeting_회의록.md")

    // Then
    expect(hidden(root, "#open-minutes-button")).toBe(false)
    expect(isPrimary(root, "#refine-button")).toBe(false)
    expect(text(root, "#refine-label")).toBe("다시 증강")
  })

  test("a new result clears the previous meeting's minutes", () => {
    // Given
    view.setEnvironment(environment(true))
    view.setAssistantKeyReady(true)
    view.renderResult(result)
    view.renderMinutes("/tmp/out/meeting_회의록.md")

    // When
    view.renderResult({ ...result, jobId: "next" })

    // Then
    expect(railState(root, "#step-augment")).toBe("pending")
    expect(hidden(root, "#result-minutes-row")).toBe(true)
    expect(hidden(root, "#open-minutes-button")).toBe(true)
    expect(isPrimary(root, "#refine-button")).toBe(true)
    expect(text(root, "#refine-label")).toBe("AI 증강 실행")
  })

  test("the augment picker presents the transcript it will use", () => {
    // Given
    view.setEnvironment(environment(true))
    expect(text(root, "#transcript-title")).toBe("전사문 파일 가져오기")

    // When
    view.renderResult(result)

    // Then
    expect(text(root, "#transcript-title")).toBe("대상 전사문")
    expect(root.querySelector("#augment-panel .choice-divider")).toBeNull()
  })
})

describe("AppView SEED action-button recipe (real DOM)", () => {
  let view: AppView
  let root: HTMLElement

  beforeEach(() => {
    ;({ view, root } = createView())
  })

  function variantOf(selector: string): string[] {
    return [...(root.querySelector(selector)?.classList ?? [])].filter((name) =>
      name.startsWith("seed-action-button--variant_"),
    )
  }

  test("the start button's recipe variant follows its primary emphasis", () => {
    // Given
    view.setEnvironment(environment(true))
    view.setAudio("/tmp/in/2026-10-03 주간 회의.m4a")
    expect(variantOf("#start-button")).toEqual(["seed-action-button--variant_brandSolid"])

    // When: a finished result steps the start button down
    view.renderResult(result)

    // Then
    expect(isPrimary(root, "#start-button")).toBe(false)
    expect(variantOf("#start-button")).toEqual(["seed-action-button--variant_neutralOutline"])

    // When
    view.setAudio("/tmp/in/다음 회의.m4a")

    // Then
    expect(variantOf("#start-button")).toEqual(["seed-action-button--variant_brandSolid"])
  })

  test("finished minutes step the refine button down to the outline variant", () => {
    // Given
    view.setEnvironment(environment(true))
    view.setAssistantKeyReady(true)
    view.renderResult(result)
    expect(variantOf("#refine-button")).toEqual(["seed-action-button--variant_brandSolid"])

    // When
    view.renderMinutes("/tmp/out/meeting_회의록.md")

    // Then
    expect(variantOf("#refine-button")).toEqual(["seed-action-button--variant_neutralOutline"])
  })

  test("roster and glossary rows build icon-only ghost remove buttons", () => {
    // When
    view.participantSettings.addRow()
    view.glossarySettings.addRow()

    // Then
    for (const selector of [".participant-remove", ".glossary-remove"]) {
      const classes = root.querySelector(selector)?.classList
      expect(classes?.contains("seed-action-button--variant_ghost")).toBe(true)
      expect(classes?.contains("seed-action-button--size_medium-layout_iconOnly")).toBe(true)
    }
  })
})
