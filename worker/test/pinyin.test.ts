// Same cases as yomitan-parser/src/pinyin.rs tests.
import assert from "node:assert/strict";
import { test } from "node:test";

import { expandPinyin, normalizePinyin } from "../src/pinyin.ts";

test("normalizes tone marks and boundaries", () => {
  for (const [i, e] of [
    ["Chángān", "chang2 an1"],
    ["Cháng'ān", "chang2 an1"],
    ["Xīān", "xi1 an1"],
    ["Tiān'ānmén", "tian1 an1 men2"],
    ["Zhōngguó", "zhong1 guo2"],
    ["māma", "ma1 ma5"],
  ])
    assert.equal(normalizePinyin(i), e, i);
});

test("toneless syllables -> neutral tone", () => {
  for (const [i, e] of [
    ["pin yin", "pin5 yin5"],
    ["pin0 yin5", "pin5 yin5"],
    ["pinyin", "pin5 yin5"],
  ])
    assert.equal(normalizePinyin(i), e, i);
});

test("umlaut spellings", () => {
  for (const i of ["nǚ", "nü3", "nu:3", "nv3", "NÜ3"])
    assert.equal(normalizePinyin(i), "nv3", i);
  assert.equal(normalizePinyin("lüè"), "lve4");
  assert.equal(normalizePinyin("lǜe"), "lve4");
  assert.equal(normalizePinyin("nü 3".replace(" ", "")), "nv3"); // decomposed ü
});

test("digits end syllables; 0 is neutral", () => {
  for (const [i, e] of [
    ["pin1yin1", "pin1 yin1"],
    ["Bei3 jing1", "bei3 jing1"],
    ["yi1 hui4 r5", "yi1 hui4 r5"],
    ["pin0 yin", "pin5 yin5"],
  ])
    assert.equal(normalizePinyin(i), e, i);
});

test("rejects non-pinyin and query operators", () => {
  for (const i of ["hello", "", "pin6", "1", "12", "AND", "OR"]) {
    assert.equal(normalizePinyin(i), null, i);
    assert.deepEqual(expandPinyin(i), [], i);
  }
});

test("normalize is idempotent", () => {
  for (const i of [
    "Chángān",
    "nǚ",
    "Xī'ān",
    "Xī ān",
    "lv4",
    "pin0 yin5",
    "pin1 yin5",
  ]) {
    const once = normalizePinyin(i)!;
    assert.equal(normalizePinyin(once), once, i);
  }
});

test("expand", () => {
  assert.deepEqual(expandPinyin("Chángān"), ["chang2 an1"]);
  assert.deepEqual(expandPinyin("Cháng'ān"), ["chang2 an1"]);
  assert.deepEqual(expandPinyin("Tiān'ānmén"), ["tian1 an1 men2"]);
  assert.deepEqual(expandPinyin("māma"), [
    "ma1 ma1",
    "ma1 ma2",
    "ma1 ma3",
    "ma1 ma4",
    "ma1 ma5",
  ]);
  const all = expandPinyin("pin yin");
  assert.equal(all.length, 25);
  assert.equal(all[0], "pin1 yin1");
  assert.equal(all[24], "pin5 yin5");
  assert.deepEqual(expandPinyin("pin0 yin5"), ["pin5 yin5"]);
  assert.deepEqual(expandPinyin("pin0 yin"), [
    "pin5 yin1",
    "pin5 yin2",
    "pin5 yin3",
    "pin5 yin4",
    "pin5 yin5",
  ]);
  assert.deepEqual(expandPinyin("pin1yin1"), ["pin1 yin1"]);
  assert.deepEqual(expandPinyin("yi1 hui4 r5"), ["yi1 hui4 r5"]);
  assert.deepEqual(expandPinyin("pinyin"), all);
  for (const i of ["nǚ", "nü3", "nu:3", "nv3", "NÜ3"])
    assert.deepEqual(expandPinyin(i), ["nv3"], i);
  assert.deepEqual(expandPinyin("nü"), ["nv1", "nv2", "nv3", "nv4", "nv5"]);
  assert.deepEqual(expandPinyin("lüè"), ["lve4"]);
});

test("expanded results are stable", () => {
  for (const i of [
    "Chángān",
    "nǚ",
    "Xī'ān",
    "Xī ān",
    "lv4",
    "pin0 yin5",
    "pin1 yin5",
  ]) {
    for (const once of expandPinyin(i))
      assert.deepEqual(expandPinyin(once), [once], `${i} ${once}`);
  }
});
