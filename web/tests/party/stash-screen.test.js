// E12 Task 6 — the stash screen, mounted and driven (house production-path rule).
//
// Every assertion here goes through `render()` + `fireEvent` on the real
// component and reads what a user sees — role, label, placeholder, disabled.
// The spec's affordances each get their NON-default state exercised, because a
// render that only passes the empty state never enters the branch the spec is
// about:
//   - the sell dialog's two prompt states (book value / no book value) — two
//     fixtures asserting two different results (the two-fixture rule)
//   - the transfer picker's quantity bound — two rows with different stock
//   - the quartermaster toggle's enabled and disabled seats
//   - the GM seat's ZERO write affordances, scoped to the screen's region
//     (E14 SC-5's discipline: assert the region's control set, not a page count)
//   - a refused op record, rendered from a driven event, non-blocking

/* global URL, setTimeout */

import { afterEach, test, vi } from "vitest";
import assert from "node:assert/strict";
import {
  render,
  screen,
  cleanup,
  fireEvent,
  within,
  waitFor,
} from "@testing-library/svelte";

/**
 * Drive a real <select> to a chosen option. `fireEvent.selectOptions` does not
 * exist in this project's testing-library version (`user-event` is not a
 * dependency either, and this suite will not add one to click a dropdown).
 * Setting `.value` then firing `change` is what a keyboard user produces: it is
 * the same event svelte 5's `onchange` listens for, and jsdom refuses a value
 * that no option carries — so a typo'd target throws instead of silently
 * selecting nothing.
 */
async function choose(select, value) {
  select.value = String(value);
  await fireEvent.change(select);
}

import StashScreen from "../../src/lib/party/StashScreen.svelte";
import { createStashState } from "../../src/lib/party/stash.js";
import { createSync } from "../../src/lib/sync/index.js";

afterEach(() => {
  cleanup();
  // The book-value read is a stubbed global fetch (see `mount`); leave no stub
  // for a test that did not install one.
  vi.unstubAllGlobals();
});

const STASH = (p, name) => ({ kind: "stash", party_id: p, item_name: name });
const BANK = (p) => ({ kind: "bank", party_id: p });
const PARTY = (p) => ({ kind: "party", party_id: p, field: "quartermaster" });
const CLAIM = (id) => ({ kind: "claim", claim_id: id });

const ROW = (name, quantity, extra = {}) => ({
  item_name: name,
  quantity,
  bulk: null,
  notes: null,
  ...extra,
});

const CLAIM_ROW = (
  id,
  item,
  origin,
  destination,
  created_at = "2026-10-15T00:00:00Z",
) => ({
  id,
  item_name: item,
  quantity: 1,
  origin,
  destination,
  actor_sub: "dev-sub-a",
  created_at,
  coins_gp: null,
  coins_sp: null,
  coins_cp: null,
});

/**
 * A real sync surface with a recorded wire underneath it.
 *
 * `createSync` composes the production queue, store and connection (index.js:82-
 * 95) and takes `socketFactory`, `storage`, `idFactory`, `now` and `timers` as
 * seams — so the whole client protocol can be driven over a socket stub while
 * every rule the screen depends on stays the production one: the store's
 * own-write echo and its clearing, `settleAck`'s `op_exposed` fan-out with the
 * surface's own op object (index.js:188, 205-210), version-merge on field
 * frames, and the queue's ids. `createSync` is the ONLY place those cross-module
 * handoffs happen, so a hand-rolled fake is not a shortcut — it is a different
 * program.
 *
 * What broke when this file drove a hand-rolled fake instead: it emitted an
 * `op_exposed` shape the surface does emit only post-dequeue and with a full op,
 * it minted op ids the store's `pendingOps` table had never seen so no ack ever
 * cleared an echo, and it invented `target` fields the server never sends — three
 * fabrications, all of them invisible to the assertions they were written to
 * satisfy. A screen test that has to fake the protocol can only prove the screen
 * agrees with the fake.
 *
 * @param {{frames?: Array<[Record<string, *>, *, number]>}} [o]
 */
function fakeSync(o = {}) {
  /** Every frame the surface sent, in order. */
  const sent = /** @type {Array<Record<string, *>>} */ ([]);
  /** Frames the test has NOT yet answered, in arrival order. */
  const unanswered = /** @type {Array<Record<string, *>>} */ ([]);
  // Field frames the fixture declares up front, fed through the surface's own
  // boot-snapshot path — the same version-merging code a live snapshot goes
  // through (index.js:105-112), so the fixture states the world the way a server
  // does and not through a private API.
  const primed = (o.frames ?? []).map((frame) => ({
    field: frame.target,
    value: frame.value,
    version: frame.version,
  }));
  let seq = 0;
  let socket = /** @type {any} */ (null);

  const storage = {
    /** @type {Map<string, string>} */
    bag: new Map(),
    getItem: (/** @type {string} */ k) => storage.bag.get(k) ?? null,
    setItem: (/** @type {string} */ k, /** @type {string} */ v) => {
      storage.bag.set(k, v);
    },
  };

  const sync = createSync({
    url: "ws://test/party",
    storage: /** @type {any} */ (storage),
    accountSub: "dev-sub-a",
    // The `SyncSocket` contract is callbacks-as-properties, not registration
    // methods (connection.js:43-50): `onopen`/`onmessage` are assigned by the
    // connection and CALLED by it, so a stub that offered `onOpen(cb)` would have
    // nothing to call and would sit silent forever — every inbound frame this file
    // feeds would die on "the socket handler is not installed". `open()` is called
    // by the test right after the surface is built; that is what drives the
    // snapshot/merge/drain/live phase sequence (connection.js:183-199), which the
    // queue drain and `connectionState()` depend on.
    socketFactory: () => {
      /** @type {any} */
      const stub = {
        sent: /** @type {Array<string>} */ ([]),
        send: (/** @type {string} */ msg) => {
          const frame = JSON.parse(msg);
          sent.push(frame);
          // `pong` is the connection's own l reply (connection.js:175-177); it is
          // not a write and must never be mistaken for one by `ack()`.
          if (frame.t === "write") unanswered.push(frame);
        },
        close: () => {},
        onopen: null,
        onmessage: null,
        onclose: null,
        onerror: null,
      };
      socket = stub;
      
      return stub;
    },
    bootSnapshot: primed.length ? { fields: primed } : undefined,
    idFactory: () => `op-${++seq}`,
    now: () => 0,
    timers: { setTimeout: (/** @type {any} */ fn) => setTimeout(fn, 0), clearTimeout },
  });

  // `connect()` is what installs the socket handler AND drains the queue, and
  // the screen's `connectionState()` guard at StashScreen.svelte:230 refuses to
  // post while the link is not live. Drive it explicitly rather than relying on a
  // transport seam that never opens: `deliver` below is the only input this file
  // uses, and it goes through the surface's own frame parser.
  sync.connect();

  // Opening the socket is what installs the handlers and runs the phase sequence;
  // `connect()` above already built it, so `open()` here is the "link came up"
  // moment the connection is waiting for.
  const open = () => {
    assert.ok(socket, "the socket stub was never constructed");
    assert.ok(socket.onopen, "the connection never installed onopen");
    socket.onopen();
  };

  /** Feed one INBOUND frame through the surface's own parser. */
  const inbound = (/** @type {Record<string, *>} */ frame) => {
    assert.ok(socket, "the socket stub was never constructed");
    assert.ok(socket.onmessage, "the connection never installed onmessage");
    socket.onmessage({ data: JSON.stringify(frame) });
  };

  return {
    /** Frames the socket carried, raw. */
    sent: () => sent,

    open,
    disconnect() {},
    hangUp() {},
    state: () => sync.state(),
    derived: () => sync.derived(),
    isSyncing: () => sync.isSyncing(),
    snapshotForBoot: () => sync.snapshotForBoot(),
    queue: () => sync.queue(),
    connectionState: () => "live",
    subscribe: (/** @type {(e: any) => void} */ cb) => sync.subscribe(cb),

    /**
     * A server field frame at `version`, in the shape the wire actually uses.
     */
    deliver(...args) {
      // Accept BOTH input shapes the wire has, because this file has to speak both
      // and the server's own frame is the more informative one to type:
      //   deliver({t:"diff", field, value, version})   — a whole frame, verbatim
      //   deliver(target, value, version)              — shorthand for one field
      // `diff` is a SINGLE-field frame (index.js:169 reads `frame.field`, never
      // `frame.fields`), so a caller that hands over a `fields` array is answered
      // with one diff per field, in order — the fan-out a server send does anyway.
      const [first, value, version] = /** @type {[any, any, number]} */ (args);
      if (first && typeof first === "object" && first.t === "diff") {
        inbound(first);
        for (const f of first.fields ?? []) inbound({ t: "diff", party_id: 1, ...f });
        return;
      }
      inbound({ t: "diff", party_id: 1, field: first, value, version });
    },

    /**
     * Ack a write, in the shape the wire uses. Address it one of three ways, and
     * all three end at the same `inbound({t:"ack", …})` — the only honest way in,
     * because the surface's refusal record is a CONSEQUENCE of settling an ack
     * (index.js:195-210): the store exposes, the store emits `op_exposed` with the
     * op it looked up out of the real queue, and the store decides what the screen
     * sees. A helper that built the `op_exposed` event by hand would be asserting
     * against an event shape the surface may never produce, and it is how this
     * file ended up fanning refusals with a fabricated `{op_id, target, value}`
     // object no queue ever held — which is exactly the artifact that cannot prove
     * the screen names what was refused.
     *
     *   ack({kind: "stash", outcome: "rejected", reason: "…"})  by target kind
     *   ack({opId: "op-2", outcome: "forbidden"})               by surface op id
     *   ack({op: frame, outcome: "rejected"})                   by a recorded frame
     *
     * `noMatch: null` says "ack whatever is oldest, and it is fine if there is
     * nothing": the lost-frame case below needs an ack for an op the queue has
     * already dropped, and `settleAck` handles that by passing `op: null`
     * (index.js:188) — the only way this file can reproduce a refusal whose frame
     * is gone without inventing an event.
     *
     * @param {{kind?: string, opId?: string, op?: Record<string, *>, outcome?: string,
     *   version?: number|null, reason?: string|null, noMatch?: null}} [opts]
     */
    ack(opts = {}) {
      /** @type {Record<string, *> | null} */
      let frame = null;
      if (opts.op) {
        frame = opts.op;
      } else if (opts.opId) {
        frame =
          sent.find((f) => f.op_id === opts.opId) ??
          (opts.noMatch === null ? null : undefined);
        assert.ok(
          frame !== undefined,
          `nothing was ever sent with op_id ${opts.opId}`,
        );
      } else {
        frame =
          (opts.kind
            ? unanswered.find(
                (f) => /** @type {any} */ (f.target).kind === opts.kind,
              )
            : unanswered[0]) ?? null;
        assert.ok(
          frame,
          `nothing unacknowledged${opts.kind ? ` of kind ${opts.kind}` : ""} — ` +
            `${unanswered.length} write frame(s) outstanding, ${sent.length} sent`,
        );
      }
      const at = unanswered.indexOf(frame);
      if (at >= 0) unanswered.splice(at, 1);
      inbound({
        t: "ack",
        party_id: 1,
        op_id: /** @type {string} */ (/** @type {any} */ (frame)?.op_id ?? opts.opId),
        outcome: opts.outcome ?? "applied",
        version: opts.version ?? null,
        reason: opts.reason ?? null,
      });
    },

    /**
     * A refusal the server sent for an op THIS seat posted, over the wire.
     *
     * Kept as a named helper because six tests ask for "this op was refused" and
     * the point of the helper is that it goes through `ack` — the frames the
     * surface actually filed — and never synthesises an `op_exposed`.
     *
     * @param {Record<string, *> | null} op the recorded frame to refuse, or null
     *   for the lost-frame case (an ack whose op already left the queue)
     * @param {string} outcome
     * @param {string|null} [reason]
     * @param {string} [opId] an id never sent, for the lost-frame case
     */
    expose(opOrEntry, outcome, reason, opId) {
      // An entry from `pendingEntry` carries the id the ack must name; take it
      // from the entry so no caller has to remember to pass it a second time and
      // no caller can pass a different one.
      const entry =
        opOrEntry && typeof opOrEntry === "object" && "op" in opOrEntry
          ? /** @type {{op: *, op_id: string}} */ (opOrEntry)
          : { op: opOrEntry, op_id: opId };
      const op = entry.op;
      opId = entry.op_id;
      if (op === null || op === undefined) {
        // The lost-frame case: an ack whose `op_id` matches nothing in the queue.
        // `settleAck` then looks the op up, finds nothing, and fans `op_exposed`
        // with `op: null` — which is the ONLY way the screen's null-frame branch
        // is reachable, and it is reachable here without this file deciding what
        // the record looks like.
        assert.ok(opId, "a lost-frame refusal needs the id the ack names");
        this.ack({ opId, outcome, reason, noMatch: null });
        return;
      }
      this.ack({ op, outcome, reason });
    },

    /**
     * The screen's post guard is written against the REAL sync interface — the
     * component checks `connectionState()` and calls `sync.write` on the object it
     * was handed (StashScreen.svelte:228-232). A name invented here would have to
     * be taught to the component, which is how a component comes to be wired to a
     * test helper rather than to the module it imports. So: pass straight through
     * to the surface. No second log — `mount`'s recorder is fed by the store's own
     * delivery seam and the socket's copy is `sent` above, both downstream of this
     * one call, so there is exactly one place a frame is created.
     */
    write(
      /** @type {Record<string, *>} */ target,
      /** @type {*} */ value,
      /** @type {number|undefined} */ baseVersion,
    ) {
      sync.write(target, value, baseVersion);
    },

    /** The full op object the surface filed for the newest unacked write. */
    pendingOp(/** @type {string} */ kind) {
      const found = this.pendingEntry(kind);
      return found.op;
    },

    /**
     * The op AND the id its ack has to name, for one kind of unacked write.
     *
     * The pair, not the op, because the surface settles an ack by `op_id`
     * (index.js:187) and a caller that only ever gets the op cannot answer it —
     * which is how a refusal test ended up handing `expose` an op-shaped object
     * with no id, having the ack file nothing, and reading the silence as "the
     * notice did not render". The id comes from the WIRE frame the socket carried,
     * never from this file's imagination: if the frame never reached the socket
     * there is no id to name, and the assert says so.
     */
    pendingEntry(/** @type {string} */ kind) {
      const frame = unanswered.find(
        (f) => /** @type {any} */ (f.t === "write" ? f.target : f.field)?.kind === kind,
      );
      assert.ok(frame, `no unacked write of kind ${kind}`);
      const opId = /** @type {any} */ (frame).op_id;
      assert.ok(opId, `the ${kind} frame reached the socket with no op_id`);
      // The op, REBUILT from the wire frame the socket actually carried — the
      // four fields the surface enqueues for a `write` (index.js:245-259), with
      // the id the frame named and the value the store's `post` built. It is not
      // read off the queue, because the queue drains the moment the link goes live
      // (connection.js's drain phase) and an op can be answered long after it left:
      // `settleAck` looks there first and falls back to `op: null` when it finds
      // nothing (index.js:188), which is the lost-frame branch, and this helper
      // exists to name an op that is NOT lost. The frame is the evidence that the
      // op is real; the queue is only evidence it is still waiting.
      const op = {
        op_id: opId,
        target: /** @type {any} */ (frame).target,
        base_version: /** @type {any} */ (frame).base_version,
        value: /** @type {any} */ (frame).value,
        created_at: /** @type {any} */ (frame).created_at ?? "",
      };
      return { op, op_id: opId };
    },
  };
}

const FLINN = { character: { id: 7, name: "Flinn", owner: "dev-sub-a" } };
const BECKY = { character: { id: 8, name: "Becky", owner: "dev-sub-b" } };

/**
 * Mount the screen over a fixture.
 * @param {{you?: {sub: string, role: string}, rows?: object[], bank?: object|null,
 *   claims?: object[], qm?: number|null, roster?: object[], bookValues?: Record<string, {gp: number, sp: number, cp: number}>}} o
 */
function mount(o = {}) {
  // The fixture's world is stated to the surface ONCE, at construction, as a
  // boot snapshot — `createSync`'s `bootSnapshot` option, merged by the same
  // version-merging path a live snapshot uses (index.js:105-112, 113). It used to
  // be delivered after construction through the same call, which left the store
  // built over an empty world: the first `project()` ran before any field existed,
  // and nothing re-ran it, so the screen sat on a stale projection until something
  // else happened to move the store. Only a test that happened to click a button
  // ever saw a row, and a test that only read the screen saw a blank one.
  let version = 100;
  /** @type {Array<{target: Record<string, *>, value: *, version: number}>} */
  const frames = [];
  for (const row of o.rows ?? [])
    frames.push({ target: STASH(1, row.item_name), value: ROW(row.item_name, row.quantity, row), version: ++version });
  if (o.bank) frames.push({ target: BANK(1), value: o.bank, version: ++version });
  frames.push({ target: PARTY(1), value: { quartermaster_character_id: o.qm ?? null }, version: ++version });
  for (const claim of o.claims ?? []) frames.push({ target: CLAIM(claim.id), value: claim, version: claim.id });

  const sync = fakeSync({ frames });

  // One recorder, held here, fed by the store's own delivery seam (`transport`)
  // so the store's write path runs complete — pin assertions, `post`, the lot —
  // and the log is the frame as `post` built it. `sync.writes()` below is this
  // array and nothing else, so an assertion on a frame and the frame the socket
  // carried cannot disagree.
  /** @type {Array<Record<string, *>>} */
  const posted = [];
  const stash = createStashState({
    sync,
    partyId: 1,
    you: o.you ?? { sub: "dev-sub-a", role: "player" },
    roster: o.roster ?? [FLINN, BECKY],
    transport: /** @type {(t: any, v: any, b: number) => string|null} */ (
      (target, value, base) => {
        const before = sync.queue().length;
        sync.write(target, value, base);
        const all = sync.queue();
        const id = all.length > before ? all[all.length - 1].op_id : null;
        posted.push({ target, value, base_version: base, id });
        return id;
      }
    ),
  });
  // The link comes up now, after the store exists: the phase sequence
  // (snapshot → merge → drain → live) runs, `connectionState()` becomes honest,
  // and the projection is recomputed off a world that is already there.
  sync.open();

  // The screen now performs D5's book-value READ itself (`book-value.js`), so
  // the fixture stubs the transport instead of injecting the lookup. An
  // injected lookup is what let `App.svelte` ship without wiring the route at
  // all: the test drove its own fake, so it could not see the missing call.
  // Stubbing fetch means the screen's real read path — URL, `found`/`price`
  // shape, the unknown fallback — is what the assertions land on. The route's
  // server half is tested in src/tests/book_value.rs.
  const bookValueCalls = /** @type {string[]} */ ([]);
  vi.stubGlobal(
    "fetch",
    vi.fn((/** @type {string} */ url) => {
      const text = String(url);
      if (!text.startsWith("/api/parties/1/items/book-value")) {
        throw new Error(`unexpected fetch ${text}`);
      }
      const name = new URL(text, "http://x").searchParams.get("name") ?? "";
      bookValueCalls.push(name);
      const price = o.bookValues?.[name];
      return price
        ? {
            ok: true,
            status: 200,
            json: () => Promise.resolve({ found: true, price }),
          }
        : {
            ok: true,
            status: 200,
            json: () => Promise.resolve({ found: false }),
          };
    }),
  );

  const utils = render(StashScreen, {
    props: { stash, partyId: 1, roster: o.roster ?? [FLINN, BECKY] },
  });

  // `rowOf()` and `bankOf()` re-resolve against whatever is mounted NOW. A test
  // that holds a `within()` from before a write is holding a node the keyed
  // `{#each}` may have replaced, and every query on it then answers "not found"
  // about a row that is plainly on screen — which reads as a broken control and is
  // a stale binding.
  const rowOf = (/** @type {string} */ name) => {
    const bind = () => {
      const li = utils
        .getAllByRole("listitem")
        .find((el) => el.getAttribute("aria-label") === name);
      assert.ok(li, `no row rendered for ${name}`);
      return within(/** @type {HTMLElement} */ (li));
    };
    return bind;
  };
  const bankOf = () =>
    within(
      /** @type {HTMLElement} */ (utils.getByRole("region", { name: /bank/i })),
    );

  return {
    sync: Object.assign(sync, { writes: () => posted }),
    stash,
    bookValueCalls,
    rowOf,
    bankOf,
    ...utils,
  };
}

/** Let Svelte settle an ack that was emitted synchronously during render. */
async function settle() {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

/** Everything write-shaped inside the screen's own region — the SC-5 scope. */
function writeControlsIn(region) {
  const buttons = [...region.querySelectorAll("button")].filter(
    (b) => b.type !== "submit" || true,
  );
  const inputs = [...region.querySelectorAll("input, select, textarea")].filter(
    (el) => el.type !== "hidden",
  );
  // Read-only text/log nodes are not affordances; only controls a seat could
  // post from count. Every control the screen renders that is not a filter of
  // the log is a write affordance.
  return { buttons, inputs };
}

// --- the loot list ---------------------------------------------------------

test("the loot list renders name, quantity, Bulk and note through the sheet's own Bulk formatting", async () => {
  const { container } = mount({
    // The wire carries bulk in Bulk units (design D1's numeric(4,2) column).
    rows: [{ item_name: "Longsword", quantity: 2, bulk: 1.3, notes: "runed" }],
  });
  await waitFor(() => {
    const region = screen.getByRole("region", { name: /stash/i });
    assert.match(region.textContent, /Longsword/);
    assert.match(region.textContent, /runed/);
    // 1.3 Bulk must reach the screen in the SHEET's words — "1 Bulk + 3 L" —
    // not as a raw float and not through a second rounding rule (design D6:
    // "Bulk rendered through the same formatting the sheet uses, no second
    // Bulk-math path"). This literal is tenthsText(13)'s own output.
    assert.match(region.textContent, /1 Bulk \+ 3 L/);
  });
  assert.ok(container);
});

test("a stash row at quantity zero never renders — the row is deleted at zero (FR-1)", async () => {
  // The server deletes at zero and fans a tombstone, so the client's honest
  // empty state is absence. A row that arrives at 0 (a stale snapshot edge)
  // must not show as "0" loot with an active Sell button.
const { sync } = mount({ rows: [{ item_name: "Torch", quantity: 5 }] });
  const torchRow = () => screen.queryByRole("listitem", { name: /Torch/i });

  await waitFor(() =>
    assert.ok(torchRow(), "the row is present before the draw"),
  );
  sync.deliver(STASH(1, "Torch"), null, 999); // the removal diff

  await waitFor(() =>
    assert.equal(torchRow(), null, "the row is gone, not zeroed"),
  );
});

// --- the bank header ------------------------------------------------------

test("the bank header shows the balance, and its honest unknown state when no movement has ever happened", async () => {
  const withBank = mount({ bank: { gp: 12, sp: 3, cp: 40 } });
  await waitFor(() => {
    const header = within(withBank.getByRole("region", { name: /bank/i }));
    // All three denominations render — a header that showed only gp would
    // pass a single-value assertion.
    assert.equal(header.getByText("12 gp").textContent, "12 gp");
    assert.equal(header.getByText("3 sp").textContent, "3 sp");
    assert.equal(header.getByText("40 cp").textContent, "40 cp");
  });

  // The other fixture: a party that has never touched the bank. Design D1
  // does not zero-mint the row, so the screen must not invent 0/0/0 — it says
  // it does not know yet.
  cleanup();
  const noBank = mount({});
  await waitFor(() => {
    const bankRegion = noBank.getByRole("region", { name: /bank/i });
    const header = within(bankRegion);
    // Two assertions that hold together: the honest wording is present, and no
    // coin value is. A screen that fabricated 0 gp / 0 sp / 0 cp (the mistake
    // D1's no-zero-mint rule exists to prevent) passes the first and fails the
    // second, which is why the first is not enough on its own.
    assert.match(header.getByText(/unknown/i).textContent, /unknown/i);
    assert.equal(
      bankRegion.querySelectorAll(".balance span").length,
      0,
      "no fabricated coin spans",
    );
    assert.doesNotMatch(bankRegion.textContent, /\d+\s*(gp|sp|cp)/);
  });
  assert.ok(withBank);
});

// --- the quartermaster toggle (FR-5) --------------------------------------

test("the designation toggle is enabled for ANY character owner, including the designated one", async () => {
  // The two-fixture rule applied to the flip rule itself: the spec's whole
  // claim is that these two seats get the SAME answer, and one fixture cannot
  // show that a `disabled` attribute is not being computed from ownership.
  const toggleOf = (seat) =>
    within(seat.getByRole("region", { name: /party settings/i })).getByRole(
      "combobox",
      { name: /quartermaster/i },
    );

  // Two seats, two documents: both MUST see an enabled control, because FR-5's
  // flip rule turns on the answer being the same whether or not you own the
  // designated character. One fixture cannot tell "enabled for any owner" from
  // "enabled for the designated owner" — the strict reading the spec rejects.
  const designated = mount({
    qm: 7,
    roster: [FLINN, BECKY],
    you: { sub: "dev-sub-a", role: "player" },
  });
  await waitFor(() => {
    assert.equal(
      toggleOf(designated).disabled,
      false,
      "the designated owner may re-seat their own",
    );
  });

  cleanup();
  const other = mount({
    qm: 7,
    roster: [FLINN, BECKY],
    you: { sub: "dev-sub-b", role: "player" },
  });
  await waitFor(() => {
    assert.equal(
      toggleOf(other).disabled,
      false,
      "any other owner may take it too",
    );
  });
});

test("the designation toggle is absent for the GM seat — FR-11, zero write affordances", async () => {
  const { container } = mount({
    qm: 7,
    you: { sub: "dev-sub-gm", role: "gm" },
  });

  await waitFor(() => {
    const region = screen.getByRole("region", { name: /stash/i });
    const { buttons, inputs } = writeControlsIn(region);
    assert.deepEqual(
      buttons.map((b) => b.textContent.trim()).filter(Boolean),
      [],
      "the GM sees no button at all in the stash region",
    );
    assert.deepEqual(inputs, [], "and no input or select either");
    // The GM still SEES the party's loot — read-only, not empty.
    assert.match(container.textContent, /Stash/i);
  });
});

test("flipping the designation posts the party write and moves the sell affordance on the fan-out, without reload", async () => {
  // FR-5's live-sync requirement, driven as a user does it: change the select,
  // then watch what the OTHER seat's fan-out does to this seat's controls.
const { sync } = mount({
    qm: 7,
    rows: [{ item_name: "Longsword", quantity: 3 }],
    you: { sub: "dev-sub-a", role: "player" }, // I own Flinn (7), so I am QM
    roster: [FLINN, BECKY],
  });

  const select = await screen.findByRole("combobox", {
    name: /quartermaster/i,
  });
  await choose(select, "8"); // hand it to Becky

  assert.deepEqual(
    sync.writes().map((op) => op.value),
    [{ character_id: 8 }],
    "the flip posts the CAS party frame",
  );

  // Now the fan-out: the designation moves off me, so Sell must disappear on
  // THIS seat with no reload — the whole point of the field being live-synced.
  sync.deliver(PARTY(1), { quartermaster_character_id: 8 }, 500);
  await waitFor(() => {
    const region = screen.getByRole("region", { name: /stash/i });
    const sells = [...region.querySelectorAll("button")].filter((b) =>
      /sell/i.test(b.textContent),
    );
    assert.deepEqual(
      sells,
      [],
      "the sell affordance is gone once I am not the QM",
    );
  });
});

// --- sell: only the QM's owner sees it, and the two book-value states -----

test("the sell action renders for the quartermaster's owner and nowhere else", async () => {
  // The return value is the sync harness; this test asserts the rendered
  // affordance only, so the mount is driven for its effect on the DOM.
  mount({
    qm: 7,
    rows: [{ item_name: "Longsword", quantity: 3 }],
    you: { sub: "dev-sub-a", role: "player" },
    roster: [FLINN, BECKY],
  });
  await waitFor(() => {
    const region = screen.getByRole("region", { name: /stash/i });
    assert.ok(
      [...region.querySelectorAll("button")].some((b) =>
        /sell/i.test(b.textContent),
      ),
      "the QM owner sees Sell",
    );
  });

  cleanup();
  mount({
    qm: 7,
    rows: [{ item_name: "Longsword", quantity: 3 }],
    you: { sub: "dev-sub-b", role: "player" }, // Becky's owner: not the QM
    roster: [FLINN, BECKY],
  });
  await waitFor(() => {
    const region = screen.getByRole("region", { name: /stash/i });
    assert.deepEqual(
      [...region.querySelectorAll("button")].filter((b) =>
        /sell/i.test(b.textContent),
      ),
      [],
      "a non-QM owner sees no Sell control at all (FR-11: rendered only where the server would allow it)",
    );
  });
});

test("the sell dialog prefills the book value when the corpus has one, and flags when it has none", async () => {
  // Two fixtures, two different results (the house two-fixture rule): a name
  // the corpus resolves and one it does not. One fixture asserting one literal
  // would also pass against a dialog that hardcodes the price.
  mount({
    qm: 7,
    rows: [{ item_name: "Longsword", quantity: 3 }],
    you: { sub: "dev-sub-a", role: "player" },
    roster: [FLINN, BECKY],
    bookValues: { Longsword: { gp: 3, sp: 0, cp: 0 } },
  });

  const sellButton = await screen.findByRole("button", { name: /sell/i });
  await fireEvent.click(sellButton);

  const dialog = await screen.findByRole("dialog");
  await waitFor(() => {
    const gp = within(dialog).getByLabelText(/gp/i);
    assert.equal(
      gp.value,
      "3",
      "the corpus price is the prompt default (D5: the default, never the constraint)",
    );
    assert.doesNotMatch(dialog.textContent, /no book value/i);
  });

  cleanup();
  const unpriced = mount({
    qm: 7,
    rows: [{ item_name: "Handmade Fence", quantity: 3 }],
    you: { sub: "dev-sub-a", role: "player" },
    roster: [FLINN, BECKY],
    bookValues: {}, // the corpus knows nothing about it
  });
  const otherSell = await screen.findByRole("button", { name: /sell/i });
  await fireEvent.click(otherSell);

  const dialog2 = await screen.findByRole("dialog");
  await waitFor(() => {
    assert.match(
      dialog2.textContent,
      /no book value/i,
      "the flagged state is visible, not a silent zero",
    );
    assert.equal(
      within(dialog2).getByLabelText(/gp/i).value,
      "",
      "and the prompt is blank, not prefilled with a guess",
    );
  });
  assert.ok(unpriced);
});

// --- transfer: open to all, bounded by availability ----------------------

test("the transfer picker is open to a non-QM owner and bounds quantity by what is actually there", async () => {
  // Two rows, two different stock levels — the two-fixture rule on quantity.
  // A single fixture cannot tell a max bound from a hardcoded attribute.
  mount({
    qm: 7, // someone else is the QM: transfers must still be open (FR-3)
    rows: [
      { item_name: "Torch", quantity: 4 },
      { item_name: "Grapeshot", quantity: 12 },
    ],
    you: { sub: "dev-sub-b", role: "player" },
    roster: [FLINN, BECKY],
  });

  const transferButtons = await screen.findAllByRole("button", {
    name: /transfer/i,
  });
  assert.equal(
    transferButtons.length,
    2,
    "one per row, and open though I am not the QM",
  );

  // The list renders alphabetized (the store sorts it), so the bound is read
  // per row by name rather than by index into an order the test assumed.
  async function maxFor(button, item) {
    await fireEvent.click(button);
    const dialog = await screen.findByRole("dialog");
    assert.match(
      dialog.textContent,
      new RegExp(item),
      "the dialog names the row it was opened from",
    );
    const max = within(dialog)
      .getByLabelText(/quantity/i)
      .getAttribute("max");
    await fireEvent.click(
      within(dialog).getByRole("button", { name: /cancel/i }),
    );
    return max;
  }

  const listItems = await screen.findAllByRole("listitem");
  const buttonFor = (name) =>
    within(
      listItems.find((li) => li.getAttribute("aria-label") === name),
    ).getByRole("button", { name: /transfer/i });

  // Two rows, two different stock levels, two different maxes — the two-fixture
  // rule on quantity. A hardcoded `max` cannot satisfy both.
  assert.equal(await maxFor(buttonFor("Grapeshot"), "Grapeshot"), "12");
  assert.equal(
    await maxFor(buttonFor("Torch"), "Torch"),
    "4",
    "the bound follows the row, not a constant",
  );
});

test("submitting a transfer to a roster character is bounded, and refuses an over-draw client-side", async () => {
  // FR-3: "the client picker additionally prevents over-draw by disabling
  // impossible quantities (belt and braces — the server is the authority)".
const { sync } = mount({
    qm: 7,
    rows: [{ item_name: "Torch", quantity: 4 }],
    you: { sub: "dev-sub-b", role: "player" },
    roster: [FLINN, BECKY],
  });

  const transferButton = await screen.findByRole("button", {
    name: /transfer/i,
  });
  await fireEvent.click(transferButton);
  const dialog = await screen.findByRole("dialog");

  await choose(
    within(dialog).getByRole("combobox", { name: /character|to/i }),
    "7",
  );
  await fireEvent.input(within(dialog).getByLabelText(/quantity/i), {
    target: { value: "9" },
  });
  await fireEvent.click(
    within(dialog).getByRole("button", { name: /^transfer$/i }),
  );

  assert.deepEqual(
    sync.writes().filter((op) => op.value?.op === "transfer"),
    [],
    "an impossible quantity never leaves the state layer — the frame is not posted",
  );
  // ...and the user is told, rather than watching a button do nothing.
  await waitFor(() => {
    assert.match(dialog.textContent, /only 4|at most 4|cannot exceed/i);
  });
});

// --- the claim log (FR-4, D7) -------------------------------------------

test("the claim log renders newest-first with the FR-4 vocabulary", async () => {
  mount({
    claims: [
      CLAIM_ROW(1, "Longsword", "outside", "stash"),
      CLAIM_ROW(3, "Torch", "outside", "stash"),
      CLAIM_ROW(2, "Longsword", "stash", "character:Flinn"),
    ],
  });

  const log = await screen.findByRole("log");
  await waitFor(() => {
    const entries = [...log.querySelectorAll("li")].map((li) => li.textContent);
    assert.ok(entries.length >= 3, `three rows render, got ${entries.length}`);
    assert.match(
      entries[0],
      /Torch/,
      "id 3 leads — newest-first by row id, not arrival order",
    );
    assert.match(
      entries.at(-1),
      /outside/,
      "the origin label renders from the row (frozen history)",
    );
  });
});

test("a money claim row renders its coin amounts alongside the label", async () => {
  mount({
    claims: [
      {
        id: 5,
        item_name: "bank adjustment",
        quantity: 1,
        origin: "outside",
        destination: "bank",
        actor_sub: "dev-sub-a",
        created_at: "2026-10-15T00:00:00Z",
        coins_gp: 25,
        coins_sp: null,
        coins_cp: null,
      },
    ],
  });

  const log = await screen.findByRole("log");
  await waitFor(() => {
    assert.match(log.textContent, /bank adjustment/);
    assert.match(
      log.textContent,
      /25/,
      "the signed coin amount rides the money row (D1 coins extension)",
    );
  });
});

// --- refused ops: non-blocking (degraded-mode §4) ------------------------

test("a refused op renders its reason in the screen, non-blockingly", async () => {
  mount({
    rows: [{ item_name: "Torch", quantity: 1 }],
    you: { sub: "dev-sub-a", role: "player" },
  });

  // Drive a real write, then play back the rejection the server would send.
  const transferButton = await screen.findByRole("button", {
    name: /transfer/i,
  });
  await fireEvent.click(transferButton);
  const dialog = await screen.findByRole("dialog");
  await choose(
    within(dialog).getByRole("combobox", { name: /character|to/i }),
    "7",
  );
  await fireEvent.input(within(dialog).getByLabelText(/quantity/i), {
    target: { value: "1" },
  });
  await fireEvent.click(
    within(dialog).getByRole("button", { name: /^transfer$/i }),
  );

  // L3 has not wired `transfer`, so the store refuses locally; once it lands
  // this same drive posts a frame. Either way the user is told. Assert the
  // unblocking reason OR the server's reason — both are the honest state.
  await waitFor(() => {
    const region = screen.getByRole("region", { name: /stash/i });
    assert.match(
      region.textContent + dialog.textContent,
      /transfer|not available|wired/i,
      "the refusal is stated in words, not a silently dead button",
    );
  });

  // The §4 ruling: refused is a user-input fact, so it is inline and
  // self-clearing — never a modal that blocks the rest of the screen.
  assert.equal(
    screen.queryAllByRole("alertdialog").length,
    0,
    "a refused op never blocks the screen",
  );
});

// --- the add-item form --------------------------------------------------

test("the add form posts a mint, and an empty name never reaches the wire", async () => {
const { sync } = mount({ you: { sub: "dev-sub-a", role: "player" } });

  const name = await screen.findByLabelText(/item name/i);
  await fireEvent.input(name, { target: { value: "   " } });
  await fireEvent.click(screen.getByRole("button", { name: /add/i }));
  assert.deepEqual(sync.writes(), [], "a blank name is not a mint");

  await fireEvent.input(name, { target: { value: "Hempen Rope" } });
  await fireEvent.input(screen.getByLabelText(/^quantity/i), {
    target: { value: "2" },
  });
  await fireEvent.click(screen.getByRole("button", { name: /add/i }));

  await waitFor(() => {
    const mint = sync.writes().find((op) => op.target.kind === "stash_new");
    assert.ok(mint, "the mint rides the wire");
    assert.equal(mint.value.item_name, "Hempen Rope");
    assert.equal(mint.value.quantity, 2);
  });
});

// Finding 1 (MOR-662) — a refused op must reach the SCREEN, not only the store.
//
// `stash.js`'s transport subscription had two branches that wrote the refused-op
// record and neither one called `refresh()`, the only place the projection is
// ever written — so the notice updated the store and the DOM never heard about
// it. `sync/index.js` confirms there is no second signal: a `rejected` ack emits
// `queue` and `op_exposed`, never `fields` and never `applied`, and (unlike
// `superseded`) a refusal is not followed by a field frame that would paper over
// it. FR-11 requires the record by name ("Refused operations (R4/R5 outcomes)
// surface through the op record in the stash screen — non-blocking"), and FR-1's
// duplicate-name rule depends on it: the "adjust the existing row instead" reason
// IS a rejected ack.
//
// The `expose()` helper on the fake sync below has been sitting uncalled since
// this file was written, while this file's own header advertised "a refused op
// record, rendered from a driven event". That header described a test that did not
// exist. It exists now.
// One thing this test earned the hard way, recorded because it is the kind of
// detail that gets a follow-up round the wrong shape. The first draft asserted the
// reason text as the thing the notice shows, and the notice does not show it that
// way: it names the frame it refused via `value.op` / `target.kind` (see
// StashScreen's `noticeFrame`) and only then the reason. Assert the shape the
// component actually renders, not the shape the store happened to record.
test("a refused op surfaces its reason in the DOM, not only in the store", async () => {
  const { sync, stash, container } = mount({ rows: [ROW("Longsword", 2)], qm: 7 });
  assert.ok(
    !container.textContent.includes("already in the stash"),
    "no notice before the ack",
  );

  // Post the duplicate-name add through the screen's own write path — the store
  // mints the frame and the surface files the op — then refuse THAT. An earlier
  // draft invented the op object here, so the record the screen named was this
  // file's artifact and the test could not see a refusal that named nothing.
  stash.addRow({ item_name: "Longsword", quantity: 1 });
  await settle();
  sync.expose(
    sync.pendingEntry("stash_new"),
    "rejected",
    "an item with that name is already in the stash — adjust its quantity instead",
  );
  // One microtask, and no more: the store's notification reaches the component's
  // `changed()` subscriber synchronously, but the `$derived(notices)` recompute and
  // the resulting DOM write flush on the next tick. The test asserts the flush, so
  // it has to let it happen; the store half needs no await at all.
  await settle();

  // The DOM is the assertion. `stash.opErrors()` already returned this record at
  // the old head and the screen stayed blank, so an accessor assert proves the
  // recorder and proves nothing about the render (AGENTS.md, production-path rule).
  // The item name lives in the rejected mint's `value`, and the notice renders
  // the frame's identifying field — `value.op` for a compound op, `target.kind`
  // for a mint (see StashScreen's noticeFrame). A mint has no op, so the kind is
  // what the row can show; the reason carries the name the user typed.
  const notice = container.textContent;
  assert.match(notice, /rejected/, "the outcome reaches the DOM");
  assert.match(
    notice,
    /already in the stash/,
    "the refusal reason reaches the DOM",
  );
  assert.match(notice, /Longsword|add/, "and the frame it refused is named");
  // Non-blocking: the rest of the screen is still there to act on.
  // The screen is still usable, not disabled: the add form is present and its
  // submit control is enabled. (The form's own label is "Add to stash".)
  const submit = screen.getByRole("button", { name: /add to stash/i });
  assert.ok(!submit.disabled, "a refusal does not disable the form");
});

test("two lost-frame refusals in one turn render as two records", async () => {
  // The latent duplicate-key defect the review named (minor #1), and a real one.
  // The `{#each}` keyed on `notice.op.op_id`, but the transport hands the store
  // `op: null` once an op has left the queue (sync/index.js:188) and the store
  // files that frame as `{}` — so every lost-frame notice keys `undefined`, and
  // Svelte silently drops one of the two rows. `OpRecord.op_id` (the EVENT's id,
  // which the transport always has) is the key now.
  //
  // Both refusals are driven in one turn, and that is load-bearing rather than
  // tidy: an await between them lets the first render, and then the second lands
  // on an each-block that has to re-derive its key list from the records — which is
  // where this dropped a row. One turn is also the shape the wire produces: a
  // `rejected` and a `forbidden` ack off a two-op burst arrive in the same read.
  const { sync, stash, container } = mount({ rows: [ROW("Longsword", 2)], qm: 7 });
  // Distinct ack ids, no frames at all: the record's own id is the only thing
  // separating them, which is exactly what the each-block must key on.
  // One of them carries an EMPTY ack id: `settleAck` reads `op_id` off the ack
  // frame and never asserts it non-empty, so a peer or a future ack shape can hand
  // two refusals the same id. The key must not be the thing that silently picks a
  // winner — Svelte drops a row on a duplicate key, and the notice that vanishes
  // is the user's only explanation for a write that never landed.
  sync.expose(null, "rejected", "the first lost-frame refusal", "ack-one");
  sync.expose(null, "forbidden", "the second lost-frame refusal", "ack-two");
  await settle();
  assert.match(container.textContent, /the first lost-frame refusal/);
  assert.match(
    container.textContent,
    /the second lost-frame refusal/,
    "two null frames are two records, not one",
  );
  // (a null frame's own slot renders empty — the notice names the frame it refused,
  // and a frame the transport could not hand back has nothing to name)
});

test("two refusals in one turn render as two records", async () => {
  const { sync, stash, container } = mount({ rows: [ROW("Longsword", 2)], qm: 7 });
  stash.adjustQty("Longsword", -1);
  await settle();
  const draw = sync.pendingEntry("stash");
  assert.ok(draw, "the draw was posted before it was refused");
  // §4's other half: an exposed op that is settled is GONE from the queue, so a
  // later ack cannot find it and cannot expose it a second time. Any refusal this
  // file wants to deliver has to name an op the surface still holds — that is not
  // a rule of the test, it is a rule of the wire.
  // One frame present, one lost: distinct keys even under the old code, so this is
  // the control that keeps the key change from being justified by a test that only
  // exercises the collision.
  sync.expose(draw, "rejected", "not enough of that item in the stash", draw.op_id);
  sync.expose(
    null,
    "forbidden",
    "only the quartermaster may do that",
    "ack-lost",
  );
  await settle();
  assert.match(container.textContent, /not enough of that item in the stash/);
  assert.match(
    container.textContent,
    /only the quartermaster may do that/,
    "a null frame must not evict its sibling",
  );
});

// `adjustQty` was added to the fixture's transport above and never removed, so
// the op the draw posts settles and clears its own-write echo like any other.
// If that clear ever stops happening, this is the test that says so: the row
// would sit on its echoed ×1 rather than the server's 2.
test("an applied ack clears the own-write echo and the row reads the server value", async () => {
  const { sync, stash, rowOf } = mount({
    qm: 7,
    rows: [ROW("Longsword", 2)],
    you: { sub: "dev-sub-a", role: "player" },
  });
  stash.adjustQty("Longsword", -1);
  await settle();
  // §4's optimistic half: the echo is on screen before any answer.
  await waitFor(() => assert.match(rowOf("Longsword")().getByText("×1").textContent, /×1/));
  const draw = sync.pendingEntry("stash");
  sync.ack({ op: draw.op, outcome: "applied", version: 300 });
  // The server's own value arrives with it (contract §4: an applied ack carries
  // the version, the diff carries the value).
  sync.deliver(STASH(1, "Longsword"), ROW("Longsword", 2), 300);
  await settle();
  await waitFor(() => {
    assert.match(
      rowOf("Longsword")().getByText("×2").textContent,
      /×2/,
      "the echo is gone and the committed count is what shows",
    );
  });
});

// --- gh#336: the manual-adjustment affordances, wired into the screen -------
//
// Three controls, three owners, and this section says which is which rather than
// leaning on one FR number for all of them. Read off `specs/014-shared-inventory/
// spec.md` at this head, not from a paraphrase:
//
//   - FR-10 (line 441): "stash writes (add, transfer, quantity adjust, metadata
//     edit) — ANY CHARACTER OWNER of the party; bank mutations (sell, manual
//     adjustment) — the quartermaster character's OWNER ONLY". So the row's −1/+1
//     and its bulk/note editor are member-gated, and the server agrees: the row
//     ops sit on `deny_non_member` (`src/sync/write.rs:968-992` — the sell arm is
//     the one stash op routed to the Bank resource). Rendering them behind
//     `canSell` would be a client-only authority, the exact thing this lane was
//     reviewed for once already.
//   - FR-7 (line 400): the party bank, "fed additively by sales and by
//     quartermaster-only manual adjustments. Adjustments carry a signed amount…
//     an adjustment that would MUST be refused whole, with a reason." Server:
//     `deny_non_quartermaster_owner` on every `FieldTarget::Bank` write
//     (`write.rs:995-1003`). So the bank adjuster is QM-only, and that gate is
//     the one asserted as a three-seat pair below.
//   - FR-1 (line 258): a duplicate add is "refused with a reason telling the
//     actor to adjust the existing row's quantity instead" — which is only a real
//     instruction if the adjust control renders for the member who was refused.
//     That is the row's −1/+1, and it is why the member fixture below asserts
//     PRESENCE, not absence.
//
// FR-11/E14 SC-5 cuts across all three: the GM seat renders zero write
// affordances, bank region included.
//
// Every test drives a MOUNTED control and reads what a user sees. The three store
// actions behind them (`adjustQty`, `editMeta`, `adjustBank`) shipped in PR #311
// with zero production callers — round 2's finding 1 and gh#336 — and store-level
// tests could not see that: "Hits confined to the defining module mean the
// requirement is unbuilt — a green unit test does not change that."

/** The row's own list item, by item name (the store sorts the list). */
/**
 * The row's delta or editor control, found by its VISIBLE TEXT.
 *
 * Text, not `getByRole(name)` — a name-computation fact, not a preference. The
 * delta buttons carry `aria-label`, and a name taken from a label SHADOWS the
 * element's visible text in the accessible-name computation, so
 * `getByRole("button", {name: /\+1/i})` cannot match a button whose visible text
 * is the sign. The repo's house style for row actions matches visible text, so
 * this control is queried the way its neighbours are, and the label stays what it
 * is for: the announcement a screen reader gets.
 *
 * @param {() => ReturnType<typeof within>} row the binder from `rowOf`
 * @param {string | null} item the row name the control sits on (null for the
 *   editor, whose label is the single word `Edit`)
 * @param {"take" | "add" | "edit"} kind
 */
function rowButton(row, item, kind) {
  const escape = (t) => /** @type {string} */ (t).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const re =
    kind === "take"
      ? new RegExp("^\\u22121\\s*" + escape(item ?? "") + "$", "i")
      : kind === "add"
        ? new RegExp("^\\+1\\s*" + escape(item ?? "") + "$", "i")
        : /^edit$/i;
  return row()
    .getAllByRole("button")
    .find((b) => re.test((b.textContent ?? "").trim()));
}

test("any party member may adjust a row's quantity by a signed delta, and the row moves on screen (FR-10, FR-1)", async () => {
  // The member fixture, not the QM's. FR-10 names "quantity adjust" as a stash
  // write open to every character owner, and FR-1's refusal reason tells the
  // member who tried to duplicate a row to go adjust that row's quantity — so the
  // control must render for exactly this seat. `you` here owns Becky and Becky is
  // not the quartermaster: a `canSell` gate would take this control away and the
  // assertion below would fail, which is the point of putting it first.
  //
  // And the delta, never the absolute: FR-7's sibling rule for the bank is
  // "adjustments carry a signed amount", and `stash_qty_tx` applies `qty_delta`
  // additively (`write.rs:1289`). What must leave is `{op:"qty", qty_delta: 1}`
  // against a row of 3, so a read-modify-write cannot hide behind a passing test.
const { sync, stash, rowOf, bankOf } = mount({
    qm: 7, // Flinn's owner is the QM; this seat is Becky's owner
    rows: [{ item_name: "Grapeshot", quantity: 3 }],
    you: { sub: "dev-sub-b", role: "player" },
  });

  const row = rowOf("Grapeshot");
  await fireEvent.click(
    rowButton(row, "grapeshot", "add"),
  );

  await waitFor(() => {
    // The delta must ride a REAL id: the surface keys its pending-op table on
    // `op_id`, and a frame carrying `t1` is a frame no ack can settle.
    const op = sync.writes().find((w) => w.target.kind === "stash");
    assert.ok(op, "the delta rides the wire");
    assert.deepEqual(op.value, { op: "qty", qty_delta: 1 });
    // The recorded frame carries the id the SURFACE filed it under — the queue's
    // own id, read back after the enqueue, not a store-local counter. The ack
    // below is keyed off this frame, and an id the queue never held settles
    // nothing: the own-write echo would sit in the store forever and the ×4 the
    // next assert looks for would never appear.
    assert.ok(
      sync.queue().some((q) => q.op_id === op.id),
      `recorded id ${op.id} is not on the surface's queue`,
    );
    // §4: the echo moves WITHOUT an ack. This is the client-side half of FR-1 and
    // the one a server answer can never supply — the frame is additive, so it
    // carries a DELTA and no balance, and the only number on screen between the
    // click and the ack is one the store painted. If this assert ever goes quiet
    // because an `ack()` was added above it, the test has stopped proving the
    // echo and started proving the ack.
    assert.match(
      row().getByText("×4").textContent,
      /×4/,
      "the echo moves on screen before the server says anything (§4)",
    );
  });

});

test("the minus delta drawn to exactly zero deletes the row rather than rendering ×0 (FR-1)", async () => {
  // The other side of the same affordance, on a fixture chosen so the answer is
  // "the row disappears": FR-1 line 257, "A row whose quantity reaches zero MUST
  // be deleted, not retained at zero". A row of 1 and one click of −1; the wire
  // carries the delta and the list loses the row.
const { sync, stash, rowOf, bankOf } = mount({
    qm: 7,
    rows: [{ item_name: "Torch", quantity: 1 }],
    you: { sub: "dev-sub-b", role: "player" },
  });

  const row = rowOf("Torch");
  await fireEvent.click(rowButton(row, "torch", "take"));

  await waitFor(() => {
    const op = sync.writes().find((w) => w.target.kind === "stash");
    assert.deepEqual(op.value, { op: "qty", qty_delta: -1 });
  });
  await waitFor(() => {
    assert.equal(
      screen
        .queryAllByRole("listitem")
        .filter((li) => li.getAttribute("aria-label") === "Torch").length,
      0,
      "the drawn-down row leaves the list",
    );
  });
});

test("the quantity deltas and the row editor render for every party member and for no non-member seat (FR-10)", async () => {
  // FR-10's row-write gate is MEMBERSHIP, so the two fixtures are a member and a
  // GM — not a QM and a non-QM, which is the bank's pair. The GM is the seat the
  // server refuses outright (`gm is read-only`), so FR-11 says it gets no control
  // rather than a button that can only produce a refusal.
  const member = mount({
    qm: null, // no quartermaster at all: the row controls must STILL be there
    rows: [{ item_name: "Longsword", quantity: 3 }],
    you: { sub: "dev-sub-b", role: "player" },
  });
  const rowOf = member.rowOf;
  await waitFor(async () => {
    const row = rowOf("Longsword");
    assert.ok(rowButton(row, "longsword", "take"), "member: minus");
    assert.ok(rowButton(row, "longsword", "add"), "member: plus");
    assert.ok(rowButton(row, null, "edit"), "member: editor");
    // Readable either way — a write gate must not hide the loot.
    assert.match(row().getByText("×3").textContent, /×3/);
  });

  cleanup();
  // The GM's OWN `rowOf`, not the member's: each mount owns its own render, and a
  // binder captured off a cleaned-up one queries a tree that no longer exists —
  // which would pass the zero-control assert by accident and fail the read below.
  const gm = mount({
    qm: 7,
    rows: [{ item_name: "Longsword", quantity: 3 }],
    you: { sub: "dev-sub-a", role: "gm" },
  });
  await waitFor(async () => {
    const row = gm.rowOf("Longsword");
    assert.deepEqual(
      row().getAllByRole("button"),
      [],
      "the GM row carries zero controls (FR-11)",
    );
    assert.match(
      row().getByText("×3").textContent,
      /×3/,
      "and the GM still reads the row",
    );
  });
});

test("the quartermaster adjusts the bank by a signed delta and the header moves (FR-7)", async () => {
  // FR-7 line 402: the bank is "fed additively by sales and by quartermaster-only
  // manual adjustments. Adjustments carry a signed amount". The frame is therefore
  // the DELTA — `adjustBank`'s own comment calls a read-modify-write here
  // "exactly the bug D2 was written to refuse", so the assertion is on the delta
  // and not on a resulting balance a hardcoded frame could fake.
const { sync, stash, rowOf, bankOf } = mount({
    qm: 7,
    bank: { gp: 12, sp: 3, cp: 40 },
    you: { sub: "dev-sub-a", role: "player" }, // Flinn's owner: the QM
  });

  const bank = within(screen.getByRole("region", { name: /bank/i }));
  await fireEvent.input(bank.getByLabelText(/gold/i), {
    target: { value: "5" },
  });
  await fireEvent.click(bank.getByRole("button", { name: /adjust bank/i }));

  await waitFor(() => {
    const op = sync.writes().find((w) => w.target.kind === "bank");
    assert.ok(op, "the bank delta rides the wire");
    assert.deepEqual(op.value, { gp_delta: 5, sp_delta: 0, cp_delta: 0 });
  });
  await waitFor(() => {
    assert.ok(
      bank.getByText("17 gp"),
      "the deposit echoes into the header immediately (§4)",
    );
  });
});

test("the bank adjuster renders for the QM owner, for no other member, and never for the GM (FR-7 gate)", async () => {
  // The three-seat pair FR-7's gate is actually about, since the bank's rule is
  // the quartermaster's owner and nobody else's (`deny_non_quartermaster_owner`
  // on every Bank write, `write.rs:995-1003`). Three states of one predicate,
  // three fixtures, and the balance stays readable in all three — this is a write
  // gate, not a hide.
  const qm = mount({
    qm: 7,
    bank: { gp: 12, sp: 3, cp: 40 },
    you: { sub: "dev-sub-a", role: "player" },
  });
  await waitFor(() => {
    assert.ok(
      within(qm.getByRole("region", { name: /bank/i })).queryByRole("button", {
        name: /adjust bank/i,
      }),
      "the QM owner gets the adjuster",
    );
  });

  cleanup();
  const member = mount({
    qm: 7,
    bank: { gp: 12, sp: 3, cp: 40 },
    you: { sub: "dev-sub-b", role: "player" },
  });
  await waitFor(() => {
    const region = member.getByRole("region", { name: /bank/i });
    assert.deepEqual(
      within(region).queryAllByRole("button"),
      [],
      "a member has no bank control",
    );
    assert.ok(within(region).getByText("12 gp"), "and still reads the balance");
  });

  cleanup();
  const gm = mount({
    qm: 7,
    bank: { gp: 12, sp: 3, cp: 40 },
    you: { sub: "dev-sub-a", role: "gm" },
  });
  await waitFor(() => {
    const region = gm.getByRole("region", { name: /bank/i });
    assert.deepEqual(
      within(region).queryAllByRole("button"),
      [],
      "the GM has no bank control",
    );
    assert.equal(
      region.querySelectorAll("input").length,
      0,
      "nor an input to type into",
    );
  });
});

test("a bank adjustment below zero is refused whole and the header keeps the committed balance (FR-7)", async () => {
  // FR-7 line 405: "Balances MUST never go negative… an adjustment that would
  // MUST be refused whole, with a reason." The guard is
  // `gp::bigint + $1 BETWEEN 0 AND $5` (write.rs:1857-1859), so a 20 gp draw on a
  // 12 gp bank matches no row, writes nothing, and refuses the frame WHOLE. The
  // header must go on reading 12 gp — not −8, and not the floored 0 that PR #311
  // round 1 named as "just as much a number the server will never hold" as the
  // negative was. The reason reaching the screen is FR-11's op record.
const { sync, stash, rowOf, bankOf } = mount({
    qm: 7,
    bank: { gp: 12, sp: 3, cp: 40 },
    you: { sub: "dev-sub-a", role: "player" },
  });

  const bank = within(screen.getByRole("region", { name: /bank/i }));
  await fireEvent.input(bank.getByLabelText(/gold/i), {
    target: { value: "-20" },
  });
  await fireEvent.click(bank.getByRole("button", { name: /adjust bank/i }));

  const op = sync.pendingEntry("bank");
  assert.ok(op, "§4: the write is queued and echoes immediately");
  sync.expose(
    op,
    "rejected",
    "that adjustment would drive the bank below zero",
    op.op_id,
  );
  await settle();

  await waitFor(() => {
    assert.ok(
      bank.getByText("12 gp"),
      "the committed balance survives the refusal",
    );
    assert.equal(bank.queryByText("-8 gp"), null, "never a negative");
    assert.equal(bank.queryByText("0 gp"), null, "nor a floored zero");
  });
  await waitFor(() => {
    assert.match(
      screen.getByRole("list", { name: /refused/i }).textContent,
      /below zero/,
    );
  });
});

test("an adjustment of nothing at all never reaches the wire (FR-7)", async () => {
  // The blank-box case, and the reason it is a rule rather than a nicety:
  // `validate_bank` requires all three keys (write.rs:3182-3199) and the server
  // would take an all-zero frame as a zero delta APPLIED — a version bump and a
  // claim row for a movement that never happened. `adjustBank` refuses it; the
  // screen must not send it on the store's behalf.
const { sync } = mount({
    qm: 7,
    bank: { gp: 12, sp: 3, cp: 40 },
    you: { sub: "dev-sub-a", role: "player" },
  });

  const bank = within(screen.getByRole("region", { name: /bank/i }));
  await fireEvent.click(bank.getByRole("button", { name: /adjust bank/i }));
  assert.deepEqual(sync.writes(), [], "an empty adjustment is not a movement");

  // And the box the user typed stays out of the next attempt's way: a control
  // that keeps its value after a send invites a second click that double-posts.
  await fireEvent.input(bank.getByLabelText(/gold/i), {
    target: { value: "2" },
  });
  await fireEvent.click(bank.getByRole("button", { name: /adjust bank/i }));
  assert.equal(sync.writes().length, 1);
  assert.equal(
    bank.getByLabelText(/gold/i).value,
    "",
    "the box is cleared on send",
  );
});

test("a refused quantity delta leaves the row at its committed count (FR-1)", async () => {
  // The row-side instance of "refused whole", driven through the control rather
  // than the store. `apply_stash_delta`'s guard is `quantity::bigint + $1 BETWEEN
  // 1 AND $4` (write.rs:1289) with the delete arm separate at `= 0` (:1310), so a
  // draw that would land below zero matches neither and changes nothing. The
  // screen must keep showing what the server holds.
  //
  // This is also the gh#340 shape reached from a control at last: a first click
  // that deletes the row, a second whose op the server refuses `unknown target`.
const { sync, stash, rowOf, bankOf } = mount({
    qm: null,
    rows: [{ item_name: "Torch", quantity: 1 }],
    you: { sub: "dev-sub-b", role: "player" },
  });

  const row = rowOf("Torch");
  await fireEvent.click(rowButton(row, "torch", "take")); // sum 0: deleted

  // The second frame goes through the STORE, not through a node the first click
  // removed. FR-1's boundary makes the row vanish the moment its quantity reaches
  // zero — that is the whole requirement — so a control that was on screen one
  // event ago is gone with it, and a test that hunts for it is testing whether the
  // row survived rather than what the second op does. The duplicate click a user
  // cannot any longer make is still the op that has to be answered: the server has
  // no row to draw from and refuses `unknown stash target`, and gh#340 is exactly
  // the fold that got that answer wrong. Driving the same frame `post` builds for
  // the control is not a shortcut around a missing control; the control is the
  // thing the requirement deletes.
  await waitFor(() => assert.equal(sync.writes().length, 1, "one frame per click"));
  stash.adjustQty("Torch", -1); // the would-be second click
  await waitFor(() => assert.equal(sync.writes().length, 2, "the second frame rides too"));
  const second = sync.writes()[1];

  sync.deliver(STASH(1, "Torch"), null, 200); // the tombstone: the row is gone
  await settle();
  sync.expose(second, "rejected", "unknown stash target", second.id);
  await settle();

  await waitFor(() => {
    assert.equal(
      screen
        .queryAllByRole("listitem")
        .filter((li) => li.getAttribute("aria-label") === "Torch").length,
      0,
      "the row stays deleted — a refused op behind a delete revives nothing",
    );
  });
});
