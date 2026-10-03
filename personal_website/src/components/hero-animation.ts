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

function animateArt(root: HTMLElement, entrance: boolean): () => void {
  let cancelled = false;
  const timeline = createTimeline({
    defaults: { ease: "outQuad" },
    onComplete: () => cancel(),
  });
  const cancel = () => {
    if (cancelled) return;
    cancelled = true;
    timeline.revert();
  };
  try {
    timeline
      .add(
        root.querySelectorAll(entrance ? ".hero-art" : ".hero-replay > svg"),
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
    cancel();
    throw error;
  }
  return cancel;
}

function writeHero(
  heading: HTMLElement,
  playArt: () => () => void,
): AnimationRun | undefined {
  const writing = writeHeading(heading, 2600);
  if (!writing) return;
  let cancelArt: (() => void) | undefined;
  try {
    cancelArt = playArt();
  } catch (error) {
    writing.cancel();
    throw error;
  }
  return {
    finished: writing.finished.then((result) => {
      cancelArt?.();
      return result;
    }),
    cancel: () => {
      writing.cancel();
      cancelArt?.();
    },
  };
}

export function mount(root: HTMLElement): () => void {
  const heading = root.querySelector<HTMLElement>("h1");
  if (!heading) return () => {};
  const details = [
    ...root.querySelectorAll<HTMLElement>("[data-animation-detail]"),
  ];
  const button = root.querySelector<HTMLButtonElement>(".hero-replay");
  const motionPreference = matchMedia("(prefers-reduced-motion: reduce)");
  let cancelArt = () => {};
  const playArt = (entrance = false) => {
    cancelArt();
    cancelArt = animateArt(root, entrance);
    return cancelArt;
  };
  const replay = () => {
    if (motionPreference.matches) return;
    try {
      playArt();
    } catch {
      cancelArt();
    }
  };
  const stopArt = () => cancelArt();
  const onMotionChange = () => {
    if (motionPreference.matches) stopArt();
  };
  button?.addEventListener("click", replay);
  motionPreference.addEventListener("change", onMotionChange);
  window.addEventListener("resize", stopArt);
  window.addEventListener("pagehide", stopArt);
  const disposeSection = mountSection(root, {
    heading,
    details,
    trigger: "immediate",
    write: () => writeHero(heading, () => playArt(true)),
    afterWrite: () => underline(root),
    reveal: () => revealDetails(details),
  });
  return () => {
    disposeSection();
    stopArt();
    button?.removeEventListener("click", replay);
    motionPreference.removeEventListener("change", onMotionChange);
    window.removeEventListener("resize", stopArt);
    window.removeEventListener("pagehide", stopArt);
  };
}
