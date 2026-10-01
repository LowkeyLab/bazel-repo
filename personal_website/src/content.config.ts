import { defineCollection, z } from "astro:content";
import { glob } from "astro/loaders";

const work = defineCollection({
  loader: glob({ pattern: "**/*.md", base: "./src/content/work" }),
  schema: z.object({
    title: z.string(),
    company: z.string(),
    role: z.string(),
    startDate: z.coerce.date(),
    summary: z.string(),
    tags: z.array(z.string()),
  }),
});

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
  work,
  blog,
};
