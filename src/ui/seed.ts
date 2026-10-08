export type ActionButtonVariant = "brandSolid" | "neutralOutline" | "ghost"
export type ActionButtonSize = "xsmall" | "medium"
export type ActionButtonLayout = "withText" | "iconOnly"

const root = "seed-action-button"
const variantPrefix = `${root}--variant_`

/** Class list for `@seed-design/css/recipes/action-button.css`, named like SEED's `createClassName`. */
export function actionButtonClass(
  variant: ActionButtonVariant,
  size: ActionButtonSize = "medium",
  layout: ActionButtonLayout = "withText",
): string {
  return [
    root,
    `${variantPrefix}${variant}`,
    `${root}--size_${size}`,
    `${root}--layout_${layout}`,
    `${root}--size_${size}-layout_${layout}`,
  ].join(" ")
}

export function setActionButtonVariant(button: HTMLElement, variant: ActionButtonVariant): void {
  for (const name of [...button.classList]) {
    if (name.startsWith(variantPrefix)) button.classList.remove(name)
  }
  button.classList.add(`${variantPrefix}${variant}`)
}
