export function calculateReadingTime(content: string) {
  const words = content.split(/\s+/).length;
  return Math.ceil(words / 200);
}
