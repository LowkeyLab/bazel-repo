import assert from "node:assert/strict";
import { homePosts, publishedPosts } from "../src/utils/blog.js";

type BlogPost = Parameters<typeof publishedPosts>[0][number];

function post(
  id: string,
  date: string,
  { featured = false, draft = false } = {},
): BlogPost {
  return {
    id,
    data: {
      title: id,
      description: "Test article",
      tags: [],
      publishDate: new Date(date),
      featured,
      draft,
    },
  };
}
const posts = [
  post("old", "2026-01-01"),
  post("featured-old", "2025-01-01", { featured: true }),
  post("zulu", "2026-03-01"),
  post("draft", "2027-01-01", { draft: true }),
  post("featured-new", "2026-04-01", { featured: true }),
  post("middle", "2026-02-01"),
  post("featured-beta", "2026-03-01", { featured: true }),
  post("alpha", "2026-03-01"),
  post("featured-draft", "2027-01-01", { featured: true, draft: true }),
  post("featured-alpha", "2026-03-01", { featured: true }),
];
const ids = (entries: BlogPost[]) => entries.map((entry) => entry.id);
for (const input of [posts, [...posts].reverse()]) {
  assert.deepEqual(ids(publishedPosts(input)), [
    "featured-new",
    "alpha",
    "featured-alpha",
    "featured-beta",
    "zulu",
    "middle",
    "old",
    "featured-old",
  ]);
  const { featured, recent } = homePosts(input);
  assert.deepEqual(ids(featured), [
    "featured-new",
    "featured-alpha",
    "featured-beta",
  ]);
  assert.deepEqual(ids(recent), ["alpha", "zulu", "middle"]);
}
assert.deepEqual(homePosts([]), { featured: [], recent: [] });
assert.deepEqual(
  publishedPosts([post("draft", "2027-01-01", { draft: true })]),
  [],
);
const only = post("only", "2026-01-01");
assert.deepEqual(homePosts([only]), { featured: [], recent: [only] });
console.log("publication_and_home_selection: passed");
