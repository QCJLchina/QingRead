import assert from "node:assert/strict";
import test from "node:test";
import { calculateBookProgress, clampPage, pageFromRatio, ratioFromPage } from "./pagination.ts";

test("clampPage keeps page indexes inside the layout bounds", () => {
  assert.equal(clampPage(-2, 4), 0);
  assert.equal(clampPage(2.8, 4), 2);
  assert.equal(clampPage(99, 4), 3);
  assert.equal(clampPage(4, 0), 0);
});

test("page and ratio conversion preserves mode-switch position", () => {
  assert.equal(pageFromRatio(0, 5), 0);
  assert.equal(pageFromRatio(0.5, 5), 2);
  assert.equal(pageFromRatio(1, 5), 4);
  assert.equal(ratioFromPage(2, 5), 0.5);
  assert.equal(ratioFromPage(3, 1), 0);
});

test("calculateBookProgress includes the current page within the chapter", () => {
  assert.equal(calculateBookProgress(0, 4, 0, 2), 13);
  assert.equal(calculateBookProgress(3, 4, 1, 2), 100);
  assert.equal(calculateBookProgress(0, 0, 0, 1), 0);
});
