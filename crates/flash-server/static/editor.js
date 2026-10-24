// The advanced card editor (external file: CSP forbids inline script).
//
// Progressive enhancement over card_editor_body.html: the contenteditable
// fields mirror their HTML into hidden inputs (front_html / back_html) on
// every change, so a plain form post (or the htmx dialog post) carries
// exactly what is on screen. Formatting emits only markup the server's
// sanitizer keeps: b/i/u/s/sup/sub, lists, and hl-{hue} / hl-bg-{hue}
// spans (never inline styles). Media is uploaded on paste or drop and
// inserted as <img>/<audio>/<video src="/media/{id}">.
(function () {
  "use strict";

  /**
   * @param {string} id
   * @param {Record<string, string | number>} [vars]
   * @returns {string}
   */
  const tx = (id, vars) => (window.flashT === undefined ? id : window.flashT(id, vars));

  const MAX_UPLOAD = 100 * 1024 * 1024;
  /** @typedef {readonly [front: string, back: string, frontHint: string, backHint: string]} Labels */
  /** @type {Labels} */
  const BASIC = ["js-editor-front", "js-editor-back", "js-editor-question", "js-editor-answer"];
  /** @type {ReadonlyMap<string, Labels>} */
  const LABELS = new Map([
    ["basic", BASIC],
    ["basic_reversed", BASIC],
    ["basic_typed", ["js-editor-front", "js-editor-back", "js-editor-question", "js-editor-answer-to-type"]],
    ["cloze", ["js-editor-text", "js-editor-extra", "js-editor-cloze-hint", "js-editor-extra-hint"]],
  ]);

  /**
   * The browser's editing commands. Deprecated as an API, but there is no
   * standard replacement that edits a contenteditable field and keeps its
   * undo history, so every editor on the web still uses it: here, through
   * this one function.
   * @param {string} command
   * @param {string} [value]
   */
  function exec(command, value) {
    // eslint-disable-next-line @typescript-eslint/no-deprecated -- contenteditable has no other way to edit that keeps undo
    return document.execCommand(command, false, value);
  }

  const { all, maybe, composing, postMedia } = window.flashDom;

  /** @param {string} s */
  function escapeHtml(s) {
    return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
  }
  /** @param {string} t */
  function textToHtml(t) {
    return escapeHtml(t).replace(/\r?\n/g, "<br>");
  }

  /**
   * Pasted HTML: keep structure, drop every attribute (styles, classes,
   * handlers, external srcs). The server sanitizes again regardless.
   * @param {string} html
   */
  function cleanHtml(html) {
    const tpl = document.createElement("template");
    tpl.innerHTML = html;
    const walker = document.createTreeWalker(tpl.content, NodeFilter.SHOW_ELEMENT);
    /** @type {Element[]} */
    const nodes = [];
    while (walker.nextNode() !== null) {
      if (walker.currentNode instanceof Element) nodes.push(walker.currentNode);
    }
    for (const el of nodes) {
      const tag = el.tagName.toLowerCase();
      if (tag === "script" || tag === "style" || tag === "meta" || tag === "link" || tag === "iframe") {
        el.remove();
        continue;
      }
      const media = tag === "img" || tag === "audio" || tag === "video";
      const keepSrc = media && /^\/media\/\d+$/.test(el.getAttribute("src") ?? "");
      if (media && !keepSrc) {
        el.remove();
        continue;
      }
      for (const a of Array.from(el.attributes)) {
        if (keepSrc && (a.name === "src" || a.name === "controls")) continue;
        el.removeAttribute(a.name);
      }
    }
    return tpl.innerHTML;
  }

  /**
   * Where a drop lands, as a collapsed range inside `within`, or null.
   * caretPositionFromPoint is the standard; Safari before 18.4 has only
   * caretRangeFromPoint.
   * @param {number} x
   * @param {number} y
   * @param {Node} within
   */
  function caretFromPoint(x, y, within) {
    /** @type {{ caretPositionFromPoint?: Document["caretPositionFromPoint"], caretRangeFromPoint?: (x: number, y: number) => Range | null }} */
    const doc = document;
    /** @type {Range | null} */
    let range = null;
    if (doc.caretPositionFromPoint !== undefined) {
      const pos = doc.caretPositionFromPoint(x, y);
      if (pos !== null) {
        range = document.createRange();
        range.setStart(pos.offsetNode, pos.offset);
        range.collapse(true);
      }
    } else if (doc.caretRangeFromPoint !== undefined) {
      range = doc.caretRangeFromPoint(x, y);
    }
    if (range !== null && !within.contains(range.startContainer)) return null;
    return range;
  }

  /** @param {Element} el */
  function unwrap(el) {
    const parent = el.parentNode;
    if (parent === null) return;
    while (el.firstChild !== null) parent.insertBefore(el.firstChild, el);
    parent.removeChild(el);
  }

  /** @param {Range} range */
  function select(range) {
    const sel = window.getSelection();
    if (sel === null) return;
    sel.removeAllRanges();
    sel.addRange(range);
  }

  /** @param {HTMLElement} root */
  function initEditor(root) {
    if (root.dataset["editorInit"] !== undefined) return;
    root.dataset["editorInit"] = "1";
    const form = root.closest("form");
    const fields = all(root, ".editor-field", HTMLElement);
    const found = fields[0];
    if (found === undefined) return;
    const first = found;
    const typeSel = maybe(root, "[data-editor-type]", HTMLSelectElement);
    const status = maybe(root, "[data-editor-status]", HTMLElement);
    const fileInput = maybe(root, "[data-editor-file]", HTMLInputElement);
    let lastField = first;

    try {
      exec("styleWithCSS", "false");
    } catch {
      // Older engines.
    }

    /** @param {HTMLElement} field */
    const nameOf = (field) => field.dataset["field"] ?? "";
    /** @param {HTMLElement} field */
    const hiddenFor = (field) => maybe(root, `input[name="${nameOf(field)}_html"]`, HTMLInputElement);
    /** @param {HTMLElement} field */
    const sourceFor = (field) => maybe(root, `[data-source-for="${nameOf(field)}"]`, HTMLTextAreaElement);
    /** @param {HTMLElement} field */
    function sync(field) {
      const src = sourceFor(field);
      if (src !== null && src.hidden === false) field.innerHTML = src.value;
      const hidden = hiddenFor(field);
      if (hidden !== null) hidden.value = field.innerHTML;
    }
    function syncAll() {
      for (const field of fields) sync(field);
    }
    /** @param {string} msg */
    function note(msg) {
      if (status === null) return;
      status.textContent = msg;
      status.hidden = msg === "";
      if (msg !== "") {
        setTimeout(() => {
          if (status.textContent === msg) status.hidden = true;
        }, 6000);
      }
    }
    /** @param {Node | null} start */
    function fieldOf(start) {
      let node = start;
      while (node !== null && node !== root) {
        if (node instanceof HTMLElement && node.classList.contains("editor-field")) return node;
        node = node.parentNode;
      }
      return null;
    }
    function activeField() {
      return fieldOf(window.getSelection()?.anchorNode ?? null) ?? lastField;
    }
    /** @param {HTMLElement} field */
    function selectionIn(field) {
      const sel = window.getSelection();
      if (sel === null || sel.rangeCount === 0) return null;
      const range = sel.getRangeAt(0);
      if (fieldOf(range.commonAncestorContainer) !== field) return null;
      return range;
    }
    /** @param {HTMLElement} field */
    function placeCaretAtEnd(field) {
      const r = document.createRange();
      r.selectNodeContents(field);
      r.collapse(false);
      select(r);
    }

    // ---- type picker ----
    function applyType() {
      const t = typeSel === null ? (root.dataset["type"] ?? "basic") : typeSel.value;
      root.dataset["type"] = t;
      const ids = LABELS.get(t) ?? BASIC;
      const [front, back, frontHint, backHint] = [tx(ids[0]), tx(ids[1]), tx(ids[2]), tx(ids[3])];
      const fl = root.querySelector("[data-front-label]");
      const bl = root.querySelector("[data-back-label]");
      if (fl !== null) fl.textContent = front;
      if (bl !== null) bl.textContent = back;
      first.dataset["placeholder"] = frontHint;
      const second = fields[1];
      if (second !== undefined) second.dataset["placeholder"] = backHint;
    }
    if (typeSel !== null) typeSel.addEventListener("change", applyType);
    applyType();

    // ---- fields ----
    for (const field of fields) {
      field.addEventListener("input", () => {
        sync(field);
      });
      field.addEventListener("focus", () => {
        lastField = field;
      });
      field.addEventListener("paste", (e) => {
        const cd = e.clipboardData;
        if (cd === null) return;
        if (cd.files.length > 0) {
          e.preventDefault();
          for (const f of cd.files) uploadLater(field, f, null);
          return;
        }
        const html = cd.getData("text/html");
        const text = cd.getData("text/plain");
        if (html !== "") {
          e.preventDefault();
          exec("insertHTML", cleanHtml(html));
        } else if (text !== "") {
          e.preventDefault();
          exec("insertText", text);
        }
        sync(field);
      });
      let depth = 0;
      field.addEventListener("dragenter", (e) => {
        e.preventDefault();
        depth++;
        field.classList.add("is-drag");
      });
      field.addEventListener("dragover", (e) => {
        e.preventDefault();
      });
      field.addEventListener("dragleave", () => {
        depth = Math.max(0, depth - 1);
        if (depth === 0) field.classList.remove("is-drag");
      });
      field.addEventListener("drop", (e) => {
        depth = 0;
        field.classList.remove("is-drag");
        const files = e.dataTransfer?.files;
        if (files === undefined || files.length === 0) return;
        e.preventDefault();
        const range = caretFromPoint(e.clientX, e.clientY, field);
        for (const f of files) uploadLater(field, f, range);
      });
      field.addEventListener("keydown", (e) => {
        if (composing(e)) return;
        const mod = e.ctrlKey || e.metaKey;
        if (mod && e.shiftKey && (e.key === "C" || e.key === "c")) {
          e.preventDefault();
          insertCloze(field, e.altKey);
        } else if (mod && e.key === "Enter") {
          e.preventDefault();
          form?.requestSubmit();
        }
      });
    }
    for (const src of all(root, ".editor-source", HTMLTextAreaElement)) {
      src.addEventListener("input", () => {
        const field = maybe(root, `.editor-field[data-field="${src.dataset["sourceFor"] ?? ""}"]`, HTMLElement);
        if (field !== null) sync(field);
      });
    }

    // ---- media ----
    /**
     * @param {HTMLElement} field
     * @param {File} file
     * @param {Range | null} range
     */
    async function upload(field, file, range) {
      if (file.size > MAX_UPLOAD) {
        note(`${file.name} is larger than 100 MB.`);
        return;
      }
      field.focus();
      lastField = field;
      if (range !== null) {
        select(range);
      } else if (selectionIn(field) === null) {
        placeCaretAtEnd(field);
      } else {
        // Never swallow a highlighted selection: attach after it.
        window.getSelection()?.collapseToEnd();
      }
      const id = `up${Date.now().toString(36)}${Math.random().toString(36).slice(2, 7)}`;
      exec(
        "insertHTML",
        `<span class="editor-uploading" id="${id}" contenteditable="false">${escapeHtml(tx("js-editor-uploading", { name: file.name }))}</span>&nbsp;`,
      );
      sync(field);
      try {
        const reply = await postMedia(file, "upload");
        if ("error" in reply) throw new Error(reply.error ?? tx("js-editor-upload-failed"));
        const { id: mediaId, kind } = reply;
        /** @type {HTMLElement} */
        let el;
        if (kind === "image") {
          const img = document.createElement("img");
          img.src = `/media/${mediaId}`;
          img.alt = "";
          el = img;
        } else {
          const player = document.createElement(kind === "video" ? "video" : "audio");
          player.controls = true;
          player.preload = "none";
          player.src = `/media/${mediaId}`;
          el = player;
        }
        const ph = document.getElementById(id);
        if (ph === null) field.appendChild(el);
        else ph.replaceWith(el);
        sync(field);
      } catch (err) {
        document.getElementById(id)?.remove();
        sync(field);
        note(err instanceof Error && err.message !== "" ? err.message : tx("js-editor-upload-failed"));
      }
    }
    /**
     * @param {HTMLElement} field
     * @param {File} file
     * @param {Range | null} range
     */
    function uploadLater(field, file, range) {
      upload(field, file, range).catch((err) => {
        console.error("editor.js: uploading:", err);
      });
    }
    const attach = root.querySelector("[data-attach]");
    if (attach !== null && fileInput !== null) {
      attach.addEventListener("mousedown", (e) => {
        e.preventDefault();
      });
      attach.addEventListener("click", () => {
        fileInput.click();
      });
      fileInput.addEventListener("change", () => {
        const field = activeField();
        for (const f of fileInput.files ?? []) uploadLater(field, f, null);
        fileInput.value = "";
      });
    }

    // ---- formatting ----
    /**
     * @param {string} tag
     * @param {string} cls
     * @param {string} prefix
     */
    function wrapSelection(tag, cls, prefix) {
      const field = activeField();
      const range = selectionIn(field);
      if (range === null || range.collapsed) {
        note(tx("js-editor-select-first"));
        return;
      }
      // Toggle/swap when the whole selection already sits inside a match.
      /** @type {Node | null} */
      let node = range.commonAncestorContainer;
      while (node !== null && node !== field) {
        if (node instanceof Element && node.tagName.toLowerCase() === tag) {
          const has =
            prefix === "" ||
            Array.from(node.classList).some((c) => c.startsWith(prefix) && (prefix !== "hl-" || !c.startsWith("hl-bg-")));
          if (has) {
            if (cls !== "" && node.className !== cls) node.className = cls;
            else unwrap(node);
            sync(field);
            return;
          }
        }
        node = node.parentNode;
      }
      if (cls === "" && prefix !== "") {
        clearWithin(field, range, `span[class^="${prefix}"]`, prefix);
        sync(field);
        return;
      }
      const el = document.createElement(tag);
      if (cls !== "") el.className = cls;
      try {
        range.surroundContents(el);
      } catch {
        el.appendChild(range.extractContents());
        range.insertNode(el);
      }
      const r = document.createRange();
      r.selectNodeContents(el);
      select(r);
      sync(field);
    }
    /**
     * @param {HTMLElement} field
     * @param {Range} range
     * @param {string} selector
     * @param {string} prefix
     */
    function clearWithin(field, range, selector, prefix) {
      for (const el of field.querySelectorAll(selector)) {
        if (!range.intersectsNode(el)) continue;
        if (prefix === "hl-") {
          // Text-color clear must leave highlight spans alone and vice versa.
          const isBg = el.className.startsWith("hl-bg-");
          if (selector.includes("hl-bg-") ? !isBg : isBg) continue;
        }
        unwrap(el);
      }
    }
    /** @param {HTMLElement} field */
    function clearFormatting(field) {
      const before = selectionIn(field);
      if (before === null || before.collapsed) {
        note(tx("js-editor-select-first"));
        return;
      }
      exec("removeFormat");
      const range = selectionIn(field) ?? before;
      for (const el of field.querySelectorAll('span[class^="hl-"], s, sup, sub, b, i, u, strong, em')) {
        if (range.intersectsNode(el)) unwrap(el);
      }
    }
    for (const btn of all(root, "[data-cmd]", HTMLElement)) {
      btn.addEventListener("mousedown", (e) => {
        e.preventDefault();
      });
      btn.addEventListener("click", () => {
        const field = activeField();
        field.focus();
        const cmd = btn.dataset["cmd"] ?? "";
        if (cmd === "strike") wrapSelection("s", "", "");
        else if (cmd === "clear") clearFormatting(field);
        else exec(cmd);
        sync(field);
      });
    }
    for (const details of all(root, ".editor-hues", HTMLDetailsElement)) {
      details.querySelector("summary")?.addEventListener("mousedown", (e) => {
        e.preventDefault();
      });
      for (const sw of all(details, ".editor-swatch", HTMLElement)) {
        sw.addEventListener("mousedown", (e) => {
          e.preventDefault();
        });
        sw.addEventListener("click", () => {
          const bg = sw.hasAttribute("data-hl-bg");
          const hue = (bg ? sw.dataset["hlBg"] : sw.dataset["hl"]) ?? "";
          const prefix = bg ? "hl-bg-" : "hl-";
          wrapSelection("span", hue === "" ? "" : prefix + hue, prefix);
          details.open = false;
        });
      }
    }
    document.addEventListener("click", (e) => {
      for (const d of all(root, ".editor-hues[open]", HTMLDetailsElement)) {
        if (!(e.target instanceof Node) || !d.contains(e.target)) d.open = false;
      }
    });

    // ---- cloze ----
    /**
     * @param {HTMLElement} field
     * @param {boolean} reuse
     */
    function insertCloze(field, reuse) {
      if (root.dataset["type"] !== "cloze" && typeSel !== null) {
        typeSel.value = "cloze";
        applyType();
      }
      let range = selectionIn(field);
      if (range === null) {
        field.focus();
        placeCaretAtEnd(field);
        range = selectionIn(field);
      }
      if (range === null) return;
      let max = 0;
      for (const m of field.innerHTML.matchAll(/\{\{c(\d+)::/g)) max = Math.max(max, Number(m[1]));
      const n = reuse ? Math.max(1, max) : max + 1;
      const frag = range.extractContents();
      // Word selections (double-click, Ctrl+Shift+Arrow) drag the trailing
      // space along; keep it outside the blank so the sentence reads on.
      let trailing = "";
      let last = frag.lastChild;
      while (last?.lastChild != null) last = last.lastChild;
      if (last instanceof Text) {
        const space = /\s+$/.exec(last.data)?.[0];
        if (space !== undefined && space.length < last.data.length) {
          trailing = space;
          last.data = last.data.slice(0, -trailing.length);
        }
      }
      const wrap = document.createDocumentFragment();
      wrap.appendChild(document.createTextNode(`{{c${n}::`));
      const empty = frag.textContent === "";
      wrap.appendChild(frag);
      const close = document.createTextNode(`}}${trailing}`);
      wrap.appendChild(close);
      range.insertNode(wrap);
      const r = document.createRange();
      if (empty) r.setStart(close, 0);
      else r.setStartAfter(close);
      r.collapse(true);
      select(r);
      sync(field);
    }
    const clozeBtn = root.querySelector("[data-cloze]");
    if (clozeBtn !== null) {
      clozeBtn.addEventListener("mousedown", (e) => {
        e.preventDefault();
      });
      clozeBtn.addEventListener("click", () => {
        insertCloze(activeField(), false);
      });
    }

    // ---- HTML source view ----
    const srcBtn = root.querySelector("[data-source]");
    if (srcBtn !== null) {
      srcBtn.addEventListener("click", () => {
        const on = !srcBtn.classList.contains("is-active");
        for (const field of fields) {
          const src = sourceFor(field);
          if (src === null) continue;
          if (on) {
            src.value = field.innerHTML;
            src.hidden = false;
            field.hidden = true;
          } else {
            field.innerHTML = src.value;
            src.hidden = true;
            field.hidden = false;
          }
          sync(field);
        }
        srcBtn.classList.toggle("is-active", on);
      });
    }

    // ---- submit ----
    if (form !== null) {
      form.addEventListener(
        "submit",
        (e) => {
          syncAll();
          if (root.closest("[hidden]") !== null) return; // basic mode: the editor is dormant
          if (root.querySelector(".editor-uploading") !== null) {
            e.preventDefault();
            e.stopImmediatePropagation();
            note(tx("js-editor-wait-upload"));
          }
        },
        true,
      );
    }
  }

  /**
   * Deck page: the Advanced toggle turns the quick form into the editor.
   * @param {HTMLFormElement} form
   */
  function initToggle(form) {
    const btn = maybe(form, "[data-editor-toggle]", HTMLElement);
    const quick = maybe(form, "[data-quick-fields]", HTMLElement);
    const adv = maybe(form, ".editor-advanced", HTMLElement);
    if (btn === null || quick === null || adv === null || form.dataset["toggleInit"] !== undefined) return;
    form.dataset["toggleInit"] = "1";
    toggle(form, btn, quick, adv);
  }

  /**
   * @param {HTMLFormElement} form
   * @param {HTMLElement} btn the Advanced / Simple button
   * @param {HTMLElement} quick the plain fields
   * @param {HTMLElement} adv the rich editor
   */
  function toggle(form, btn, quick, adv) {
    const textareas = Array.from(quick.querySelectorAll("textarea"));
    const fields = all(adv, ".editor-field", HTMLElement);
    /**
     * @param {boolean} on
     * @param {boolean} focus
     */
    function set(on, focus) {
      adv.hidden = !on;
      quick.hidden = on;
      btn.setAttribute("aria-expanded", on ? "true" : "false");
      btn.textContent = on ? tx("js-editor-simple") : tx("js-editor-advanced");
      form.action = (on ? form.dataset["notesAction"] : form.dataset["basicAction"]) ?? form.action;
      for (const t of textareas) t.required = !on;
      if (on) {
        // Carry anything already typed into the rich fields, once.
        textareas.forEach((t, i) => {
          const f = fields[i];
          if (f?.textContent.trim() === "" && t.value.trim() !== "") {
            f.innerHTML = textToHtml(t.value);
            f.dispatchEvent(new Event("input"));
          }
        });
        if (focus) fields[0]?.focus();
      } else {
        fields.forEach((f, i) => {
          const t = textareas[i];
          if (t?.value.trim() === "" && f.textContent.trim() !== "") t.value = f.textContent;
        });
      }
      try {
        if (on) localStorage.setItem("flash.editor.advanced", "1");
        else localStorage.removeItem("flash.editor.advanced");
      } catch {
        // Storage blocked: the choice lasts as long as the page.
      }
    }
    // The form is boosted, and htmx records a boosted form's path when it
    // first processes the form: the action set above would never be used,
    // and the rich fields went to the plain route, which refused them for
    // their empty plain twins. The request takes the form's action here.
    form.addEventListener("htmx:configRequest", (e) => {
      if (e.detail.elt === form) e.detail.path = form.getAttribute("action") ?? e.detail.path;
    });
    btn.hidden = false;
    btn.addEventListener("click", () => {
      set(adv.hidden !== false, true);
    });
    let remembered = false;
    try {
      remembered = localStorage.getItem("flash.editor.advanced") === "1";
    } catch {
      // Storage blocked: the simple form, as on a first visit.
    }
    if (remembered) set(true, false);
  }

  // Edit dialog lifecycle.
  function closeDialog() {
    const host = document.getElementById("editor-dialog");
    if (host === null) return;
    // A restore already saved the card, so the rows behind the dialog
    // are out of date: closing reloads them, as a save does. The study
    // page shows its card again instead (study.js), keeping its session.
    const changed = host.querySelector("[data-editor-changed]");
    host.replaceChildren();
    // Tell the page how the dialog closed (nothing saved, or the card changed)
    // so a page showing a card can pick up where it was.
    if (changed === null) {
      host.dispatchEvent(new CustomEvent("flash:editor-closed", { bubbles: true }));
    } else if (host.dataset["editorAfter"] === "study") {
      host.dispatchEvent(new CustomEvent("flash:card-changed", { bubbles: true }));
    } else {
      window.location.reload();
    }
  }
  // A save from the study page answers with this event (HX-Trigger), not
  // a reload: the dialog closes and study.js shows the card again.
  document.addEventListener("flash:card-saved", () => {
    document.getElementById("editor-dialog")?.replaceChildren();
  });
  /** @param {HTMLElement} host */
  function initDialog(host) {
    host.dispatchEvent(new CustomEvent("flash:editor-opened", { bubbles: true }));
    for (const b of host.querySelectorAll("[data-editor-close]")) b.addEventListener("click", closeDialog);
    const overlay = host.querySelector("[data-editor-overlay]");
    if (overlay !== null) {
      overlay.addEventListener("mousedown", (e) => {
        if (e.target === overlay) closeDialog();
      });
    }
    maybe(host, ".editor-field", HTMLElement)?.focus();
  }
  document.addEventListener("keydown", (e) => {
    if (composing(e)) return;
    if (e.key === "Escape" && document.querySelector("#editor-dialog [data-editor-overlay]") !== null) {
      e.preventDefault();
      const open = maybe(document, "#editor-dialog .editor-hues[open]", HTMLDetailsElement);
      if (open !== null) {
        open.open = false;
        return;
      }
      closeDialog();
    }
  });

  /** @param {ParentNode} scope */
  function init(scope) {
    for (const root of all(scope, "[data-editor]", HTMLElement)) initEditor(root);
    for (const form of all(scope, "form[data-editor-toggle], form[data-basic-action]", HTMLFormElement)) initToggle(form);
  }
  document.addEventListener("DOMContentLoaded", () => {
    init(document);
    document.body.addEventListener("htmx:afterSwap", (e) => {
      if (!(e.target instanceof HTMLElement)) return;
      init(e.target);
      if (e.target.id === "editor-dialog") initDialog(e.target);
    });
  });
})();
