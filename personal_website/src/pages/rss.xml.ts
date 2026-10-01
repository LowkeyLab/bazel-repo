import type { APIRoute } from "astro";
import { getCollection } from "astro:content";
import { publishedPosts } from "../utils/blog";
import { canonicalUrl, escapeXml } from "../utils/seo";
export const GET: APIRoute = async ({ site }) => {
  if (!site) throw new Error("The production site origin must be configured");
  const posts = publishedPosts(await getCollection("blog"));
  const items = posts.map((post) => {
    const link = escapeXml(canonicalUrl(`/blog/${post.id}/`, site));
    return `<item><title>${escapeXml(post.data.title)}</title><description>${escapeXml(post.data.description)}</description><link>${link}</link><guid isPermaLink="true">${link}</guid><pubDate>${post.data.publishDate.toUTCString()}</pubDate></item>`;
  });
  const xml = `<?xml version="1.0" encoding="UTF-8"?><rss version="2.0"><channel><title>Tim Tran</title><description>Writing about software, developer tooling, and the work behind it.</description><link>${escapeXml(canonicalUrl("/blog/", site))}</link>${items.join("")}</channel></rss>`;
  return new Response(xml, {
    headers: { "Content-Type": "application/rss+xml; charset=utf-8" },
  });
};
