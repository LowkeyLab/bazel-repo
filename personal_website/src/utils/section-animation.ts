import { animate, stagger } from "animejs";
import {
  createSettlement,
  logAnimation,
  type AnimationRoot,
  type AnimationObserver,
  type AnimationPhase,
  type AnimationResult,
} from "./animation-observation";

export interface AnimationRun {
  finished: Promise<"completed" | "cancelled">;
  cancel(): void;
}

export interface SectionSequence {
  heading: HTMLElement;
  details: readonly HTMLElement[];
  trigger: "immediate" | "visible";
  write(): AnimationRun | undefined;
  afterWrite?(): AnimationRun | undefined;
  reveal(): AnimationRun;
}

function staticContent(root: HTMLElement, details: readonly HTMLElement[]) {
  (root as AnimationRoot).animationBootstrap?.restore();
  delete root.dataset.animationPending;
  for (const detail of details) {
    detail.inert = false;
    detail.style.removeProperty("opacity");
    detail.style.removeProperty("visibility");
    detail.style.removeProperty("transform");
    detail.style.removeProperty("translate");
  }
}

export function revealDetails(details: readonly HTMLElement[]): AnimationRun {
  let finish!: (result: "completed" | "cancelled") => void;
  let settled = false;
  const finished = new Promise<"completed" | "cancelled">(
    (resolve) => (finish = resolve),
  );
  const settle = (result: "completed" | "cancelled") => {
    if (settled) return;
    settled = true;
    finish(result);
  };
  if (details.length === 0) {
    settle("completed");
    return { finished, cancel: () => settle("cancelled") };
  }
  const animation = animate(details, {
    opacity: [0, 1],
    translateY: [8, 0],
    duration: 450,
    delay: stagger(70),
    ease: "outQuad",
    onComplete: () => settle("completed"),
  });
  return {
    finished,
    cancel: () => {
      animation.revert();
      settle("cancelled");
    },
  };
}

export function mountSection(
  root: HTMLElement,
  sequence: SectionSequence,
  observe: AnimationObserver = logAnimation,
): () => void {
  const bootstrap = (root as AnimationRoot).animationBootstrap;
  const settle = bootstrap ? bootstrap.settle : createSettlement(root, observe);
  let phase: AnimationPhase = "fonts";
  const motionPreference = matchMedia("(prefers-reduced-motion: reduce)");
  const active = new Set<AnimationRun>();
  let disposed = false;
  let observer: IntersectionObserver | undefined;
  let started = false;
  const restore = () => staticContent(root, sequence.details);

  const cancel = (
    result: AnimationResult = {
      outcome: "cancelled",
      reason: root.isConnected ? "cancelled" : "disconnection",
    },
  ) => {
    if (disposed) return;
    disposed = true;
    observer?.disconnect();
    for (const run of active) run.cancel();
    active.clear();
    restore();
    settle(result, phase);
    motionPreference.removeEventListener("change", onMotionChange);
    window.removeEventListener("resize", onResize);
    window.removeEventListener("pagehide", onPageHide);
  };
  const onResize = () => cancel({ outcome: "cancelled", reason: "resize" });
  const onPageHide = () => cancel({ outcome: "cancelled", reason: "pagehide" });
  const onMotionChange = () => {
    if (motionPreference.matches)
      cancel({ outcome: "cancelled", reason: "preference" });
  };
  if (
    bootstrap &&
    !bootstrap.register(
      () => cancel({ outcome: "fallback", reason: "watchdog_expired" }),
      observe,
    )
  ) {
    cancel();
    return cancel;
  }
  if (motionPreference.matches) {
    cancel({ outcome: "cancelled", reason: "preference" });
    return cancel;
  }
  for (const detail of sequence.details) detail.inert = true;
  if (!root.hasAttribute("data-animation-pending"))
    root.dataset.animationPending = "";
  motionPreference.addEventListener("change", onMotionChange);
  window.addEventListener("resize", onResize);
  window.addEventListener("pagehide", onPageHide);

  const follow = async (run: AnimationRun | undefined): Promise<boolean> => {
    if (!run) {
      cancel({ outcome: "fallback", reason: "initialization_unavailable" });
      return false;
    }
    active.add(run);
    const result = await run.finished;
    active.delete(run);
    return !disposed && result === "completed" && root.isConnected;
  };

  const start = async () => {
    if (started || disposed || !root.isConnected) return;
    started = true;
    try {
      phase = "writing";
      if (!(await follow(sequence.write()))) {
        cancel();
        return;
      }
      phase = "after_write";
      if (sequence.afterWrite && !(await follow(sequence.afterWrite()))) {
        cancel();
        return;
      }
      for (const detail of sequence.details) {
        detail.style.opacity = "0";
        detail.style.visibility = "visible";
      }
      delete root.dataset.animationPending;
      phase = "reveal";
      if (!(await follow(sequence.reveal()))) {
        cancel();
        return;
      }
      restore();
      settle({ outcome: "completed" }, phase);
    } catch {
      cancel({ outcome: "fallback", reason: "exception" });
    }
  };

  void document.fonts.ready
    .then(() => {
      if (disposed || !root.isConnected) return;
      const loaded = [...document.fonts].some(
        (font) => font.family.includes("Caveat") && font.status === "loaded",
      );
      if (!loaded) {
        cancel({ outcome: "fallback", reason: "font_unavailable" });
        return;
      }
      if (!root.hasAttribute("data-animation-pending")) {
        cancel();
        return;
      }
      if (bootstrap && !bootstrap.claim()) {
        cancel();
        return;
      }
      phase = "waiting";
      if (sequence.trigger === "immediate") {
        void start();
      } else {
        observer = new IntersectionObserver(
          (entries) => {
            if (!entries.some((entry) => entry.isIntersecting)) return;
            observer?.disconnect();
            void start();
          },
          { threshold: 0.8 },
        );
        observer.observe(sequence.heading);
      }
    })
    .catch(() => cancel({ outcome: "fallback", reason: "exception" }));
  return cancel;
}
