import { defineCollection, z } from "astro:content";
import { glob } from "astro/loaders";

const projects = defineCollection({
  loader: glob({ pattern: "**/*.md", base: "./src/content/projects" }),
  schema: () =>
    z.object({
      title: z.string(),
      description: z.string(),
      tags: z.array(z.string()),
      startDate: z.coerce.date(),
      endDate: z.coerce.date().optional(),
      featured: z.boolean().optional().default(false),
      links: z.object({
        github: z.string().optional(),
        demo: z.string().optional(),
        website: z.string().optional(),
      }),
    }),
});

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
    draft: z.boolean().optional().default(false),
  }),
});

export const collections = {
  projects,
  work,
  blog,
};
