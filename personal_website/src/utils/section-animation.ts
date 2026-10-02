import { animate, stagger } from "animejs";

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

type BootstrapRoot = HTMLElement & {
  animationBootstrap?: {
    register(cancel: () => void): boolean;
    claim(): boolean;
    restore(): void;
  };
};

function staticContent(root: HTMLElement, details: readonly HTMLElement[]) {
  (root as BootstrapRoot).animationBootstrap?.restore();
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
): () => void {
  const motionPreference = matchMedia("(prefers-reduced-motion: reduce)");
  const active = new Set<AnimationRun>();
  let disposed = false;
  let observer: IntersectionObserver | undefined;
  let started = false;
  const restore = () => staticContent(root, sequence.details);

  const cancel = () => {
    if (disposed) return;
    disposed = true;
    observer?.disconnect();
    for (const run of active) run.cancel();
    active.clear();
    restore();
    motionPreference.removeEventListener("change", onMotionChange);
    window.removeEventListener("resize", cancel);
    window.removeEventListener("pagehide", cancel);
  };
  const onMotionChange = () => {
    if (motionPreference.matches) cancel();
  };
  if (motionPreference.matches) {
    restore();
    return cancel;
  }
  const bootstrap = (root as BootstrapRoot).animationBootstrap;
  if (bootstrap && !bootstrap.register(cancel)) {
    cancel();
    return cancel;
  }
  for (const detail of sequence.details) detail.inert = true;
  if (!root.hasAttribute("data-animation-pending"))
    root.dataset.animationPending = "";
  motionPreference.addEventListener("change", onMotionChange);
  window.addEventListener("resize", cancel);
  window.addEventListener("pagehide", cancel);

  const follow = async (run: AnimationRun | undefined): Promise<boolean> => {
    if (!run) return false;
    active.add(run);
    const result = await run.finished;
    active.delete(run);
    return !disposed && result === "completed" && root.isConnected;
  };

  const start = async () => {
    if (started || disposed || !root.isConnected) return;
    started = true;
    try {
      if (!(await follow(sequence.write()))) {
        cancel();
        return;
      }
      if (sequence.afterWrite && !(await follow(sequence.afterWrite()))) {
        cancel();
        return;
      }
      for (const detail of sequence.details) {
        detail.style.opacity = "0";
        detail.style.visibility = "visible";
      }
      delete root.dataset.animationPending;
      if (!(await follow(sequence.reveal()))) {
        cancel();
        return;
      }
      restore();
    } catch {
      cancel();
    }
  };

  void document.fonts.ready
    .then(() => {
      if (disposed || !root.isConnected) return;
      const loaded = [...document.fonts].some(
        (font) => font.family.includes("Caveat") && font.status === "loaded",
      );
      if (!loaded || !root.hasAttribute("data-animation-pending")) {
        cancel();
        return;
      }
      if (bootstrap && !bootstrap.claim()) {
        cancel();
        return;
      }
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
    .catch(cancel);
  return cancel;
}
