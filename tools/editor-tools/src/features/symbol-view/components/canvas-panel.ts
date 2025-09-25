import van, { type State } from "vanjs-core";
import { startModal } from "@/components/modal";
import { HelpIcon } from "@/icons";
import type { Point, Stroke } from "../detypify-filter";

const { div, canvas, button } = van.tags;

const HelpPanel = () => {
  const { p, b, i, a, h4 } = van.tags;

  return div(
    { class: "text-base-content" },
    p(
      "The ",
      b("offline"),
      " handwritten stroke recognizer is powered by ",
      a({ href: "https://github.com/QuarticCat/detypify" }, "Detypify"),
      ". Draw a symbol to search for it.",
    ),
    h4("Cannot find some symbols?"),
    p(
      "🔍: Check the supported symbols listed in ",
      a(
        { href: "https://github.com/QuarticCat/detypify/blob/main/assets/supported-symbols.txt" },
        "supported-symbols.txt",
      ),
      ".",
    ),
    p(
      "❤️‍🔥: Click the ",
      i("contribute mode button"),
      " and contribute at ",
      a({ href: "https://detypify.quarticcat.com/" }, "Detypify"),
      ".",
    ),
    p(
      "📝: Report the missing symbol to ",
      a({ href: "https://github.com/QuarticCat/detypify/issues/new" }, "GitHub Issues"),
      ".",
    ),
    h4("Like it?"),
    p(
      "Give a star🌟 to the ",
      a({ href: "https://github.com/QuarticCat/detypify" }, "Detypify"),
      "!",
    ),
  );
};

export const CanvasPanel = (strokesState: State<Stroke[] | undefined>) => {
  const srcCanvas = canvas({
    width: "160",
    height: "160",
    class: "stroke-canvas",
  });

  const context = srcCanvas.getContext("2d");
  if (!context) {
    throw new Error("Could not get 2D context from canvas");
  }

  // Store the context in a variable that TypeScript knows is not null
  const ctx = context;

  // Configure drawing context
  ctx.lineWidth = 2;
  ctx.lineJoin = "round";
  ctx.lineCap = "round";

  // Check for dark theme
  const serverDark = document.body.classList.contains("typst-preview-dark");
  if (serverDark) {
    ctx.fillStyle = "white";
    ctx.strokeStyle = "white";
  } else {
    ctx.fillStyle = "black";
    ctx.strokeStyle = "black";
  }

  type PointEvent = Pick<MouseEvent, "offsetX" | "offsetY">;

  let isDrawing = false;
  let currP: Point;
  let stroke: Point[];

  const touchCall = (fn: (e: PointEvent) => void) => (e: TouchEvent) => {
    const rect = srcCanvas.getBoundingClientRect();
    fn({
      offsetX: e.touches[0].clientX - rect.left,
      offsetY: e.touches[0].clientY - rect.top,
    });
  };

  function drawStart({ offsetX, offsetY }: PointEvent) {
    isDrawing = true;

    offsetX = Math.round(offsetX);
    offsetY = Math.round(offsetY);

    currP = [offsetX, offsetY];
    stroke = [currP];
  }

  function drawMove({ offsetX, offsetY }: PointEvent) {
    if (!isDrawing) return;

    offsetX = Math.round(offsetX);
    offsetY = Math.round(offsetY);

    ctx.beginPath();
    ctx.moveTo(currP[0], currP[1]);
    ctx.lineTo(offsetX, offsetY);
    ctx.stroke();

    currP = [offsetX, offsetY];
    stroke.push(currP);
  }

  function drawEnd() {
    if (!isDrawing) return; // normal mouse leave
    isDrawing = false;
    if (stroke.length === 1) return; // no line
    strokesState.val = [...(strokesState.oldVal ?? []), stroke];
  }

  function drawClear() {
    strokesState.val = undefined;
    ctx.clearRect(0, 0, srcCanvas.width, srcCanvas.height);
  }

  srcCanvas.addEventListener("mousedown", drawStart);
  srcCanvas.addEventListener("mousemove", drawMove);
  srcCanvas.addEventListener("mouseup", drawEnd);
  srcCanvas.addEventListener("mouseleave", drawEnd);
  srcCanvas.addEventListener("touchstart", touchCall(drawStart));
  srcCanvas.addEventListener("touchmove", touchCall(drawMove));
  srcCanvas.addEventListener("touchend", drawEnd);
  srcCanvas.addEventListener("touchcancel", drawEnd);

  return div(
    { class: "flex flex-col items-center gap-xs" },
    div(
      // title & help
      { class: "flex items-center justify-between" },
      div({ class: "text-sm font-bold" }, "Draw Symbol"),
      div(
        {
          class: "cursor-pointer",
          title: `The offline handwritten stroke recognizer is powered by Detypify. Draw a symbol to search for it.`,
          onclick: () => startModal(HelpPanel()),
        },
        HelpIcon(),
      ),
    ),
    srcCanvas,
    button(
      {
        class: "btn btn-secondary w-full",
        title: "Clear canvas",
        onclick: drawClear,
      },
      "Clear",
    ),
  );
};
