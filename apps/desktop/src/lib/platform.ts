export const isMac = () => /mac|iphone|ipad/i.test(navigator.platform);

/** A shortcut the way this system writes it: `⌘K` or `Ctrl+K`. */
export const shortcut = (key: string) => (isMac() ? `⌘${key}` : `Ctrl+${key}`);
