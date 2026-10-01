import { defineCollection, z } from "astro:content";
import { glob } from "astro/loaders";

const blog = defineCollection({
  loader: glob({ pattern: "**/*.md", base: "./src/content/blog" }),
  schema: z.object({
    title: z.string(),
    description: z.string(),
    publishDate: z.coerce.date(),
    tags: z.array(z.string()),
    type: z.enum(["work", "project", "essay", "note"]),
    featured: z.boolean().default(false),
    context: z.string().optional(),
    draft: z.boolean().optional().default(false),
  }),
});

export const collections = {
  blog,
};
