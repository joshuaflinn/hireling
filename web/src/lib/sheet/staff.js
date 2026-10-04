// Staff Nexus charge math (E6 spec §2.3, Q1 ruling) — pure, tested.
// The daily row: {staff_charge_rank 0..10, staff_spent ≥0, drain_used}.
// Charges left = rank − spent, floored at zero.

/**
 * @param {{ staff_charge_rank?: number, staff_spent?: number, drain_used?: boolean }} daily
 * @returns {number} charges left in the makeshift staff
 */
export function chargesLeft(daily) {
  const rank = daily.staff_charge_rank ?? 0;
  const spent = daily.staff_spent ?? 0;
  return Math.max(0, rank - spent);
}

/**
 * The whole-row write after spending or restoring one charge: spent moves
 * within [0, rank] — the pips model restoring too, so a misclick is not a
 * GM call.
 * @param {{ staff_charge_rank?: number, staff_spent?: number, drain_used?: boolean }} daily
 * @param {number} spentTarget
 * @returns {{ staff_charge_rank: number, staff_spent: number, drain_used: boolean }}
 */
export function setSpent(daily, spentTarget) {
  const rank = daily.staff_charge_rank ?? 0;
  return {
    staff_charge_rank: rank,
    staff_spent: Math.min(rank, Math.max(0, Math.round(spentTarget))),
    drain_used: Boolean(daily.drain_used),
  };
}
