import type { CollectionEntry } from "astro:content";

type BlogPost = CollectionEntry<"blog">;

export function publishedPosts(posts: BlogPost[]): BlogPost[] {
  return posts
    .filter((post) => !post.data.draft)
    .sort(
      (a, b) =>
        b.data.publishDate.valueOf() - a.data.publishDate.valueOf() ||
        a.id.localeCompare(b.id),
    );
}

export function homePosts(posts: BlogPost[]): {
  featured: BlogPost[];
  recent: BlogPost[];
} {
  const published = publishedPosts(posts);
  return {
    featured: published.filter((post) => post.data.featured).slice(0, 3),
    recent: published.filter((post) => !post.data.featured).slice(0, 3),
  };
}

export function calculateReadingTime(content: string) {
  const words = content.split(/\s+/).length;
  return Math.ceil(words / 200);
}
