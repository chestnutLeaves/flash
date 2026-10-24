// What breaks here. An error thrown by one of this site's own scripts, or
// a promise of theirs that failed with nobody to catch it, is reported to
// the server (`/client-errors`), which counts it and, when it is new,
// tells whoever runs the site. Only our scripts, the ones under /static/:
// an extension's error or another site's is not ours and is never sent.
// What is sent: the kind, the message (300 characters), the script's
// path and line, and the page's path; nothing about who saw it. At most
// five a page, each once. Loaded first on every page, so it hears the
// scripts that run after it.
(function () {
  "use strict";

  const LIMIT = 5;
  const OURS = window.location.origin + "/static/";
  /** @type {Set<string>} */
  const sent = new Set();

  /**
   * A script's path when it is one of ours, else null.
   * @param {string} url
   * @returns {string | null}
   */
  function ours(url) {
    if (!url.startsWith(OURS)) return null;
    return url.slice(window.location.origin.length).split(/[?#]/)[0] ?? null;
  }

  /**
   * The first of our scripts a stack names, and the line in it. Chrome
   * writes `at f (https://site/static/x.js?v=1:12:3)`, Firefox and
   * Safari `f@https://site/static/x.js?v=1:12:3`.
   * @param {string} stack
   * @returns {{ file: string, line: number } | null}
   */
  function fromStack(stack) {
    for (const match of stack.matchAll(/(https?:\/\/[^\s()@]+?):(\d+):\d+/g)) {
      const file = ours(match[1] ?? "");
      if (file !== null) return { file, line: Number(match[2] ?? "0") };
    }
    return null;
  }

  /**
   * @param {"error" | "rejection"} kind
   * @param {string} message
   * @param {string} file
   * @param {number} line
   */
  function send(kind, message, file, line) {
    if (sent.size >= LIMIT) return;
    const key = `${kind} ${file}:${String(line)} ${message}`;
    if (sent.has(key)) return;
    sent.add(key);
    const body = JSON.stringify({
      kind,
      message: message.slice(0, 300),
      file,
      line,
      page: window.location.pathname,
    });
    try {
      navigator.sendBeacon("/client-errors", body);
    } catch {
      // Nowhere left to say so.
    }
  }

  window.addEventListener("error", (e) => {
    const file = ours(e.filename);
    if (file === null) return;
    send("error", e.message, file, e.lineno);
  });

  window.addEventListener("unhandledrejection", (e) => {
    const reason = /** @type {unknown} */ (e.reason);
    if (!(reason instanceof Error)) return;
    const where = fromStack(reason.stack ?? "");
    if (where === null) return;
    send("rejection", `${reason.name}: ${reason.message}`, where.file, where.line);
  });
})();
