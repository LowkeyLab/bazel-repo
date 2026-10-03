/** Local diagnostic contract. No content, URLs, exception text, or user data. */
export type AnimationPhase =
  "bootstrap" | "fonts" | "waiting" | "writing" | "after_write" | "reveal";
export type AnimationResult =
  | { outcome: "completed" }
  | {
      outcome: "fallback";
      reason:
        | "font_unavailable"
        | "initialization_unavailable"
        | "watchdog_expired"
        | "exception";
    }
  | {
      outcome: "cancelled";
      reason:
        "preference" | "resize" | "pagehide" | "disconnection" | "cancelled";
    };
export type SectionAnimationSettled = Readonly<
  AnimationResult & {
    type: "SectionAnimationSettled";
    section: "hero" | "featured_work" | "recent_writing" | "other";
    runId: number;
    phase: AnimationPhase;
    elapsedMs: number;
  }
>;
export type AnimationObserver = (event: SectionAnimationSettled) => void;

/** Listener owns severity. Delivery is synchronous, local, and best effort. */
export const logAnimation: AnimationObserver = (event) => {
  if (event.outcome === "fallback") console.warn(event);
  else console.debug(event);
};

export function deliverAnimation(
  event: SectionAnimationSettled,
  observer: AnimationObserver,
) {
  try {
    observer(event);
  } catch {
    // The observer itself is unavailable; never recursively emit through it.
    try {
      console.warn("Animation diagnostic delivery failed");
    } catch {
      /* Diagnostics must not break content restoration. */
    }
  }
}

export interface AnimationBootstrap {
  register(cancel: () => void, observer: AnimationObserver): boolean;
  claim(): boolean;
  restore(): void;
  settle(result: AnimationResult, phase: AnimationPhase): void;
}
export type AnimationRoot = HTMLElement & {
  animationBootstrap?: AnimationBootstrap;
};

type AnimationDocument = Document & { animationRunId?: number };
export function createSettlement(
  root: HTMLElement,
  observer: AnimationObserver,
) {
  const document = root.ownerDocument as AnimationDocument;
  const runId = (document.animationRunId = (document.animationRunId ?? 0) + 1);
  const startedAt = performance.now();
  let settled = false;
  const section =
    root.localName === "home-hero-animation"
      ? "hero"
      : root.localName === "featured-work-animation"
        ? "featured_work"
        : root.localName === "recent-writing-animation"
          ? "recent_writing"
          : "other";
  return (result: AnimationResult, phase: AnimationPhase) => {
    if (settled) return;
    settled = true;
    deliverAnimation(
      Object.freeze({
        type: "SectionAnimationSettled",
        section,
        runId,
        phase,
        elapsedMs: performance.now() - startedAt,
        ...result,
      }),
      observer,
    );
  };
}
