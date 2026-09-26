/**
 * The entry-flow decision: what the app does with an `/api/me` status.
 *
 * The session probe happens on load (E3 Story 1 AC1): a visitor without a
 * live session is sent to the login leg instead of staring at a shell page.
 *
 * @param {number} status - the HTTP status `/api/me` answered with
 * @returns {"enter"|"offline"} `"enter"` sends the visitor to
 * `/api/auth/login`; `"offline"` keeps them here with a retry affordance.
 */
export function entryAction(status) {
  if (status === 401 || status === 403) {
    return 'enter';
  }
  return 'offline';
}
