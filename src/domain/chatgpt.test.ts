import { describe, expect, test } from "bun:test"

import {
  assistantReady,
  type ChatGptAccount,
  type ChatGptModel,
  effortChoices,
  pickDefaultModel,
  pickEffort,
  signInFailureMessage,
} from "./chatgpt"

const signedIn: ChatGptAccount = { state: "signedIn", email: "dev@example.com" }
const signedOut: ChatGptAccount = { state: "signedOut", email: null }
const signInRequired: ChatGptAccount = { state: "signInRequired", email: "dev@example.com" }

const models: readonly ChatGptModel[] = [
  {
    slug: "gpt-5.5",
    displayName: "GPT-5.5",
    reasoningEfforts: ["low", "medium", "high", "xhigh"],
    defaultReasoningEffort: "medium",
  },
  {
    slug: "gpt-5.4-mini",
    displayName: "GPT-5.4 mini",
    reasoningEfforts: [],
    defaultReasoningEffort: null,
  },
]

describe("assistantReady", () => {
  test("API key mode is ready exactly when a key is stored", () => {
    // Given / When / Then: the ChatGPT account is irrelevant in API key mode
    expect(assistantReady("apiKey", true, signedOut)).toBe(true)
    expect(assistantReady("apiKey", false, signedIn)).toBe(false)
  })

  test("ChatGPT mode is ready only while signed in", () => {
    // Given / When / Then: a stored API key does not satisfy ChatGPT mode
    expect(assistantReady("chatGpt", true, signedIn)).toBe(true)
    expect(assistantReady("chatGpt", true, signedOut)).toBe(false)
    expect(assistantReady("chatGpt", true, signInRequired)).toBe(false)
  })
})

describe("pickDefaultModel", () => {
  test("keeps a saved model that the account still lists", () => {
    expect(pickDefaultModel(models, "gpt-5.4-mini")).toBe("gpt-5.4-mini")
  })

  test("falls back to the first model in server order", () => {
    // Given: nothing saved, or a saved slug the list no longer carries
    expect(pickDefaultModel(models, null)).toBe("gpt-5.5")
    expect(pickDefaultModel(models, "retired-model")).toBe("gpt-5.5")
  })

  test("has nothing to pick from an empty list", () => {
    expect(pickDefaultModel([], "gpt-5.5")).toBeNull()
  })
})

describe("reasoning effort", () => {
  test("offers what the model list says, or the widely accepted three when it says nothing", () => {
    expect(effortChoices(models[0])).toEqual(["low", "medium", "high", "xhigh"])
    expect(effortChoices(models[1])).toEqual(["low", "medium", "high"])
    expect(effortChoices(undefined)).toEqual([])
  })

  test("keeps a saved effort only while the model accepts it", () => {
    // Given / When / Then: switching to a model without `xhigh` hands the choice back to the model
    expect(pickEffort(models[0], "xhigh")).toBe("xhigh")
    expect(pickEffort(models[1], "xhigh")).toBeNull()
    expect(pickEffort(models[1], "high")).toBe("high")
    expect(pickEffort(models[0], null)).toBeNull()
  })
})

describe("signInFailureMessage", () => {
  test("states the denied consent in Korean regardless of the host wording", () => {
    // Given / When
    const message = signInFailureMessage("CHATGPT_CONSENT_DENIED", "denied")

    // Then
    expect(message.startsWith("ChatGPT 요금제 사용 동의가 거부되었습니다")).toBe(true)
  })

  test("passes every other host message through", () => {
    expect(signInFailureMessage("CHATGPT_SIGN_IN_TIMEOUT", "시간이 지났습니다.")).toBe(
      "시간이 지났습니다.",
    )
  })
})
