import { expect, test } from "bun:test"

test("keeps hidden components out of layout when component styles set display", async () => {
  // Given
  const stylesheet = await Bun.file(new URL("./styles.css", import.meta.url)).text()

  // When
  const hiddenRule = stylesheet.match(/html\s+\[hidden\]\s*\{([^}]*)\}/u)

  // Then
  expect(hiddenRule?.at(1)).toMatch(/display:\s*none\s*;/u)
})

test("panel headings keep their own type instead of the description's", async () => {
  // Given
  const stylesheet = await Bun.file(new URL("./styles.css", import.meta.url)).text()

  // When: every rule whose selector list names the panel heading
  const rules = [...stylesheet.matchAll(/([^{}]+)\{([^}]*)\}/gu)].filter((rule) =>
    (rule[1] ?? "").split(",").some((selector) => selector.trim() === ".section-heading h2"),
  )

  // Then: the 21px H2 from DESIGN.md, never the right-aligned description rule
  expect(rules.length).toBeGreaterThan(0)
  for (const rule of rules) {
    expect(rule[2]).not.toMatch(/text-align:\s*right/u)
  }
  expect(rules.some((rule) => /font-size:\s*21px/u.test(rule[2] ?? ""))).toBe(true)
})

test("workspace grid reserves a row for the app-error banner", async () => {
  // Given: the workspace shell stacks four children — topbar, #app-error
  // banner, scroll body, footer.
  const stylesheet = await Bun.file(new URL("./styles.css", import.meta.url)).text()

  // When
  const workspaceRule = stylesheet.match(/^\.workspace\s*\{([^}]*)\}/mu)
  const rows = workspaceRule?.at(1)?.match(/grid-template-rows:\s*([^;]+);/u)?.at(1)

  // Then: four row tracks — a three-track template collapses the banner's row
  // to 0px and the opaque body panel paints over the banner text (VQA-006).
  expect(rows?.trim()).toBe("auto auto minmax(0, 1fr) auto")
})

test("font stacks cover Windows system fonts", async () => {
  // Given
  const stylesheet = await Bun.file(new URL("./styles.css", import.meta.url)).text()

  // When
  const root = stylesheet.match(/:root\s*\{[^}]*?font-family:([^;]+);/u)?.at(1) ?? ""
  const monoStacks = [...stylesheet.matchAll(/"SFMono-Regular"[^;]*?;/gu)].map((match) => match[0])

  // Then: Korean UI text and mono blocks fall back to Windows fonts before generic families
  expect(root).toContain('"Segoe UI"')
  expect(root).toContain('"Malgun Gothic"')
  expect(monoStacks.length).toBeGreaterThan(0)
  for (const stack of monoStacks) expect(stack).toContain("Consolas")
})

test("component display rules do not outrank the hidden attribute", async () => {
  // Given
  const stylesheet = await Bun.file(new URL("./styles.css", import.meta.url)).text()

  // Then: `html [hidden]` ties with single-class rules, so these need own hides
  const hideRule = stylesheet.match(/([^{}]+)\{([^}]*display:\s*none[^}]*)\}/gu) ?? []
  const selectors = hideRule.join("\n")
  expect(selectors).toMatch(/\.engine-segmented\s+label\[hidden\]/u)
  expect(selectors).toMatch(/#engine-device\[hidden\]/u)
})
