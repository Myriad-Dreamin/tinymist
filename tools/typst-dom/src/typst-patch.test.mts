import { describe, expect, it } from "vitest";
import {
  PatchPair,
  interpretTargetView,
  changeViewPerspective,
} from "./typst-patch.mjs";

interface Attributes {
  [key: string]: string | null | undefined;
  "data-tid"?: string | null;
  "data-kind"?: string | null;
  "data-reuse-from"?: string | null;
}

class MockElement {
  tagName = "g";

  constructor(public attrs: Attributes) { }

  getAttribute(s: string): string | null {
    return this.attrs[s] ?? null;
  }

  cloneNode(deep: boolean): MockElement {
    deep;
    return new MockElement(this.attrs);
  }
}

const injectOffsets = (kind: string, elems: MockElement[]): MockElement[] => {
  for (let i = 0; i < elems.length; i++) {
    elems[i].attrs["data-kind"] = kind;
    if (elems[i].attrs["data-tid"] || elems[i].attrs["data-tid"] === null) {
      continue;
    }
    elems[i].attrs["data-tid"] = i.toString();
  }

  return elems;
};

const repeatOrJust = (n: number | (number | null)[]): MockElement[] => {
  if (Array.isArray(n)) {
    return n.map(
      (i) =>
        new MockElement({
          "data-tid": i !== null ? i.toString() : null,
        })
    );
  }

  const res: MockElement[] = [];
  for (let i = 0; i < n; i++) {
    res.push(new MockElement({}));
  }

  return res;
};

const reuseStub = (n: number | null) =>
  new MockElement({
    "data-reuse-from": n !== null ? n.toString() : null,
  });

function toSnapshot([targetView, patchPair]: [
  (MockElement | number | string)[][],
  PatchPair<MockElement>[]
]): string[] {
  const repr = (elem: unknown) => {
    if (elem instanceof MockElement) {
      return (elem.attrs["data-kind"] || "") + elem.attrs["data-tid"];
    }
    return elem;
  };

  const instructions = targetView.map((i) => {
    return i.map(repr).join(",");
  });
  const patches = patchPair.length
    ? [patchPair.map((i) => i.map(repr).join("->")).join(",")]
    : [];
  return [...instructions, ...patches];
}

const hasTid = (elem: MockElement): elem is MockElement =>
  elem.getAttribute("data-tid") !== null;

const indexTargetView = (
  init: number | (number | null)[],
  rearrange: (number | null)[]
) =>
  interpretTargetView<MockElement>(
    injectOffsets("o", repeatOrJust(init)),
    injectOffsets("t", rearrange.map(reuseStub)),
    true,
    hasTid
  );
const indexOriginView = (
  init: number | (number | null)[],
  rearrange: (number | null)[]
) =>
  changeViewPerspective<MockElement>(
    injectOffsets("o", repeatOrJust(init)),
    indexTargetView(init, rearrange)[0],
    hasTid
  );

describe("interpretView", () => {
  it("handleNoReuse", () => {
    const result = indexTargetView(1, [null]);
    expect(toSnapshot(result)).toMatchInlineSnapshot(`
      [
        "append,t0",
        "remove,0",
      ]
    `);
  });
  it("handleNoReuse_origin", () => {
    const result = indexOriginView(1, [null]);
    expect(toSnapshot([result, []])).toMatchInlineSnapshot(`
      [
        "remove,0",
        "insert,0,t0",
      ]
    `);
  });

  it("handleReuse", () => {
    const result = indexTargetView(1, [0]);
    expect(toSnapshot(result)).toMatchInlineSnapshot(`
      [
        "reuse,0",
        "o0->t0",
      ]
    `);
  });
  it("handleReuse_origin", () => {
    const result = indexOriginView(1, [0]);
    expect(toSnapshot([result, []])).toMatchInlineSnapshot("[]");
  });

  it("handleMultipleReuse", () => {
    const result = indexTargetView(1, [0, 0]);
    expect(toSnapshot(result)).toMatchInlineSnapshot(`
      [
        "reuse,0",
        "append,t1",
        "o0->t0",
      ]
    `);
  });
  it("handleMultipleReuse_origin", () => {
    const result = indexOriginView(1, [0, 0]);
    expect(toSnapshot([result, []])).toMatchInlineSnapshot(`
      [
        "insert,1,t1",
      ]
    `);
  });

  it("handleReuseRemove", () => {
    const result = indexTargetView(2, [1]);
    expect(toSnapshot(result)).toMatchInlineSnapshot(`
      [
        "reuse,1",
        "remove,0",
        "o1->t0",
      ]
    `);
  });
  it("handleReuseRemove_origin", () => {
    const result = indexOriginView(2, [1]);
    expect(toSnapshot([result, []])).toMatchInlineSnapshot(`
      [
        "remove,0",
      ]
    `);
  });

  it("handleReuseRemove2", () => {
    const result = indexTargetView(5, [2, 1, 4]);
    expect(toSnapshot(result)).toMatchInlineSnapshot(`
      [
        "reuse,2",
        "reuse,1",
        "reuse,4",
        "remove,0",
        "remove,3",
        "o2->t0,o1->t1,o4->t2",
      ]
    `);
  });
  it("handleReuseRemove2_origin", () => {
    const result = indexOriginView(5, [2, 1, 4]);
    expect(toSnapshot([result, []])).toMatchInlineSnapshot(`
      [
        "remove,0",
        "remove,2",
        "swap_in,0,1",
      ]
    `);
  });

  it("handleReuseInsert", () => {
    const result = indexTargetView(5, [null, 2, null, 1, null, 4, null]);
    expect(toSnapshot(result)).toMatchInlineSnapshot(`
      [
        "append,t0",
        "reuse,2",
        "append,t2",
        "reuse,1",
        "append,t4",
        "reuse,4",
        "append,t6",
        "remove,0",
        "remove,3",
        "o2->t1,o1->t3,o4->t5",
      ]
    `);
  });
  it("handleReuseInsert_origin", () => {
    const result = indexOriginView(5, [null, 2, null, 1, null, 4, null]);
    // after remove: [1, 2, 4]
    // swap_in,0,1: [2, 1, 4]
    // insert,0,t0: [t0, 2, 1, 4]
    // insert,2,t2: [t0, 2, t2, 1, 4]
    // insert,4,t4: [t0, 2, t2, 1, t4, 4]
    // insert,6,t6: [t0, 2, t2, 1, t4, 4, t6]

    expect(toSnapshot([result, []])).toMatchInlineSnapshot(`
      [
        "remove,0",
        "remove,2",
        "swap_in,0,1",
        "insert,0,t0",
        "insert,2,t2",
        "insert,4,t4",
        "insert,6,t6",
      ]
    `);
  });

  it("handleReusePreseveOrder", () => {
    const result = indexTargetView([0, 1, 2, 1, 2], [1, 2, 1, 2]);
    expect(toSnapshot(result)).toMatchInlineSnapshot(`
      [
        "reuse,1",
        "reuse,2",
        "reuse,3",
        "reuse,4",
        "remove,0",
        "o1->t0,o2->t1,o1->t2,o2->t3",
      ]
    `);
  });
  it("handleReusePreseveOrder_origin", () => {
    const result = indexOriginView([0, 1, 2, 1, 2], [1, 2, 1, 2]);
    expect(toSnapshot([result, []])).toMatchInlineSnapshot(`
      [
        "remove,0",
      ]
    `);
  });
  it("handleReusePreseveOrder2", () => {
    const result = indexTargetView(
      [0, 1, 2, 1, 2, 3, 4, 3, 4],
      [1, 2, 3, 4, 3, 4, 1, 2]
    );
    expect(toSnapshot(result)).toMatchInlineSnapshot(`
      [
        "reuse,1",
        "reuse,2",
        "reuse,5",
        "reuse,6",
        "reuse,7",
        "reuse,8",
        "reuse,3",
        "reuse,4",
        "remove,0",
        "o1->t0,o2->t1,o3->t2,o4->t3,o3->t4,o4->t5,o1->t6,o2->t7",
      ]
    `);
  });
  it("handleReusePreseveOrder2_origin", () => {
    const result = indexOriginView(
      [0, 1, 2, 1, 2, 3, 4, 3, 4],
      [1, 2, 3, 4, 3, 4, 1, 2]
    );
    expect(toSnapshot([result, []])).toMatchInlineSnapshot(`
      [
        "remove,0",
        "swap_in,2,4",
        "swap_in,3,5",
        "swap_in,4,6",
        "swap_in,5,7",
      ]
    `);
  });
  it("handleMasterproefThesisAffectedByEmptyPage", () => {
    const origin = injectOffsets("o", repeatOrJust([null, null, null, 0]));
    const target = injectOffsets("t", [0, null].map(reuseStub));
    target[0].attrs["data-tid"] = "1";
    target[1].attrs["data-tid"] = "0";
    const result = interpretTargetView<MockElement>(
      origin,
      target,
      true,
      hasTid
    );
    const result2 = changeViewPerspective<MockElement>(
      origin,
      result[0],
      hasTid
    );
    expect(toSnapshot(result)).toMatchInlineSnapshot(`
      [
        "reuse,3",
        "append,t0",
        "o0->t1",
      ]
    `);
    expect(toSnapshot([result2, []])).toMatchInlineSnapshot(`
      [
        "insert,4,t0",
      ]
    `);
  });
  it("handleMasterproefThesisAffectedByEmptyPageAntiCase", () => {
    const origin = injectOffsets("o", repeatOrJust([null, null, null, 0, 1]));
    const target = injectOffsets("t", [0, 1].map(reuseStub));
    const result = interpretTargetView<MockElement>(
      origin,
      target,
      true,
      hasTid
    );
    const result2 = changeViewPerspective<MockElement>(
      origin,
      result[0],
      hasTid
    );
    expect(toSnapshot(result)).toMatchInlineSnapshot(`
      [
        "reuse,3",
        "reuse,4",
        "o0->t0,o1->t1",
      ]
    `);
    expect(toSnapshot([result2, []])).toMatchInlineSnapshot(`
      []
    `);
  });

  it("handleReuseAppend", () => {
    const origin = injectOffsets("o", repeatOrJust([null, null, null, 0, 1]));
    const target = injectOffsets("t", [1, null, 0, null, 1].map(reuseStub));
    const result = interpretTargetView<MockElement>(
      origin,
      target,
      true,
      hasTid
    );
    const result2 = changeViewPerspective<MockElement>(
      origin,
      result[0],
      hasTid
    );
    expect(toSnapshot(result)).toMatchInlineSnapshot(`
      [
        "reuse,4",
        "append,t1",
        "reuse,3",
        "append,t3",
        "append,t4",
        "o1->t0,o0->t2",
      ]
    `);

    // after swap_in,3,4: [o0, o1, o2, o4(1), o3(0)]
    // insert,4,t1: [o0, o1, o2, o4(1), t1, o3(0)]
    // insert,6,t3: [o0, o1, o2, o4(1), t1, o3(0), t3]
    // insert,7,t4: [o0, o1, o2, o4(1), t1, o3(0), t3, t4]
    // with patch: [o0, o1, o2, t0, t1, t2, t3, t4]
    expect(toSnapshot([result2, []])).toMatchInlineSnapshot(`
      [
        "swap_in,3,4",
        "insert,4,t1",
        "insert,6,t3",
        "insert,7,t4",
      ]
    `);
  });
});
