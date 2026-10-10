import assert from "node:assert/strict";
import test from "node:test";
import { compareAnchors } from "./bookmark-order.ts";
test("anchors sort by node path then character offset", () => {
  const make = (path: number[], offset: number) => ({ path, offset, snippet: "", ratio: 0 });
  const anchors = [make([2], 10), make([1], 20), make([2], 1)];
  assert.deepEqual(anchors.sort(compareAnchors).map(a => [a.path, a.offset]), [[[1], 20], [[2], 1], [[2], 10]]);
});
