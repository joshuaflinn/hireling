import { expect, test } from "vitest";

// Deliberately red — #92 deliverable 4 / #89 acceptance: prove a broken web
// test turns the required `gate` context red. Reverted in the next commit.
test("deliberately red: the gate context must fail this PR", () => {
	expect(1).toBe(2);
});
