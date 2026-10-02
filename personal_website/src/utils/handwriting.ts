import { createTimeline } from "animejs";
import type { AnimationRun } from "./section-animation";

// Pen routes in each glyph's ink bounds. The mask reveals the real Caveat text;
// these broad strokes are guides, not replacement letterforms.
const penRoutes: Record<string, string> = {
  H: "M.3 0 .05 1 M1 0 .7 1 M.15 .55 .85 .4",
  I: "M.8 0 .2 1",
  T: "M0 .15 1 .05 M.6 .1 .35 1",
  R: "M.3 0 .05 1 M.3 .1 Q1 -.1 .9 .3 Q.8 .55 .2 .55 L.9 1",
  a: "M.8 .2 Q.4 -.2 .15 .4 Q-.1 1 .35 .9 Q.7 .8 .8 .1 L.65 .9 1 .8",
  b: "M.8 0 Q.4 .3 .15 .9 Q.3 .4 .7 .5 Q1 .6 .6 .9 Q.3 1 .15 .9",
  c: "M.9 .15 Q.5 -.1 .2 .4 Q-.1 1 .4 .9 L1 .7",
  d: "M.65 .5 Q.3 .25 .15 .65 Q0 1 .4 .9 Q.65 .7 1 0 L.65 .9 1 .8",
  e: "M.1 .6 Q.9 .6 .85 .15 Q.4 -.1 .15 .5 Q-.1 1 .5 .9 L1 .65",
  f: "M.9 .1 Q.5 -.2 .4 .35 L.1 1 M0 .45 1 .35",
  g: "M.8 .1 Q.4 -.1 .15 .3 Q0 .65 .4 .55 L.8 .1 Q.6 .8 .2 .95 Q-.1 1 .1 .7 L1 .5",
  h: "M.65 0 .05 1 Q.65 .2 .8 .6 L.65 .95 1 .8",
  i: "M.55 .35 .2 .95 .9 .75 M.75 .05 .78 .05",
  k: "M.65 0 .1 1 M1 .35 .25 .65 .8 1",
  l: "M.15 .8 Q.9 .1 .7 .05 Q.4 0 .2 .6 Q0 1 .8 .85",
  m: "M.2 .1 0 1 Q.4 -.2 .45 .25 L.35 .95 Q.8 -.2 .85 .3 L.75 .9 1 .8",
  n: "M.25 .1 .05 1 Q.7 -.2 .85 .25 L.65 .95 1 .8",
  o: "M.75 .1 Q.3 -.1 .15 .5 Q0 1 .5 .9 Q1 .7 .85 .2 L.55 .1",
  p: "M.4 .1 .05 1 M.35 .3 Q.9 -.15 .9 .3 Q.8 .65 .3 .6",
  r: "M.3 .15 .05 1 M.2 .55 Q.65 -.1 1 .15",
  s: "M1 .1 Q.4 -.1 .3 .25 Q.2 .4 .65 .5 Q1 .8 .35 .9 L.05 .85",
  t: "M.65 0 .3 .75 Q.2 1 .85 .8 M0 .4 1 .3",
  u: "M.35 .05 Q-.1 1 .35 .9 Q.6 .8 .9 .1 L.7 .9 1 .8",
  v: "M.15 .1 .25 .9 Q.7 .6 .95 0",
  w: "M.1 .1 .1 .9 .55 .15 .5 .9 Q.9 .5 1 0",
  "'": "M.8 0 .2 1",
  ".": "M.4 .5 .6 .5",
  ",": "M.7 .1 .25 .9",
};

let nextMaskId = 0;
const namespace = "http://www.w3.org/2000/svg";

function element<K extends keyof SVGElementTagNameMap>(
  tag: K,
  attributes: Record<string, string | number> = {},
): SVGElementTagNameMap[K] {
  const node = document.createElementNS(namespace, tag);
  for (const [name, value] of Object.entries(attributes)) {
    node.setAttribute(name, String(value));
  }
  return node;
}

/** A temporary, reversible overlay; the original heading remains accessible. */
export function prepareHandwriting(
  heading: HTMLElement,
  duration: number,
  onComplete?: () => void,
) {
  const context = document.createElement("canvas").getContext("2d");
  if (!context) return;
  const walker = document.createTreeWalker(heading, NodeFilter.SHOW_TEXT);
  const originals: Text[] = [];
  while (walker.nextNode()) {
    const text = walker.currentNode as Text;
    if (text.textContent?.trim() && !text.parentElement?.closest("svg"))
      originals.push(text);
  }
  // Future copy with unsupported glyphs keeps its ordinary text rendering.
  if (
    originals.some((text) =>
      [...text.data].some((char) => !/\s/.test(char) && !penRoutes[char]),
    )
  )
    return;

  const replacements: { original: Text; wrapper: HTMLSpanElement }[] = [];
  const strokes: { path: SVGPathElement; finish: SVGRectElement }[] = [];
  const restore = () => {
    for (const { original, wrapper } of replacements)
      wrapper.replaceWith(original);
  };

  try {
    for (const original of originals) {
      const wrapper = document.createElement("span");
      original.replaceWith(wrapper);
      replacements.push({ original, wrapper });
      for (const word of original.data.split(/(\s+)/)) {
        if (!word.trim()) {
          wrapper.append(document.createTextNode(word));
          continue;
        }
        const span = document.createElement("span");
        span.className = "handwriting-word";
        const source = document.createElement("span");
        source.className = "handwriting-source";
        source.textContent = word;
        span.append(source);
        wrapper.append(span);
        const style = getComputedStyle(span);
        context.font = `${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
        const metrics = context.measureText(word);
        const { width, height } = span.getBoundingClientRect();
        const baseline =
          (height -
            metrics.fontBoundingBoxAscent -
            metrics.fontBoundingBoxDescent) /
            2 +
          metrics.fontBoundingBoxAscent;
        const overlay = element("svg", {
          class: "handwriting-overlay",
          "aria-hidden": "true",
          focusable: "false",
          width,
          height,
          viewBox: `0 0 ${width} ${height}`,
        });
        const id = `handwriting-${nextMaskId++}`;
        const mask = element("mask", {
          id,
          maskUnits: "userSpaceOnUse",
          x: -20,
          y: -20,
          width: width + 40,
          height: height + 40,
        });
        const defs = element("defs");
        defs.append(mask);
        const text = element("text", {
          x: 0,
          y: baseline,
          fill: "currentColor",
          mask: `url(#${id})`,
        });
        text.textContent = word;
        overlay.append(defs, text);
        span.append(overlay);

        [...word].forEach((char, index) => {
          const ink = context.measureText(char);
          const x =
            text.getStartPositionOfChar(index).x - ink.actualBoundingBoxLeft;
          const y = baseline - ink.actualBoundingBoxAscent;
          const inkWidth =
            ink.actualBoundingBoxLeft + ink.actualBoundingBoxRight;
          const inkHeight =
            ink.actualBoundingBoxAscent + ink.actualBoundingBoxDescent;
          const path = element("path", {
            d: penRoutes[char]!,
            transform: `translate(${x} ${y}) scale(${inkWidth} ${inkHeight})`,
            fill: "none",
            stroke: "white",
            "stroke-width": 0.5,
            "stroke-linecap": "round",
            "stroke-linejoin": "round",
            pathLength: 1,
            "stroke-dasharray": 1,
            "stroke-dashoffset": 1,
            opacity: 0,
          });
          // Finish the glyph's fine edges after its pen pass.
          const finish = element("rect", {
            x,
            y,
            width: inkWidth,
            height: inkHeight,
            fill: "white",
            opacity: 0,
          });
          mask.append(path, finish);
          strokes.push({ path, finish });
        });
      }
    }

    const timeline = createTimeline({
      autoplay: false,
      onComplete: () => {
        restore();
        onComplete?.();
      },
    });
    const step = duration / strokes.length;
    strokes.forEach(({ path, finish }, index) => {
      timeline.add(
        path,
        {
          strokeDashoffset: [1, 0],
          opacity: { from: 0, to: 1, duration: 1 },
          duration: step,
          ease: "linear",
        },
        index * step,
      );
      timeline.add(
        finish,
        { opacity: [0, 1], duration: step * 0.2, ease: "linear" },
        (index + 0.8) * step,
      );
    });
    return { timeline, restore };
  } catch {
    restore();
    return;
  }
}

export function writeHeading(
  heading: HTMLElement,
  duration: number,
): AnimationRun | undefined {
  let settle!: (result: "completed" | "cancelled") => void;
  let settled = false;
  const finished = new Promise<"completed" | "cancelled">(
    (resolve) => (settle = resolve),
  );
  const complete = (result: "completed" | "cancelled") => {
    if (settled) return;
    settled = true;
    settle(result);
  };
  let writing: ReturnType<typeof prepareHandwriting>;
  try {
    writing = prepareHandwriting(heading, duration, () =>
      complete("completed"),
    );
  } catch {
    return;
  }
  if (!writing) return;
  try {
    writing.timeline.play();
  } catch {
    writing.restore();
    return;
  }
  return {
    finished,
    cancel: () => {
      writing.timeline.revert();
      writing.restore();
      complete("cancelled");
    },
  };
}
