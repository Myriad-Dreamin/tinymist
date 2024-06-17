import { describe, expect, it } from "vitest";
import type { GConstructor } from "./typst-doc.mjs";

class TypstDocument {
  doc = 1;
}

interface TypstSvgDocument {
  svgProp(): number;
  renderSvg(): number;
}

interface TypstCanvasDocument {
  renderCanvas(): number;
}

function provideCanvas<
  TBase extends GConstructor<TypstDocument & TypstSvgDocument>
>(Base: TBase): TBase & GConstructor<TypstCanvasDocument> {
  return class extends Base {
    canvasFeat = 10;
    renderCanvas() {
      return this.doc + this.canvasFeat * this.svgProp();
    }
  };
}

function provideSvg<
  TBase extends GConstructor<TypstDocument & TypstCanvasDocument>
>(Base: TBase): TBase & GConstructor<TypstSvgDocument> {
  return class extends Base {
    feat = 100;
    svgProp() {
      return 5;
    }
    renderSvg() {
      return this.doc + this.feat * this.renderCanvas();
    }
  };
}

describe("mixinClass", () => {
  it("doMixin", () => {
    const T = provideSvg(
      provideCanvas(
        TypstDocument as GConstructor<TypstDocument & TypstSvgDocument>
      )
    );
    const t = new T();
    expect(t.renderCanvas()).toBe(51);
    expect(t.renderSvg()).toBe(5101);
  });
});
