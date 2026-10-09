import {
  type AccessEvent,
  type AccessState,
  canSignIn,
  initialAccess,
  reduceAccess,
} from "../application/access-machine"
import { type AppAccess, type BackendPort, errorCode, errorMessage } from "../domain/backend"
import type { AccessScreenView } from "./access-screen"

/**
 * The Google sign-in through auth-gateway that every feature requires.
 *
 * The host enforces the rule on every feature command; this controller only
 * decides what the window shows. `onOpened` runs each time the app opens,
 * `onClosed` each time it closes again after a sign-out.
 */
export class AccessController {
  private state: AccessState = initialAccess

  constructor(
    private readonly backend: BackendPort,
    private readonly view: AccessScreenView,
    private readonly onOpened: () => Promise<void>,
    private readonly onClosed: () => void,
  ) {
    this.view.render(this.state)
  }

  /** Ask the host whether the stored sign-in still opens the app. */
  async check(): Promise<void> {
    if (this.dispatch({ type: "checkStarted" }).status !== "checking") return
    let access: AppAccess
    try {
      access = await this.backend.loadAppAccess()
    } catch (error) {
      this.dispatch({ type: "loadFailed", message: errorMessage(error) })
      return
    }
    if (this.dispatch({ type: "loaded", access }).status === "signedIn") await this.onOpened()
  }

  /** No IPC can succeed without the event channel, so only a restart helps. */
  runtimeUnavailable(message: string): void {
    this.dispatch({ type: "runtimeUnavailable", message })
  }

  async signIn(): Promise<void> {
    if (!canSignIn(this.state)) return
    this.dispatch({ type: "signInStarted" })
    let access: AppAccess
    try {
      access = await this.backend.signInToGateway()
    } catch (error) {
      this.dispatch(
        errorCode(error) === "CANCELLED"
          ? { type: "signInCancelled" }
          : { type: "signInFailed", message: errorMessage(error) },
      )
      return
    }
    if (this.dispatch({ type: "signInSucceeded", access }).status === "signedIn") {
      await this.onOpened()
    }
  }

  async cancelSignIn(): Promise<void> {
    if (this.state.status !== "signingIn") return
    try {
      await this.backend.cancelGatewaySignIn()
    } catch {
      // The sign-in keeps waiting; its own result still settles the screen.
    }
  }

  async signOut(): Promise<void> {
    if (this.state.status !== "signedIn") return
    this.dispatch({ type: "signOutStarted" })
    this.onClosed()
    try {
      await this.backend.signOutOfGateway()
      this.dispatch({ type: "signOutSucceeded" })
    } catch (error) {
      this.dispatch({ type: "signOutFailed", message: errorMessage(error) })
    }
  }

  private dispatch(event: AccessEvent): AccessState {
    this.state = reduceAccess(this.state, event)
    this.view.render(this.state)
    return this.state
  }
}
