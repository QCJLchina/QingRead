import assert from "node:assert/strict";
import test from "node:test";
import {
  clampRatio,
  collapseWhitespace,
  findSnippetOffset,
  nodeToPath,
  pathToNode,
  snippetAround,
  snippetLooseMatch,
} from "./anchor.ts";
import type { NodeLike } from "./anchor.ts";

function fakeNode(children: NodeLike[] = []): NodeLike {
  const node: NodeLike = { childNodes: children, parentNode: null };
  for (const child of children) {
    (child as { parentNode?: NodeLike | null }).parentNode = node;
  }
  return node;
}

test("node path survives a round trip through a nested tree", () => {
  const leaf = fakeNode();
  const middle = fakeNode([fakeNode(), leaf]);
  const root = fakeNode([middle, fakeNode()]);

  assert.deepEqual(nodeToPath(root, leaf), [0, 1]);
  assert.equal(pathToNode(root, [0, 1]), leaf);
  assert.equal(pathToNode(root, [1]), root.childNodes[1]);
});

test("walking from the root to itself produces an empty path", () => {
  const root = fakeNode([fakeNode()]);
  assert.deepEqual(nodeToPath(root, root), []);
  assert.equal(pathToNode(root, []), root);
});

test("detached nodes and out-of-range paths resolve to null", () => {
  const root = fakeNode([fakeNode()]);
  const orphan = fakeNode();
  assert.equal(nodeToPath(root, orphan), null);
  assert.equal(pathToNode(root, [7]), null);
  assert.equal(pathToNode(root, [0, 3]), null);
});

test("clampRatio bounds the fallback position", () => {
  assert.equal(clampRatio(-1), 0);
  assert.equal(clampRatio(2), 1);
  assert.equal(clampRatio(0.42), 0.42);
  assert.equal(clampRatio(Number.NaN), 0);
});

test("snippet around a position keeps surrounding context", () => {
  const text = "abcdefghij";
  assert.equal(snippetAround(text, 5, 2), "defgh");
  assert.equal(snippetAround(text, 0, 2), "abc");
  assert.equal(snippetAround(text, 10, 2), "ij");
});

test("findSnippetOffset locates text inside one node", () => {
  assert.equal(findSnippetOffset("the quick brown fox", "brown"), 10);
  assert.equal(findSnippetOffset("the quick brown fox", "purple"), -1);
  assert.equal(findSnippetOffset("anything", ""), -1);
});

test("stored offsets are trusted only while the text still matches", () => {
  const text = "窗外的树影慢慢移过桌面";
  const snippet = snippetAround(text, 5, 3);
  assert.equal(snippetLooseMatch(text, 5, snippet), true);
  assert.equal(snippetLooseMatch("完全不同的正文内容", 5, snippet), false);
  // 没有片段信息时不做额外判断
  assert.equal(snippetLooseMatch(text, 5, ""), true);
});

test("collapseWhitespace normalizes layout differences", () => {
  assert.equal(collapseWhitespace("  a \n b\t c  "), "a b c");
  assert.equal(collapseWhitespace(""), "");
});
