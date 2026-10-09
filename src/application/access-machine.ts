import type { AppAccess } from "../domain/backend"

/**
 * Whether the window may show the workspace, as the login screen sees it.
 * `failed` is a check that could not finish: a retryable one offers both a
 * new check and a fresh sign-in (which overwrites whatever the host could not
 * read), an unretryable one only asks for a restart.
 */
export type AccessState =
  | { readonly status: "checking" }
  | { readonly status: "signedOut"; readonly error: string | null }
  | { readonly status: "signingIn" }
  | { readonly status: "signedIn"; readonly email: string | null; readonly offline: boolean }
  | { readonly status: "signingOut" }
  | { readonly status: "failed"; readonly message: string; readonly retryable: boolean }

export type AccessEvent =
  | { readonly type: "checkStarted" }
  | { readonly type: "loaded"; readonly access: AppAccess }
  | { readonly type: "loadFailed"; readonly message: string }
  | { readonly type: "runtimeUnavailable"; readonly message: string }
  | { readonly type: "signInStarted" }
  | { readonly type: "signInSucceeded"; readonly access: AppAccess }
  | { readonly type: "signInFailed"; readonly message: string }
  | { readonly type: "signInCancelled" }
  | { readonly type: "signOutStarted" }
  | { readonly type: "signOutSucceeded" }
  | { readonly type: "signOutFailed"; readonly message: string }

export const initialAccess: AccessState = { status: "checking" }

/** The login button shows only where a sign-in may begin. */
export function canSignIn(state: AccessState): boolean {
  return state.status === "signedOut" || (state.status === "failed" && state.retryable)
}

export function reduceAccess(state: AccessState, event: AccessEvent): AccessState {
  switch (event.type) {
    case "checkStarted":
      return state.status === "failed" && state.retryable ? initialAccess : state
    case "loaded":
    case "signInSucceeded": {
      // A load answers a check, a sign-in answers its own attempt; any other
      // arrival is stale.
      if (state.status !== (event.type === "loaded" ? "checking" : "signingIn")) return state
      const access = event.access
      return access.state === "signedIn"
        ? { status: "signedIn", email: access.email, offline: access.offline }
        : { status: "signedOut", error: null }
    }
    case "loadFailed":
      return state.status === "checking"
        ? { status: "failed", message: event.message, retryable: true }
        : state
    case "runtimeUnavailable":
      return { status: "failed", message: event.message, retryable: false }
    case "signInStarted":
      return canSignIn(state) ? { status: "signingIn" } : state
    case "signInFailed":
      return state.status === "signingIn" ? { status: "signedOut", error: event.message } : state
    case "signInCancelled":
      return state.status === "signingIn" ? { status: "signedOut", error: null } : state
    case "signOutStarted":
      return state.status === "signedIn" ? { status: "signingOut" } : state
    case "signOutSucceeded":
      return state.status === "signingOut" ? { status: "signedOut", error: null } : state
    case "signOutFailed":
      return state.status === "signingOut" ? { status: "signedOut", error: event.message } : state
  }
}
