const MASKED_SECRET = "••••••••••••"

export function nextSecretVisibility(visible: boolean): boolean {
  return !visible
}

export function secretDisplayValue(secret: string, visible: boolean): string {
  return visible ? secret : MASKED_SECRET
}
