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

function animateArt(root: HTMLElement, entrance: boolean): AnimationRun {
  let settle!: (result: "completed" | "cancelled") => void;
  const finished = new Promise<"completed" | "cancelled">(
    (resolve) => (settle = resolve),
  );
  let cancelled = false;
  const timeline = createTimeline({
    defaults: { ease: "outQuad" },
    onComplete: () => {
      settle("completed");
      cancel();
    },
  });
  const cancel = () => {
    if (cancelled) return;
    cancelled = true;
    timeline.revert();
    settle("cancelled");
  };
  try {
    timeline
      .add(
        root.querySelectorAll(".hero-code"),
        { opacity: [0, 1], duration: 650 },
        entrance ? 0 : 120,
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
  return { finished, cancel };
}

function revealHero(
  details: readonly HTMLElement[],
  playArt: () => AnimationRun,
): AnimationRun {
  const reveal = revealDetails(details);
  let art: AnimationRun;
  try {
    art = playArt();
  } catch (error) {
    reveal.cancel();
    throw error;
  }
  const textFinished = reveal.finished.then((result) => {
    if (result === "completed") {
      for (const detail of details) detail.inert = false;
    }
    return result;
  });
  return {
    // Replaying replaces the entrance art without cancelling the text reveal.
    finished: Promise.all([textFinished, art.finished]).then(
      ([result]) => result,
    ),
    cancel: () => {
      reveal.cancel();
      art.cancel();
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
  let cancelArt = () => {};
  const playArt = (entrance = false) => {
    cancelArt();
    const art = animateArt(root, entrance);
    cancelArt = art.cancel;
    return art;
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
    if (button) button.disabled = motionPreference.matches;
    if (motionPreference.matches) stopArt();
  };
  const disposeSection = mountSection(root, {
    heading,
    details,
    trigger: "immediate",
    write: () => writeHeading(heading, 2600),
    afterWrite: () => underline(root),
    reveal: () => revealHero(details, () => playArt(true)),
  });
  // Section cancellation must settle before art-only listeners resolve playback.
  const motionPreference = matchMedia("(prefers-reduced-motion: reduce)");
  button?.addEventListener("click", replay);
  motionPreference.addEventListener("change", onMotionChange);
  window.addEventListener("resize", stopArt);
  window.addEventListener("pagehide", stopArt);
  onMotionChange();
  return () => {
    if (button) button.disabled = true;
    disposeSection();
    stopArt();
    button?.removeEventListener("click", replay);
    motionPreference.removeEventListener("change", onMotionChange);
    window.removeEventListener("resize", stopArt);
    window.removeEventListener("pagehide", stopArt);
  };
}
