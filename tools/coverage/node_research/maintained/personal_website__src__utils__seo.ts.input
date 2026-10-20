export function canonicalUrl(path: string, site: URL): string {
  const url = new URL(path, site);
  url.search = "";
  url.hash = "";
  if (!url.pathname.endsWith(".xml") && !url.pathname.endsWith("/"))
    url.pathname += "/";
  return url.href;
}
export function escapeXml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&apos;");
}
