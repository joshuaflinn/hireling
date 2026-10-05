/* global HTMLDialogElement */
// jsdom does not implement <dialog>.showModal (whatwg/jsdom#3317). The
// components under test only observe the element's open/close state — the
// top layer, ::backdrop and native focus semantics a real browser adds are
// invisible here. Fill exactly that gap; do not shadow a native
// implementation.
for (const method of ['show', 'showModal', 'close']) {
  if (typeof HTMLDialogElement !== 'undefined' && !HTMLDialogElement.prototype[method]) {
    HTMLDialogElement.prototype[method] =
      method === 'close'
        ? function close() {
            this.removeAttribute('open');
          }
        : function open() {
            this.setAttribute('open', '');
          };
  }
}
