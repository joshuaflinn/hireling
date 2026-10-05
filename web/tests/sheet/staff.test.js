import { test } from 'vitest';
import assert from 'node:assert/strict';

import { chargesLeft, setSpent } from '../../src/lib/sheet/staff.js';

test('charges left is rank minus spent, floored at zero', () => {
  assert.equal(chargesLeft({ staff_charge_rank: 3, staff_spent: 1 }), 2);
  assert.equal(chargesLeft({ staff_charge_rank: 3, staff_spent: 3 }), 0);
  assert.equal(chargesLeft({ staff_charge_rank: 3, staff_spent: 5 }), 0, 'never negative');
  assert.equal(chargesLeft({}), 0, 'no staff = no charges');
});

test('setSpent clamps within the rank and preserves the rest of the row', () => {
  const daily = { staff_charge_rank: 3, staff_spent: 0, drain_used: true };
  assert.deepEqual(setSpent(daily, 2), { staff_charge_rank: 3, staff_spent: 2, drain_used: true });
  assert.deepEqual(
    setSpent(daily, 9),
    { staff_charge_rank: 3, staff_spent: 3, drain_used: true },
    'spent never exceeds the rank',
  );
  assert.deepEqual(
    setSpent(daily, -1),
    { staff_charge_rank: 3, staff_spent: 0, drain_used: true },
    'spent never goes below zero',
  );
});
