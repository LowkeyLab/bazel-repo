import { writeHeading } from "../utils/handwriting";
import { mountSection, revealDetails } from "../utils/section-animation";

export function mount(root: HTMLElement): () => void {
  const heading = root.querySelector<HTMLElement>(".home-section > h2");
  if (!heading) return () => {};
  const details = [
    ...root.querySelectorAll<HTMLElement>(".entry-link, .entry-list > p"),
  ];
  return mountSection(root, {
    heading,
    details,
    trigger: "visible",
    write: () =>
      writeHeading(heading, Number(root.dataset.writingDuration) || 1200),
    reveal: () => revealDetails(details),
  });
}
