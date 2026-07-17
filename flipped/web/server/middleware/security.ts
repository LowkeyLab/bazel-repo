export default defineEventHandler((event) => {
  setResponseHeaders(event, {
    "cache-control": event.path.startsWith("/api/") ? "no-store" : "no-cache",
    "referrer-policy": "no-referrer",
    "x-content-type-options": "nosniff",
    "x-frame-options": "DENY",
  });
});
