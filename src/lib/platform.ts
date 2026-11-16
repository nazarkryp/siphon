// WKWebView reports "Macintosh" in the user agent. The macOS window keeps the native traffic lights,
// so the title bar must not draw its own window buttons there.
export const isMac = /Macintosh|Mac OS X/.test(navigator.userAgent);

export const isWindows = /Windows/.test(navigator.userAgent);

// The player that opens a file when the user double-clicks it. Used in texts about formats it cannot open.
export const defaultPlayer = isMac ? "QuickTime" : isWindows ? "Media Player" : "the default player";

// The same name at the start of a sentence.
export const DefaultPlayer = defaultPlayer[0].toUpperCase() + defaultPlayer.slice(1);
