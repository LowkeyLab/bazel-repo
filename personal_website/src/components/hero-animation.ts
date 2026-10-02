import { createTimeline, svg, utils } from "animejs";
import { writeHeading } from "../utils/handwriting";
import {
  mountSection,
  revealDetails,
  type AnimationRun,
} from "../utils/section-animation";

function underline(root: HTMLElement): AnimationRun {
  const drawable = svg.createDrawable(
    root.querySelectorAll(".ink-underline > svg path"),
    0,
    1,
  );
  const reset = utils.set(drawable, { draw: "0 0" });
  const visibility = utils.set(root.querySelectorAll(".ink-underline > svg"), {
    visibility: "visible",
  });
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
  const timeline = createTimeline({ onComplete: () => complete("completed") });
  timeline.add(drawable, { draw: ["0 0", "0 1"], duration: 500 });
  return {
    finished,
    cancel: () => {
      timeline.revert();
      reset.revert();
      visibility.revert();
      complete("cancelled");
    },
  };
}

function writeHero(
  root: HTMLElement,
  heading: HTMLElement,
): AnimationRun | undefined {
  const writing = writeHeading(heading, 2600);
  if (!writing) return;
  let decoration: ReturnType<typeof createTimeline> | undefined;
  try {
    decoration = createTimeline({ defaults: { ease: "outQuad" } });
    decoration
      .add(
        root.querySelectorAll(".hero-art"),
        { opacity: [0.35, 1], translateY: [10, 0], duration: 650 },
        120,
      )
      .add(
        svg.createDrawable(root.querySelectorAll(".hero-flourish"), 0, 1),
        { draw: ["0 0", "0 1"], duration: 450 },
        650,
      )
      .add(
        root.querySelectorAll(".hero-plant"),
        { rotate: [0, -3, 2, 0], duration: 1000, ease: "inOutSine" },
        450,
      );
  } catch (error) {
    writing.cancel();
    decoration?.revert();
    throw error;
  }
  return {
    finished: writing.finished.then((result) => {
      decoration?.revert();
      return result;
    }),
    cancel: () => {
      writing.cancel();
      decoration?.revert();
    },
  };
}

export function mount(root: HTMLElement): () => void {
  const heading = root.querySelector<HTMLElement>("h1");
  if (!heading) return () => {};
  const details = [
    ...root.querySelectorAll<HTMLElement>("[data-animation-detail]"),
  ];
  return mountSection(root, {
    heading,
    details,
    trigger: "immediate",
    write: () => writeHero(root, heading),
    afterWrite: () => underline(root),
    reveal: () => revealDetails(details),
  });
}
