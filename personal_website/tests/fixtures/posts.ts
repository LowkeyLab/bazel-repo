import type { CollectionEntry } from "astro:content";

type BlogPost = CollectionEntry<"blog">;

function post(
  id: string,
  title: string,
  description: string,
  tags: string[],
  day: number,
): BlogPost {
  return {
    id,
    body: "A short example article for the browser fixture.",
    data: {
      title,
      description,
      publishDate: new Date(
        `2026-09-${String(day).padStart(2, "0")}T00:00:00Z`,
      ),
      tags,
      type: "project",
      featured: true,
      draft: false,
    },
  } as BlogPost;
}

export const featuredPosts = [
  post(
    "first-featured",
    "First featured project",
    "A complete first description.",
    ["rust", "bazel"],
    21,
  ),
  post(
    "second-featured",
    "Second featured project",
    "A complete second description.",
    ["typescript", "astro"],
    22,
  ),
  post(
    "third-featured",
    "Third featured project",
    "A complete third description.",
    ["kotlin", "testing"],
    23,
  ),
];

export const longPosts = [
  post(
    "long-featured",
    "A much longer project title that needs to wrap cleanly on a narrow mobile display",
    "A complete description that stays in its own entry while the title and tags wrap.",
    [
      "developer-experience",
      "remote-execution",
      "build-systems",
      "continuous-delivery",
    ],
    24,
  ),
];
