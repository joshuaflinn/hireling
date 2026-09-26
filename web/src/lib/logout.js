/**
 * The logout decision: what the app does with a logout answer.
 *
 * Only a confirmed logout puts the user on the signed-out screen, and that
 * screen offers an explicit Sign in action instead of bouncing through the
 * login leg: the house Authentik session survives a Hireling logout by
 * design, so an automatic redirect would mint a fresh session and flip the
 * user straight back to signed-in (E3 Story 6 AC3). Anything the server
 * could not confirm keeps the signed-in view with a retryable error.
 *
 * @param {number|null} status - the HTTP status `POST /api/auth/logout`
 *   answered with, or `null` when the request never completed
 * @returns {"signed-out"|"retry"} `"signed-out"` shows the signed-out
 *   screen; `"retry"` keeps the signed-in view with an error affordance.
 */
export function logoutAction(status) {
  if (status === 204) {
    return 'signed-out';
  }
  return 'retry';
}
