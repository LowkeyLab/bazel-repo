import { defineCollection, z } from "astro:content";
import { glob } from "astro/loaders";

const projects = defineCollection({
  loader: glob({ pattern: "**/*.md", base: "./src/content/projects" }),
  schema: ({ image }) =>
    z.object({
      title: z.string(),
      description: z.string(),
      tags: z.array(z.string()),
      featured: z.boolean().optional().default(false),
      links: z.object({
        github: z.string().optional(),
        demo: z.string().optional(),
        website: z.string().optional(),
      }),
      cover: image().optional(),
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

export const collections = {
  projects,
  work,
};
