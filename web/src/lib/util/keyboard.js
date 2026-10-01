// The dialog keyboard pattern (E6 design §9), implemented once: focus is
// trapped in a container while it is open, Escape cancels, Enter commits,
// and focus returns to the opener on close. Every dialog in the sheet —
// prep picker, level-down confirm, New Day confirm — rides this module;
// `window.confirm` appears nowhere.
//
// The container is anything with `addEventListener`/`removeEventListener`
// and a `querySelectorAll`; the host element is anything with `focus()`.
// Tests drive `keydown` handlers by hand — no DOM environment required.

/**
 * @typedef {Object} TrapTarget
 * @property {() => void} focus
 */

/**
 * Install the keyboard discipline on `container` while `host` is shown.
 *
 * - Tab / Shift+Tab cycle within `container`'s focusable descendants (a
 *   container with none, or without querySelectorAll, lets the browser
 *   handle it — the trap only ever constrains, never steals).
 * - Escape calls `onCancel`.
 * - Enter on a non-button, non-textarea element calls `onCommit` (buttons
 *   and links commit natively; textareas need their newlines).
 *
 * Returns the disposer: call it on close, then focus the opener.
 *
 * @param {{
 *   container: {addEventListener: Function, removeEventListener: Function, querySelectorAll?: (sel: string) => Iterable<TrapTarget>},
 *   host: TrapTarget,
 *   onCancel: () => void,
 *   onCommit?: () => void,
 * }} setup
 * @returns {() => void} disposer — removes the listener and restores focus
 */
export function trapKeys({ container, host, onCancel, onCommit }) {
  const FOCUSABLE =
    'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])';

  function onKeydown(event) {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      onCancel();
      return;
    }
    if (event.key === 'Enter') {
      const target = /** @type {{tagName?: string}} */ (event.target) ?? {};
      const nativeCommit =
        target.tagName === 'BUTTON' ||
        target.tagName === 'A' ||
        target.tagName === 'TEXTAREA';
      if (!nativeCommit && onCommit) {
        event.preventDefault();
        onCommit();
        return;
      }
    }
    if (event.key === 'Tab' && typeof container.querySelectorAll === 'function') {
      const focusables = Array.from(container.querySelectorAll(FOCUSABLE));
      if (focusables.length === 0) return;
      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      if (event.shiftKey && event.target === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && event.target === last) {
        event.preventDefault();
        first.focus();
      }
    }
  }

  container.addEventListener('keydown', onKeydown, true);
  return () => {
    container.removeEventListener('keydown', onKeydown, true);
    host.focus();
  };
}
