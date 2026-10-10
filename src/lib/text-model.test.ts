import assert from "node:assert/strict";
import test from "node:test";
import { plainTextFromHtml, textModel } from "./text-model.ts";
import type { TextNodeLike } from "./text-model.ts";

test("inline markup preserves contiguous text, entities and block boundaries", () => {
  assert.equal(plainTextFromHtml("<p>alpha<b>beta</b> gamma &#x4e2d;&#25991;</p><p>end</p>"), "alphabeta gamma 中文 end");
  assert.equal(plainTextFromHtml("<script>ignored</script><p>a &amp; b</p>"), "a & b");
});

test("normalized positions map back to each original text node", () => {
  const a: TextNodeLike = { nodeType: 3, data: "alpha", childNodes: [] };
  const b: TextNodeLike = { nodeType: 3, data: "beta", childNodes: [] };
  const c: TextNodeLike = { nodeType: 3, data: "   gamma", childNodes: [] };
  const model = textModel({ nodeType: 1, childNodes: [a, b, c] });
  assert.equal(model.text, "alphabeta gamma");
  const position = model.text.indexOf("gamma");
  assert.equal(model.points[position].node, c);
  assert.equal(model.points[position].offset, 3);
  assert.equal(model.points[4].node, a);
  assert.equal(model.points[5].node, b);
});
