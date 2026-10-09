import { expect, test } from "bun:test"

const readStylesheet = () => Bun.file(new URL("./styles.css", import.meta.url)).text()
const withoutComments = (css: string) => css.replace(/\/\*[\s\S]*?\*\//gu, "")
const declarations = (css: string) =>
  [...withoutComments(css).matchAll(/(?:^|[;{])\s*([a-z-]+)\s*:\s*([^;{}]+);/gu)].map((match) => ({
    property: match[1] ?? "",
    value: (match[2] ?? "").trim(),
  }))

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

  // Then: the H2 level from DESIGN.md, never the right-aligned description rule
  expect(rules.length).toBeGreaterThan(0)
  for (const rule of rules) {
    expect(rule[2]).not.toMatch(/text-align:\s*right/u)
  }
  expect(rules.some((rule) => /font-size:\s*var\(--seed-font-size-t8\)/u.test(rule[2] ?? ""))).toBe(true)
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

test("loads SEED tokens and the action-button recipe before any app rule", async () => {
  // Given
  const stylesheet = await readStylesheet()

  // When
  const imports = stylesheet.match(/^@import\s+"[^"]+";$/gmu) ?? []

  // Then
  expect(imports).toContain('@import "@seed-design/css/base.css";')
  expect(imports).toContain('@import "@seed-design/css/recipes/action-button.css";')
})

test("takes every color from SEED variables instead of literals", async () => {
  // Given
  const stylesheet = withoutComments(await readStylesheet())

  // When
  const literals = stylesheet.match(/#[0-9a-fA-F]{3,8}\b|\brgba?\(|\bhsla?\(/gu) ?? []

  // Then
  expect(literals).toEqual([])
})

test("declares no app-owned custom properties, only SEED overrides", async () => {
  // Given
  const stylesheet = await readStylesheet()

  // When
  const custom = declarations(stylesheet)
    .map((declaration) => declaration.property)
    .filter((property) => property.startsWith("--"))

  // Then
  expect(custom.filter((property) => !property.startsWith("--seed-"))).toEqual([])
})

test("spacing, type, radius, and motion values come from SEED tokens", async () => {
  // Given
  const stylesheet = await readStylesheet()
  const tokenized = /^(padding|margin|gap|row-gap|column-gap|font-size|border-radius)(-[a-z-]+)?$/u

  // When
  const pxLeaks = declarations(stylesheet).filter(
    (declaration) => tokenized.test(declaration.property) && /\d+px\b/u.test(declaration.value),
  )
  const msLiterals = withoutComments(stylesheet).match(/\b\d*\.?\d+m?s\b/gu) ?? []

  // Then: only the reduced-motion floor keeps a literal duration
  expect(pxLeaks).toEqual([])
  expect(msLiterals.filter((literal) => literal !== "0.01ms" && literal !== "1.6s")).toEqual([])
})

test("reduced motion overrides recipe transitions and Korean text keeps whole words", async () => {
  // Given
  const stylesheet = await readStylesheet()

  // When
  const reduced = stylesheet.match(/@media \(prefers-reduced-motion: reduce\)\s*\{([\s\S]*?)\n\}/u)?.at(1) ?? ""

  // Then: recipe classes beat a `*` rule, so each reset needs !important
  expect(reduced).toMatch(/transition-duration:\s*0\.01ms !important;/u)
  expect(reduced).toMatch(/animation-duration:\s*0\.01ms !important;/u)
  expect(reduced).toMatch(/animation-iteration-count:\s*1 !important;/u)
  expect(stylesheet).toMatch(/^p\s*\{[^}]*word-break:\s*keep-all;/mu)
})

test("AA overrides sit in one :root block after every import", async () => {
  // Given
  const stylesheet = await readStylesheet()
  const selector = ':root[data-seed-color-mode="light-only"] {'

  // When
  const blocks = stylesheet.split(selector).length - 1
  const lastImport = Math.max(...[...stylesheet.matchAll(/^@import\s/gmu)].map((match) => match.index ?? -1))

  // Then: the same selector as SEED's base.css, so source order makes it win
  expect(blocks).toBe(1)
  expect(stylesheet).not.toContain("html[data-seed-color-mode")
  expect(stylesheet.indexOf(selector)).toBeGreaterThan(lastImport)
})

test("DESIGN.md lists exactly the SEED tokens the stylesheet uses", async () => {
  // Given
  const [stylesheet, design] = await Promise.all([
    readStylesheet(),
    Bun.file(new URL("../DESIGN.md", import.meta.url)).text(),
  ])

  // When: token names, deduplicated
  const used = [...new Set(stylesheet.match(/--seed-[a-z0-9_-]+/gu))].sort()
  const documented = [...new Set(design.match(/--seed-[a-z0-9_-]+/gu))].sort()

  // Then
  expect(documented).toEqual(used)
})

test("DESIGN.md describes depth with SEED shadows, not warm or pure-black prose", async () => {
  // Given
  const design = await Bun.file(new URL("../DESIGN.md", import.meta.url)).text()

  // Then
  expect(design.match(/warm|pure black/giu) ?? []).toEqual([])
})
