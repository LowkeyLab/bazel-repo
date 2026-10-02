import { writeHeading } from "../utils/handwriting";
import {
  mountSection,
  revealDetails,
  type AnimationRun,
} from "../utils/section-animation";

function revealEntriesThenLink(
  entries: readonly HTMLElement[],
  link: HTMLElement,
): AnimationRun {
  let resolve!: (result: "completed" | "cancelled") => void;
  const finished = new Promise<"completed" | "cancelled">(
    (settle) => (resolve = settle),
  );
  let cancelled = false;
  let current = revealDetails(entries);
  void (async () => {
    if ((await current.finished) !== "completed" || cancelled) {
      resolve("cancelled");
      return;
    }
    current = revealDetails([link]);
    resolve(await current.finished);
  })();
  return {
    finished,
    cancel: () => {
      cancelled = true;
      current.cancel();
      resolve("cancelled");
    },
  };
}

export function mount(root: HTMLElement): () => void {
  const heading = root.querySelector<HTMLElement>(".home-section > h2");
  const link = root.querySelector<HTMLElement>(".all-writing");
  if (!heading || !link) return () => {};
  const entries = [...root.querySelectorAll<HTMLElement>(".entry-link")];
  const details = [...entries, link];
  return mountSection(root, {
    heading,
    details,
    trigger: "visible",
    write: () => writeHeading(heading, 1200),
    reveal: () => revealEntriesThenLink(entries, link),
  });
}
