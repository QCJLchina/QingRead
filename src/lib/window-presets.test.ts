import assert from "node:assert/strict";
import test from "node:test";
import {
  MAX_WINDOW_HEIGHT,
  MAX_WINDOW_WIDTH,
  MIN_WINDOW_HEIGHT,
  MIN_WINDOW_WIDTH,
  WINDOW_PRESETS,
  applyLockedRatio,
  clampWindowSize,
  presetById,
} from "./window-presets.ts";

test("presets stay inside the supported window range", () => {
  assert.equal(WINDOW_PRESETS.length, 4);
  for (const preset of WINDOW_PRESETS) {
    assert.ok(preset.width >= MIN_WINDOW_WIDTH, preset.id + " width");
    assert.ok(preset.height >= MIN_WINDOW_HEIGHT, preset.id + " height");
    assert.ok(preset.width <= MAX_WINDOW_WIDTH, preset.id + " width");
    assert.ok(preset.height <= MAX_WINDOW_HEIGHT, preset.id + " height");
  }
});

test("preset lookup returns null for unknown layouts", () => {
  assert.equal(presetById("slim")?.width, 360);
  assert.equal(presetById("mini")?.height, 230);
  assert.equal(presetById("custom"), null);
});

test("clampWindowSize respects both the floor and the display work area", () => {
  assert.deepEqual(clampWindowSize(10, 10), {
    width: MIN_WINDOW_WIDTH,
    height: MIN_WINDOW_HEIGHT,
  });
  assert.deepEqual(clampWindowSize(99999, 99999, { width: 1920, height: 1080 }), {
    width: 1920,
    height: 1080,
  });
  // 显示器比下限还小时不会把窗口夹成负数
  assert.deepEqual(clampWindowSize(500, 400, { width: 200, height: 120 }), {
    width: MIN_WINDOW_WIDTH,
    height: MIN_WINDOW_HEIGHT,
  });
});

test("locked ratio follows whichever side the user edited", () => {
  const ratio = 940 / 610;
  assert.deepEqual(applyLockedRatio(470, 610, ratio, "width"), {
    width: 470,
    height: Math.round(470 / ratio),
  });
  assert.deepEqual(applyLockedRatio(940, 305, ratio, "height"), {
    width: Math.round(305 * ratio),
    height: 305,
  });
  // 比例为 0 或非法时原样返回，避免出现除零
  assert.deepEqual(applyLockedRatio(500, 400, 0, "width"), { width: 500, height: 400 });
});
