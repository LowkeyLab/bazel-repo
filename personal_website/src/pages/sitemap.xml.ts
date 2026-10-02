import type { APIRoute } from "astro";
import { getCollection } from "astro:content";
import { publishedPosts } from "../utils/blog";
import { canonicalUrl, escapeXml } from "../utils/seo";
export const GET: APIRoute = async ({ site }) => {
  if (!site) throw new Error("The production site origin must be configured");
  const posts = publishedPosts(await getCollection("blog"));
  const paths = [
    "/",
    "/blog/",
    "/about/",
    ...posts.map((post) => `/blog/${post.id}/`),
  ];
  const xml = `<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">${paths.map((path) => `<url><loc>${escapeXml(canonicalUrl(path, site))}</loc></url>`).join("")}</urlset>`;
  return new Response(xml, {
    headers: { "Content-Type": "application/xml; charset=utf-8" },
  });
};
