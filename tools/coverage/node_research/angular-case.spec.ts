import { it, expect } from "vitest";
import { spawnSync } from "node:child_process";
import "./maintained/personal_website/tests/blog.test";
import { choose } from "./production";
import { twice } from "./helper";
import { canonicalUrl } from "./maintained/personal_website/src/utils/seo";
import { featuredPosts } from "./maintained/personal_website/tests/fixtures/posts";
import { createMockObject } from "./maintained/angular/projects/predix/src/testing/create-mock-object";
it("original parent, test, helper, and child records", () => {
  expect(choose(1)).toBe(2);
  expect(twice(3)).toBe(6);
  expect(canonicalUrl("/post", new URL("https://example.test/"))).toBe(
    "https://example.test/post/",
  );
  expect(featuredPosts.length).toBe(3);
  const mock = createMockObject(["execute"] as const);
  expect(mock.execute).toBeTypeOf("function");
  expect(Object.keys(mock)).toEqual(["execute"]);
  const child = spawnSync(
    process.execPath,
    [process.env["RESEARCH_CHILD_SCRIPT"]!],
    { encoding: "utf8" },
  );
  expect(child.status, child.stderr).toBe(0);
});
