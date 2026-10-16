// Client store — strictly-newer merge, optimistic echo, ack settlement (E7).
//
// Per-field truth is a server slot `{ value, version }`; a pending op renders
// an optimistic echo over it until its ack settles the field. Merge rule is
// strictly-newer per field: a diff at version <= the stored version is a
// no-op — which makes the writer's own post-ack echo disappear for free, and
// makes any interleaving of two devices' diffs converge to the same state.
//
// Revert semantics (degraded-mode contract §4):
// - `applied`     — settle the field at the committed value/version
// - `superseded`  — drop the echo, silently; the newest server truth shows
//                   through (never an error, never an exposure — FR-8)
// - `rejected` / `forbidden` — drop the echo AND expose the op record on the
//   event bus for E6 (user-input / authz faults are not covered by the
//   silence rule).
//
// `isSyncing()` counts unsettled pending ops — in composition (index.js)
// every queue enqueue is mirrored by `enqueueView` and every dequeue by an
// `ack`, so pending count == queue length and the indicator's truth is the
// queue, as the degraded-mode contract demands.

/**
 * One queued/local write — field-for-field the `ClientFrame::Write` payload
 * plus the local creation stamp.
 *
 * @typedef {Object} SyncOp
 * @property {string} op_id
 * @property {Record<string, *>} target
 * @property {number} base_version
 * @property {*} value
 * @property {string} created_at
 */

/**
 * One field as `state()` reports it: the rendered value, the server's own last
 * statement, the version both are gated on, and the server's own last statement
 * separately. Top-level so a consumer that folds `state()` can name the entry
 * type rather than reach for `any` — party/stash.js is that consumer.
 *
 * @typedef {Object} FieldState
 * @property {Record<string, *>} target
 * @property {*} value the field AS SHOWN — the pending echo while a write is in
 *   flight, the server's statement otherwise
 * @property {number} version
 * @property {*} serverValue the server's last statement, surviving the echo
 */

/**
 * @typedef {Object} StoreEvent
 * @property {'changed'|'op_exposed'} type
 * @property {string} [key] targetKey of the touched field (`changed`)
 * @property {string} [op_id] (`op_exposed`)
 * @property {string} [outcome] (`op_exposed`)
 * @property {SyncOp} [op] (`op_exposed`)
 */

/**
 * Stable key for a field target — key order never splits a field, and no
 * two distinct targets share a key. The encoding is a prefix code: strings
 * are quoted with backslash escapes, containers bracketed, atoms bare —
 * so only strings can contain quotes, brackets, commas, or colons.
 * (Deliberately not JSON.stringify: a hand-rolled total order here is the
 * identity, and the gate's no-stringify-keys rule is honest about the
 * key-order trap the naive version walks into.)
 */
/** @param {string} text @returns {string} */
function quote(text) {
  return `"${text.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`;
}

/** The canonical encoding `targetKey` is built from: key-sorted, so the same
 *  value always yields the same string. Exported for a consumer that must
 *  fingerprint a value it did not build (party/stash.js caches a fold keyed on
 *  it) -- one encoder for both jobs, so the two can never disagree about when a
 *  value changed. */
/** @param {*} value @returns {string} */
export function encodeValue(value) {
  if (typeof value === 'string') return quote(value);
  if (value === null) return 'null';
  if (Array.isArray(value)) {
    // `encodeValue` is annotated `(*) -> string` and its recursion accepts a
    // nested array, so the elements have to be read as `*`. The cast sits on a
    // named variable rather than inside the template string because semgrep's JS
    // parser (rule `missing-template-string-indicator`, a blocking leg of
    // `just scan-semgrep`) mis-parses a comment in an arrow's parameter list as
    // the start of a template substitution and reports the line five times over.
    /** @type {*[]} */
    const items = value;
    return `[${items.map((item) => encodeValue(item)).join(',')}]`;
  }
  if (typeof value === 'object') {
    /** @type {Record<string, *>} */
    const obj = /** @type {Record<string, *>} */ (value);
    const parts = Object.keys(obj)
      .sort()
      .map((k) => `${quote(k)}:${encodeValue(obj[k])}`);
    return `{${parts.join(',')}}`;
  }
  // Numbers, booleans, undefined: bare atoms — unambiguous next to the
  // quoted strings, which is the whole point of the prefix code.
  return String(value);
}

/** @param {Record<string, *>} target */
export function targetKey(target) {
  return encodeValue(target);
}

/**
 * @returns {{
 *   state: () => Record<string, FieldState>,
 *   applyServerField: (target: Record<string, *>, value: *, version: number) => void,
 *   enqueueView: (viewOp: SyncOp) => void,
 *   ack: (opId: string, outcome: 'applied'|'superseded'|'already_applied'|'rejected'|'forbidden', serverVersion: number | null, serverValue: *) => void,
 *   isSyncing: () => boolean,
 *   subscribe: (cb: (event: StoreEvent) => void) => () => boolean,
 * }}
 */
/**
 * Is `value` an ADDITIVE write frame — a delta rather than a value?
 *
 * The one place this test is written. It used to live inside `createStore` and be
 * read only by `ack`, which meant the projection layer had to REDECIDE the same
 * question with its own shape test and got it wrong in both directions at once: a
 * stash quantity delta (`{op: 'qty', qty_delta: n}`) is additive and was treated
 * as absolute, and a sell or transfer frame is `{op: …}` carrying NO delta and was
 * treated as one. Same rule, two copies, two different answers.
 *
 * The rule is a shape test because the shape is the evidence, and the frame is the
 * only place the distinction exists: §4 gives an additive write the DELTA and no
 * balance, so a frame that names a `*_delta` is asserting "I am not a value" — the
 * absolute arms of the same targets (`{op: 'meta', bulk, notes}`, a sell's
 * `{price, proceeds}`) name no delta and are values. `isAdditiveOpValue` below is
 * this function, unchanged, under its old name.
 *
 * A frame is read for what it IS, not for what target it rode: the ack settles the
 * op, and the op's frame is the evidence, exactly as the comment in `ack` says.
 *
 * @param {*} value
 * @returns {boolean}
 */
export function isAdditiveFrame(value) {
  return (
    value !== null &&
    typeof value === 'object' &&
    ('gp_delta' in value ||
      'sp_delta' in value ||
      'cp_delta' in value ||
      'qty_delta' in value)
  );
}

/**
 * The value a field should SHOW: the pending echo while this seat's own write is in
 * flight, else the server's last statement.
 *
 * Exported for the same reason `isAdditiveFrame` is. A consumer that reads
 * `state()[key].value` and then re-derives what a pending frame MEANS has to know
 * the pending rule and the additive rule together, and if it knows them it can get
 * them wrong — which is how the stash screen came to paint a `qty_delta` frame as
 * though it were the balance's new value while its sibling fold added that same
 * delta to the server's number. Read the field through here and there is nothing
 * left to re-derive: `serverValue` is the last number the server stated, and this
 * says whether an echo is sitting in front of it.
 *
 * @param {{serverValue: *, pending: {opId: string, value: *} | null}} field
 * @returns {*}
 */
export function effectiveValueOf(field) {
  return field.pending !== null ? field.pending.value : field.serverValue;
}

export function createStore() {
  /**
   * @typedef {Object} Field
   * @property {Record<string, *>} target
   * @property {*} serverValue
   * @property {number} serverVersion
   * @property {{ opId: string, value: * } | null} pending
   * @property {number} [opCommittedVersion] version the last ADDITIVE ack
   *   recorded for this seat's own write — see `applyServerField`
   */
  /** @type {Map<string, Field>} */
  const fields = new Map();
  /** @type {Map<string, { op: SyncOp, key: string }>} */
  const pendingOps = new Map();
  /** @type {Set<(event: StoreEvent) => void>} */
  const listeners = new Set();

  /** @param {StoreEvent} event */
  function emit(event) {
    for (const cb of listeners) cb(event);
  }

  /** @param {Record<string, *>} target */
  function fieldFor(target) {
    const key = targetKey(target);
    let field = fields.get(key);
    if (!field) {
      field = { target, serverValue: null, serverVersion: 0, pending: null };
      fields.set(key, field);
    }
    return { key, field };
  }

  /**
   * Is this written value an additive delta frame (`{gp_delta…}` /
   * `{op:'qty', qty_delta…}`) rather than an absolute value? The additive class
   * is the one whose applied version arrives without its value (the delta was
   * added server-side; wire-protocol.md §4), so it is the only class whose ack
   * version is not also a version whose VALUE has been seen.
   *
   * @param {*} value
   */
  function isAdditiveOpValue(value) {
    return isAdditiveFrame(value);
  }

  return {
    /** Merged state: `{ [targetKey]: { target, value, version, serverValue,
     *  pendingValue, hasPending } }` — see `effectiveValueOf` for what `value`
     *  is showing and why the pair next to it is the honest question. */
    state() {
      /** @type {Record<string, FieldState>} */
      const out = {};
      for (const [key, field] of fields) {
        out[key] = {
          target: field.target,
          value: effectiveValueOf(field),
          // The two facts a projection needs to fold an additive echo, and the
          // only pair that lets it tell an echo from a server value: what the
          // server last STATED, and whether this seat has a frame in front of it.
          // `value` above cannot answer either — it is whichever of the two is
          // being shown, which is exactly why a fold that consumed it doubled a
          // delta on one side and drew a raw delta as a balance on the other.
          serverValue: field.serverValue,
          pendingValue: field.pending ? field.pending.value : null,
          hasPending: field.pending !== null,
          // Whose echo is in front of the value. A fold cache cannot key on
          // `pendingValue` alone: a delta that the server refuses leaves the
          // rendered number exactly where it was, and only the id distinguishes
          // "an echo is live" from "the echo just went away".
          pendingOpId: field.pending ? field.pending.opId : null,
          version: field.serverVersion,
          // The server's own last statement, alongside the rendered value above
          // rather than buried under it. `value` is the field as it is SHOWN —
          // the pending echo while a write is in flight, which is what a CAS
          // field wants and what degraded-mode §4 asks for. On the ADDITIVE
          // class that rendered value is the op's delta frame, so a reader that
          // must fold the echo onto a real balance or quantity cannot get the
          // base out of `value` at all; it is in `serverValue`. Nothing that
          // reads `value` changes, and the existing consumers (sheet/state.js,
          // party/state.js) keep their own mirrors and never look here.
          serverValue: field.serverValue,
        };
      }
      return out;
    },

    /** Server truth for one field. Strictly-newer only. */
    /** @param {Record<string, *>} target @param {*} value @param {number} version */
    applyServerField(target, value, version) {
      const { field } = fieldFor(target);
      // Strictly newer than the last value we applied — with one exception only
      // the additive class can produce: the version of THIS seat's own committed
      // write, whose value the ack did not carry (see `ack` above). Refusing it
      // would freeze the field at a delta frame forever, because the number the
      // ack recorded is the number the fan-out arrives under.
      const ownCommittedVersion = field.opCommittedVersion ?? -1;
      if (version > field.serverVersion || version === ownCommittedVersion) {
        field.target = target;
        field.serverValue = value;
        field.serverVersion = version;
        emit({ type: 'changed', key: targetKey(target) });
      }
    },

    /** Optimistic echo for a queued op; the server slot is untouched. */
    /** @param {SyncOp} viewOp */
    enqueueView(viewOp) {
      const { key, field } = fieldFor(viewOp.target);
      field.pending = { opId: viewOp.op_id, value: viewOp.value };
      pendingOps.set(viewOp.op_id, { op: viewOp, key });
      emit({ type: 'changed', key });
    },

    /**
     * Settle a pending op.
     *
     * @param {string} opId
     * @param {'applied'|'superseded'|'already_applied'|'rejected'|'forbidden'} outcome
     * @param {number|null} serverVersion committed/winning version if known
     * @param {*|null} serverValue committed/winning value if known
     */
    ack(opId, outcome, serverVersion, serverValue) {
      const pending = pendingOps.get(opId);
      if (!pending) return;
      pendingOps.delete(opId);
      const { field } = fieldFor(pending.op.target);

      if (serverVersion !== null && serverVersion > field.serverVersion) {
        // `applied`: the committed value lands. `superseded`/`rejected`/
        // `forbidden`: the winner arrived with the ack, ahead of its diff.
        field.serverValue = serverValue;
        field.serverVersion = serverVersion;
      }
      // The ack's `version` and the version of a field the server has already
      // STATE are different facts, and the strictly-newer test above cannot tell
      // them apart. On an ADDITIVE op they arrive as the SAME number:
      // wire-protocol.md §4 gives `applied` the committed version, and the
      // matching `diff` — which "may arrive before or after it" — carries that
      // same version on the resulting value. So recording it here first makes the
      // diff at that version fail the test above and get discarded, leaving the
      // field holding the op's own delta frame as though it were the committed
      // balance, and every later fan-out with it. `opCommittedVersion` keeps the
      // ack's number as what it is — "this op's write reached version N" — and
      // leaves `serverVersion` at the last version whose VALUE was seen, so the
      // additive diff is applied. CAS is untouched: its ack arrives carrying the
      // value, `serverVersion` advances, and nothing reads the new flag.
      //
      // Only the additive class has a version whose value is absent from the
      // frame that names it, so only it may hold an ack's version without the
      // value. The class is read off the op being settled, not off the field: the
      // frame is the delta, so the delta is the evidence. A CAS ack is not
      // recorded this way and the `=== opCommittedVersion` test below can never
      // fire, which keeps "a version we already have is not news" (and its
      // no-`changed`-event consequence) exactly as it was.
      if (serverVersion !== null && isAdditiveOpValue(pending.op.value)) {
        field.opCommittedVersion = serverVersion;
      }
      field.pending = null;

      if (outcome === 'rejected' || outcome === 'forbidden') {
        emit({
          type: 'op_exposed',
          op_id: opId,
          outcome,
          op: pending.op,
        });
      } else {
        emit({ type: 'changed', key: pending.key });
      }
    },

    /** True while any queued write is unsettled — the indicator's input. */
    isSyncing() {
      return pendingOps.size > 0;
    },

    /** @param {(event: StoreEvent) => void} cb */
    subscribe(cb) {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
  };
}
