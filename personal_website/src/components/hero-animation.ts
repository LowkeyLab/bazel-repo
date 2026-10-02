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

export function mount(root: HTMLElement): () => void {
  const heading = root.querySelector<HTMLElement>("h1");
  if (!heading) return () => {};
  const details = [
    ...root.querySelectorAll<HTMLElement>("[data-animation-detail]"),
  ];
  const stopSection = mountSection(root, {
    heading,
    details,
    trigger: "immediate",
    write: () => writeHeading(heading, 2600),
    afterWrite: () => underline(root),
    reveal: () => revealDetails(details),
  });
  const motionPreference = matchMedia("(prefers-reduced-motion: reduce)");
  if (motionPreference.matches) return stopSection;
  const decoration = createTimeline({ defaults: { ease: "outQuad" } });
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
  let disposed = false;
  const stop = () => {
    if (disposed) return;
    disposed = true;
    stopSection();
    decoration.revert();
    motionPreference.removeEventListener("change", onMotionChange);
    window.removeEventListener("resize", stop);
    window.removeEventListener("pagehide", stop);
  };
  const onMotionChange = () => {
    if (motionPreference.matches) stop();
  };
  motionPreference.addEventListener("change", onMotionChange);
  window.addEventListener("resize", stop);
  window.addEventListener("pagehide", stop);
  return stop;
}
