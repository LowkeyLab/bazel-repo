export function calculateReadingTime(content) {
  const words = content.split(/\s+/).length;
  return Math.ceil(words / 200);
}
