import { expect, test } from "bun:test"

import { appTemplate } from "./app-template"
import { nextSecretVisibility, secretDisplayValue } from "./secret-mask"

test("toggles secret visibility without changing the value", () => {
  expect(nextSecretVisibility(false)).toBeTrue()
  expect(nextSecretVisibility(true)).toBeFalse()
  expect(secretDisplayValue("secret-key", false)).not.toContain("secret-key")
  expect(secretDisplayValue("secret-key", true)).toBe("secret-key")
})

test("uses a non-credential input to avoid WebView password restoration", () => {
  const field = appTemplate.match(/<input id="settings-assistant-key"[^>]*>/u)?.at(0)

  expect(field).toContain('autocomplete="off"')
  expect(field).toContain('type="text"')
})

test("offers no Hugging Face token controls", () => {
  expect(appTemplate).not.toMatch(/hugging.?face|settings-hf-token|token-guide|model-access/iu)
})
