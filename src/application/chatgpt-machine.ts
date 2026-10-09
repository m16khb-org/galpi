import type { ChatGptSignInPhase } from "../domain/chatgpt"

export interface ChatGptFlowState {
  readonly status: "idle" | "awaitingBrowser" | "exchanging" | "failed"
  readonly code: string | null
  readonly message: string | null
}

export type ChatGptFlowEvent =
  | { readonly type: "started" }
  | { readonly type: "phase"; readonly phase: ChatGptSignInPhase }
  | { readonly type: "succeeded" }
  | { readonly type: "failed"; readonly code: string | null; readonly message: string }
  | { readonly type: "cancelled" }

export const initialChatGptFlow: ChatGptFlowState = {
  status: "idle",
  code: null,
  message: null,
}

export function isChatGptFlowActive(state: ChatGptFlowState): boolean {
  return state.status === "awaitingBrowser" || state.status === "exchanging"
}

/**
 * Sign-in progress as the window sees it. The final outcome comes from the
 * command's result; `phase` events only narrate the wait, so one that arrives
 * after the outcome is dropped instead of reviving a finished attempt.
 */
export function reduceChatGptFlow(
  state: ChatGptFlowState,
  event: ChatGptFlowEvent,
): ChatGptFlowState {
  switch (event.type) {
    case "started":
      return isChatGptFlowActive(state)
        ? state
        : { status: "awaitingBrowser", code: null, message: null }
    case "phase":
      return isChatGptFlowActive(state) ? { ...state, status: event.phase } : state
    case "succeeded":
    case "cancelled":
      return initialChatGptFlow
    case "failed":
      return { status: "failed", code: event.code, message: event.message }
  }
}
