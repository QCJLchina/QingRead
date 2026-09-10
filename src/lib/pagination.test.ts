import assert from "node:assert/strict";
import test from "node:test";
import {
  calculateBookProgress,
  clampPage,
  estimateBookProgress,
  pageCountFromWidth,
  pageFromOffset,
  pageFromRatio,
  ratioFromPage,
} from "./pagination.ts";

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

test("page count comes from total column width and page stride", () => {
  assert.equal(pageCountFromWidth(0, 500), 1);
  assert.equal(pageCountFromWidth(500, 500), 1);
  assert.equal(pageCountFromWidth(501, 500), 2);
  assert.equal(pageCountFromWidth(1500, 500), 3);
  assert.equal(pageCountFromWidth(1200, 0), 1);
});

test("page index comes from the horizontal offset of a character", () => {
  assert.equal(pageFromOffset(10, 0, 500), 0);
  assert.equal(pageFromOffset(500, 0, 500), 1);
  assert.equal(pageFromOffset(1040, 40, 500), 2);
  assert.equal(pageFromOffset(10, 0, 0), 0);
});

test("book progress uses chapter weights and the ratio inside the chapter", () => {
  assert.equal(estimateBookProgress(0, 4, 0), 0);
  assert.equal(estimateBookProgress(0, 4, 0.5), 13);
  assert.equal(estimateBookProgress(3, 4, 1), 100);
  assert.equal(estimateBookProgress(0, 0, 0.5), 0);
  // 越界的章节序号和比例都会被夹住，不会出现 105% 或者负数
  assert.equal(estimateBookProgress(99, 4, 2), 100);
  assert.equal(estimateBookProgress(-3, 4, -1), 0);
});
