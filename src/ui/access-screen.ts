import { type AccessState, canSignIn } from "../application/access-machine"
import { required } from "./dom"

const SIGNING_IN =
  "브라우저에서 Google 로그인을 마쳐 주세요. 로그인 창이 보이지 않으면 취소 후 다시 시도하세요."

/**
 * The login screen in front of the app shell, and the account chip in the
 * top bar. The shell stays inert until someone is signed in, so nothing
 * behind the screen can be reached by pointer, keyboard, or screen reader.
 */
export class AccessScreenView {
  private readonly root: HTMLElement

  constructor(root: HTMLElement) {
    this.root = root
  }

  render(state: AccessState): void {
    const open = state.status === "signedIn"
    this.element("#access-screen").hidden = open
    this.element(".app-shell").toggleAttribute("inert", !open)

    const [message, tone] = statusLine(state)
    const status = this.element("#access-status")
    status.textContent = message
    status.dataset["state"] = tone
    this.element("#access-sign-in-button").hidden = !canSignIn(state)
    this.element("#access-retry-button").hidden = !(state.status === "failed" && state.retryable)
    this.element("#access-cancel-button").hidden = state.status !== "signingIn"

    this.element("#account-chip").hidden = !open
    this.element("#sign-out-button").hidden = !open
    if (state.status === "signedIn") {
      const email = this.element("#account-email")
      email.textContent = state.email ?? "Google 계정"
      email.dataset["state"] = state.offline ? "pending" : "ready"
      this.element("#account-mode").textContent = state.offline
        ? "오프라인 · 저장된 로그인 사용"
        : "온라인"
    }
  }

  /** Signing out mid-recording or mid-job would strand the work, so it waits. */
  setSignOutBlocked(blocked: boolean): void {
    this.element<HTMLButtonElement>("#sign-out-button").disabled = blocked
  }

  private element<T extends HTMLElement = HTMLElement>(selector: string): T {
    return required<T>(this.root, selector)
  }
}

function statusLine(state: AccessState): readonly [string, "pending" | "ready" | "error"] {
  switch (state.status) {
    case "checking":
      return ["로그인 상태를 확인하는 중입니다.", "pending"]
    case "signedOut":
      return state.error === null
        ? ["Google 계정으로 로그인하면 바로 사용할 수 있습니다.", "ready"]
        : [state.error, "error"]
    case "signingIn":
      return [SIGNING_IN, "pending"]
    case "signingOut":
      return ["로그아웃하는 중입니다.", "pending"]
    case "failed":
      return [
        state.retryable ? `${state.message} 다시 확인하거나 다시 로그인해 주세요.` : state.message,
        "error",
      ]
    case "signedIn":
      return ["로그인했습니다.", "ready"]
  }
}
