import { expect, test } from "vitest";

// Deliberately red — #92/#89 acceptance proof that the repo-guards job
// actually gates the vitest suite. Reverted in the next commit.
test("deliberately red: repo-guards must fail this PR", () => {
	expect(1).toBe(2);
});
