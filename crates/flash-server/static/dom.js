(function () {
  "use strict";

  /**
   * The element `selector` finds under `root`, as a `Ctor`; throws, naming
   * the selector, when the page does not have it.
   * @template {Element} T
   * @param {ParentNode} root
   * @param {string} selector
   * @param {new () => T} Ctor
   * @returns {T}
   */
  function one(root, selector, Ctor) {
    const el = root.querySelector(selector);
    if (!(el instanceof Ctor)) throw new Error(`no ${Ctor.name} at ${selector}`);
    return el;
  }

  /**
   * @template {Element} T
   * @param {ParentNode} root
   * @param {string} selector
   * @param {new () => T} Ctor
   * @returns {T | null}
   */
  function maybe(root, selector, Ctor) {
    const el = root.querySelector(selector);
    return el instanceof Ctor ? el : null;
  }

  /**
   * @template {Element} T
   * @param {ParentNode} root
   * @param {string} selector
   * @param {new () => T} Ctor
   * @returns {T[]}
   */
  function all(root, selector, Ctor) {
    return Array.from(root.querySelectorAll(selector), (el) => {
      if (!(el instanceof Ctor)) throw new Error(`${selector} is not a ${Ctor.name}`);
      return el;
    });
  }

  /**
   * @param {EventTarget | null} target
   * @param {string} selector
   * @returns {HTMLElement | null}
   */
  function closestTo(target, selector) {
    const el = target instanceof Element ? target.closest(selector) : null;
    return el instanceof HTMLElement ? el : null;
  }

  /** @param {unknown} value @returns {value is Record<string, unknown>} */
  function isRecord(value) {
    return typeof value === "object" && value !== null && !Array.isArray(value);
  }

  /** @param {KeyboardEvent} e */
  function composing(e) {
    // eslint-disable-next-line @typescript-eslint/no-deprecated -- Safari ends a composition with a keydown whose isComposing is false; keyCode 229 is its only sign
    return e.isComposing || e.keyCode === 229;
  }

  /**
   * @param {File} file
   * @param {string} fallbackName
   * @returns {Promise<{ body: ArrayBuffer | Blob, type: string }>}
   */
  async function formBytes(file, fallbackName) {
    // Bytes, not FormData: WebKit for Windows can send a FormData part
    // empty, even one already read into memory.
    const boundary = `flash-${crypto.getRandomValues(new Uint32Array(4)).join("-")}`;
    const name = (file.name === "" ? fallbackName : file.name).replace(/"/g, "%22").replace(/\r/g, "%0D").replace(/\n/g, "%0A");
    const type = file.type === "" ? "application/octet-stream" : file.type;
    const head = `--${boundary}\r\nContent-Disposition: form-data; name="file"; filename="${name}"\r\nContent-Type: ${type}\r\n\r\n`;
    const blob = new Blob([head, file, `\r\n--${boundary}--\r\n`]);
    // shortcut: a file over 8 MiB goes unread, sparing memory, and keeps the
    // empty-part risk on WebKit for Windows (no one browses with it); read it
    // too if a real browser ever shows it.
    const body = file.size > 8 << 20 ? blob : await blob.arrayBuffer();
    return { body, type: `multipart/form-data; boundary=${boundary}` };
  }

  /**
   * @param {File} file
   * @param {string} fallbackName
   * @returns {Promise<{ id: number, kind: string } | { error: string | null }>}
   */
  async function postMedia(file, fallbackName) {
    const form = await formBytes(file, fallbackName);
    const r = await fetch("/media", { method: "POST", body: form.body, headers: { "Content-Type": form.type }, credentials: "same-origin" });
    /** @type {unknown} */
    const reply = await r.json().catch(() => null);
    if (!isRecord(reply)) return { error: null };
    const { id, kind, error } = reply;
    if (r.ok && typeof id === "number" && Number.isSafeInteger(id) && id > 0) {
      return { id, kind: typeof kind === "string" ? kind : "" };
    }
    return { error: typeof error === "string" ? error : null };
  }

  window.flashDom = { one, maybe, all, closestTo, isRecord, composing, formBytes, postMedia };
})();
