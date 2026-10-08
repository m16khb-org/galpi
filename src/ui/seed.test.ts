import { expect, test } from "bun:test"
import { Window } from "happy-dom"

import {
  type ActionButtonLayout,
  type ActionButtonSize,
  type ActionButtonVariant,
  actionButtonClass,
  setActionButtonVariant,
} from "./seed"

test("composes the action-button recipe classes the way SEED names them", () => {
  expect(actionButtonClass("brandSolid")).toBe(
    "seed-action-button seed-action-button--variant_brandSolid seed-action-button--size_medium seed-action-button--layout_withText seed-action-button--size_medium-layout_withText",
  )
})

test("every class the helper can produce exists in the installed recipe stylesheet", async () => {
  // Given: an unknown class renders an unstyled button, so a SEED rename must fail here
  const recipe = await Bun.file(
    Bun.resolveSync("@seed-design/css/recipes/action-button.css", import.meta.dir),
  ).text()
  const variants: ActionButtonVariant[] = ["brandSolid", "neutralOutline", "ghost"]
  const sizes: ActionButtonSize[] = ["xsmall", "medium"]
  const layouts: ActionButtonLayout[] = ["withText", "iconOnly"]

  // When
  const missing = variants
    .flatMap((variant) =>
      sizes.flatMap((size) => layouts.map((layout) => actionButtonClass(variant, size, layout))),
    )
    .flatMap((classes) => classes.split(" "))
    .filter((name) => !recipe.includes(`.${name} {`))

  // Then
  expect(missing).toEqual([])
})

test("switching variants swaps only the variant class", () => {
  // Given
  const button = new Window().document.createElement("button") as unknown as HTMLElement
  button.className = `primary-button ${actionButtonClass("brandSolid", "medium", "withText")}`

  // When
  setActionButtonVariant(button, "neutralOutline")

  // Then
  expect([...button.classList]).toEqual([
    "primary-button",
    "seed-action-button",
    "seed-action-button--size_medium",
    "seed-action-button--layout_withText",
    "seed-action-button--size_medium-layout_withText",
    "seed-action-button--variant_neutralOutline",
  ])
})
