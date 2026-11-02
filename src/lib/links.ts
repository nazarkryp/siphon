const URL_PATTERN = /https?:\/\/[^\s"'<>]+/gi;

/** All http(s) URLs in the text, in order, without duplicates. */
export function extractUrls(text: string): string[] {
  const found = text.match(URL_PATTERN) ?? [];
  return [...new Set(found)];
}

const FILE_EXTENSIONS = /\.(mp4|mkv|webm|mov|avi|mp3|m4a|zip|rar|7z|tar|gz|iso|pdf|exe|msi)$/i;

export function describeLink(url: string): string {
  let path: string;
  try {
    path = new URL(url).pathname;
  } catch {
    return "a link";
  }
  if (/\.m3u8$/i.test(path)) return "looks like an HLS playlist";
  if (FILE_EXTENSIONS.test(path)) return "looks like a file";
  return "looks like a web page";
}
