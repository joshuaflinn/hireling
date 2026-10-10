// Inventory rollup math (E6 design §5) — pure, tested.
// Bulk lives in tenths everywhere: L=1, one Bulk=10, "—"/null=0. Container
// groups sum member item bulk × qty; containers flagged extradimensional
// are excluded from every total (their contents never count toward Bulk);
// the character total excludes extradimensional contents too.

/**
 * @typedef {Object} InventoryItemView
 * @property {string} name
 * @property {number} qty effective quantity (base + live delta)
 * @property {number|null} bulkTenths null = corpus gap → rendered "—"
 * @property {string} bulkText
 * @property {string[]} traits corpus trait chips
 * @property {boolean} invested
 */

/**
 * @typedef {Object} InventoryGroupView
 * @property {string | null} name null = the top level
 * @property {boolean} extradimensional
 * @property {InventoryItemView[]} items
 * @property {number | null} bulkTenths null for extradimensional groups
 * @property {string} bulkText
 */

/**
 * @param {*} baseSheet the normalized sheet (equipment + containers +
 * weapons + armor)
 * @param {Record<string, number | null>} itemBulk the bootstrap bulk map
 * @param {Record<string, string[]>} itemTraits the bootstrap trait map
 * @param {(name: string) => number} qtyOf effective quantity per item name
 * @returns {{ groups: InventoryGroupView[], totalTenths: number, totalText: string }}
 */
export function inventoryView(baseSheet, itemBulk, itemTraits, qtyOf) {
  const extradimensional = new Set(
    (baseSheet.containers ?? [])
      .filter((/** @type {any} */ container) => container.extradimensional)
      .map((/** @type {any} */ container) => container.name),
  );

  /** @type {Map<string | null, InventoryItemView[]>} */
  const grouped = new Map();
  for (const item of baseSheet.equipment ?? []) {
    const tenths = itemBulk[item.name] ?? null;
    const view = {
      name: item.name,
      qty: qtyOf(item.name),
      bulkTenths: tenths,
      bulkText: tenthsText(tenths),
      traits: itemTraits[item.name] ?? [],
      invested: Boolean(item.invested),
    };
    const key = item.container ?? null;
    if (!grouped.has(key)) grouped.set(key, []);
    /** @type {InventoryItemView[]} */ (grouped.get(key)).push(view);
  }

  const keys = [...grouped.keys()].sort((a, b) => {
    if (a === null) return -1; // top level first
    if (b === null) return 1;
    return a.localeCompare(b);
  });

  let total = 0;
  const groups = keys.map((name) => {
    const items = /** @type {InventoryItemView[]} */ (grouped.get(name));
    const extra = name !== null && extradimensional.has(name);
    const sum = extra
      ? null
      : items.reduce(
          /** @type {(acc: number, item: InventoryItemView) => number} */
          (acc, item) => acc + (item.bulkTenths ?? 0) * item.qty,
          0,
        );
    if (sum !== null) total += sum;
    return {
      name,
      extradimensional: extra,
      items,
      bulkTenths: sum,
      bulkText: extra ? 'excluded' : tenthsText(sum ?? 0),
    };
  });

  // Carried and worn both count (PF2e Bulk): the weapon in hand and the
  // armor on their back join the character total. Same tenths map, same
  // gap degrade (null → 0); stowed (non-worn) armor never counts. These
  // rows render in the strikes/AC panes, not the inventory list, so they
  // add to the total without adding list rows.
  const carriedOnBody = [...(baseSheet.weapons ?? [])]
    .concat((baseSheet.armor ?? []).filter((/** @type {any} */ piece) => piece.worn))
    .reduce(
      (acc, item) => acc + (itemBulk[item.name] ?? 0) * (item.qty ?? 1),
      0,
    );
  total += carriedOnBody;

  return { groups, totalTenths: total, totalText: tenthsText(total) };
}

/**
 * Bulk text from tenths — the shared formatter, with null (a corpus gap)
 * rendered "—" and zero "negligible" (prototype `bulkTxt` semantics).
 * @param {number | null} tenths
 * @returns {string}
 */
export function tenthsText(tenths) {
  if (tenths === null || tenths === undefined) return '—';
  const bulk = Math.floor(tenths / 10);
  const light = tenths % 10;
  if (bulk && light) return `${bulk} Bulk + ${light} L`;
  if (bulk) return `${bulk} Bulk`;
  if (light) return `${light} L`;
  return 'negligible';
}
