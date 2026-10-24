// The study page: keyboard shortcuts, archiving and the tag picker.
(function () {
  "use strict";

  const { one, all, closestTo, composing } = window.flashDom;

  /**
   * Whether a key press belongs to something other than studying: someone
   * typing into a field (the card editor's are contenteditable), or
   * anything while the edit dialog is up over the card.
   * @param {EventTarget | null} target
   */
  function typingOrEditing(target) {
    if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) return true;
    if (target instanceof HTMLElement && target.isContentEditable) return true;
    return document.querySelector("#editor-dialog [data-editor-overlay]") !== null;
  }

  /** @param {string} id */
  function press(id) {
    const button = document.getElementById(id);
    if (button === null) return false;
    button.click();
    return true;
  }

  // Study-view keyboard shortcuts: space reveals, 1-4 grades. In the
  // typed-answer input, Enter reveals (and other shortcuts stay inert so
  // typing "1" doesn't grade).
  document.addEventListener("keydown", (e) => {
    if (composing(e)) return;
    if (e.target instanceof Element && e.target.id === "type-input") {
      if (e.key === "Enter") {
        e.preventDefault();
        press("reveal-btn");
      }
      return;
    }
    if (typingOrEditing(e.target)) return;
    const id = e.key === " " ? "reveal-btn" : /^[1-4]$/.test(e.key) ? `grade-${e.key}` : null;
    if (id === null) return;
    const button = document.getElementById(id);
    if (button === null) return;
    e.preventDefault();
    button.click();
  });

  // Edit: the card on screen in the card editor's dialog (editor.js runs
  // it). A save, or a restore from its History, answers with an event
  // instead of a reload, and the card shows again in place: the same
  // session, counts, and front or answer, with the time spent editing
  // left out of the answer time.
  (function () {
    if (document.getElementById("study") === null) return;
    /** When the dialog was opened, to take that time out of the answer's. */
    let editFrom = 0;

    /** @returns {HTMLElement | null} */
    function card() {
      const el = document.querySelector("#study .study-card");
      return el instanceof HTMLElement ? el : null;
    }

    /**
     * Swaps a server render into the page, as the card's own buttons do.
     * @param {string} path
     * @param {string} target
     */
    function load(path, target) {
      const htmx = window.htmx;
      if (htmx === undefined) throw new Error("study.js: htmx is not loaded");
      htmx.ajax("GET", path, { target, swap: "innerHTML" }).catch((err) => {
        console.error("study.js: loading", path, err);
      });
    }

    document.addEventListener("click", (e) => {
      if (closestTo(e.target, "[data-study-edit-open]") === null) return;
      e.preventDefault();
      const c = card();
      const id = c?.dataset["cardId"];
      if (id === undefined) return;
      editFrom = Date.now();
      load(`/cards/${id}/edit?after=study`, "#editor-dialog");
    });

    // The card again, as it was when Edit was pressed.
    function again() {
      const c = card();
      if (c === null) return;
      const d = c.dataset;
      const q = new URLSearchParams();
      for (const key of ["session", "remaining", "total"]) q.set(key, d[key] ?? "0");
      q.set("card", d["cardId"] ?? "0");
      const shown = Number(d["shown"] ?? "0");
      // 0 means the front time was not measured; it stays so.
      const editing = editFrom > 0 ? Date.now() - editFrom : 0;
      q.set("shown", String(shown > 0 ? shown + editing : 0));
      if (d["revealed"] !== undefined) q.set("revealed", "true");
      const typed = d["typed"];
      if (typed !== undefined && typed !== "") q.set("typed", typed);
      const next = d["nextCard"];
      if (next !== undefined && next !== "") q.set("next", next);
      editFrom = 0;
      load(`/study/card?${q.toString()}`, "#study");
    }
    document.addEventListener("flash:card-saved", again);
    document.addEventListener("flash:card-changed", again);
    document.addEventListener("flash:editor-closed", () => {
      editFrom = 0;
    });

  })();

  (function () {
    if (document.getElementById("study") === null) return;
    let busy = false;

    function archive() {
      const card = document.querySelector("#study .study-card");
      if (!(card instanceof HTMLElement) || busy) return;
      const htmx = window.htmx;
      if (htmx === undefined) throw new Error("study.js: htmx is not loaded");
      const d = card.dataset;
      busy = true;
      htmx
        .ajax("POST", "/study/archive", {
          target: "#study",
          swap: "innerHTML",
          values: { session: d["session"] ?? "0", card: d["cardId"] ?? "0", total: d["total"] ?? "0" },
        })
        .catch((err) => {
          console.error("study.js: archiving the card:", err);
        })
        .finally(() => {
          busy = false;
        });
    }

    document.addEventListener("click", (e) => {
      if (closestTo(e.target, "[data-study-archive]") === null) return;
      e.preventDefault();
      archive();
    });

    document.addEventListener("keydown", (e) => {
      if (typingOrEditing(e.target) || e.ctrlKey || e.metaKey || e.altKey) return;
      if (e.key === "@") {
        e.preventDefault();
        archive();
      } else if ((e.key === "z" || e.key === "Z") && press("undo-archive")) {
        e.preventDefault();
      }
    });
  })();

  // The tag picker (templates/study_tags.html). The form is a real GET to
  // /study, so the browser builds and encodes the URL and a pick starts a
  // new session exactly as a deck link does; this only filters the rows as
  // the user types and fills the hidden field from the boxes. Escape and
  // the backdrop are the dialog's own.
  (function () {
    const dialog = document.getElementById("study-tags-dialog");
    const open = document.querySelector("[data-study-tags-open]");
    if (!(dialog instanceof HTMLDialogElement) || open === null) return;
    const form = one(dialog, "[data-study-tags-form]", HTMLFormElement);
    const value = one(dialog, "[data-study-tags-value]", HTMLInputElement);
    // Without tags the dialog only says so: no filter, rows or buttons.
    const filter = dialog.querySelector("[data-study-tags-filter]");
    const none = dialog.querySelector("[data-study-tags-none]");
    const clear = dialog.querySelector("[data-study-tags-clear]");
    const rows = all(dialog, ".study-tags-row", HTMLElement).map((row) => ({
      row,
      tag: row.dataset["tag"] ?? "",
      box: one(row, "input", HTMLInputElement),
    }));

    open.addEventListener("click", (e) => {
      e.preventDefault();
      dialog.showModal();
      if (filter instanceof HTMLInputElement) filter.focus();
    });
    for (const b of all(dialog, "[data-study-tags-close]", HTMLElement)) {
      b.addEventListener("click", () => {
        dialog.close();
      });
    }

    if (filter instanceof HTMLInputElement) {
      filter.addEventListener("input", () => {
        const q = filter.value.trim().toLowerCase();
        let shown = 0;
        for (const { row, tag } of rows) {
          const hit = q === "" || tag.includes(q);
          row.hidden = !hit;
          if (hit) shown++;
        }
        if (none instanceof HTMLElement) none.hidden = shown > 0;
      });
    }

    // A hidden field with no value would still put "tags=" in the URL, so
    // an empty pick disables it and the link reads /study or /study?deck=N.
    function fill() {
      const picked = rows.filter(({ box }) => box.checked).map(({ tag }) => tag);
      value.value = picked.join(",");
      value.disabled = picked.length === 0;
    }
    form.addEventListener("submit", fill);

    if (clear !== null) {
      clear.addEventListener("click", () => {
        for (const { box } of rows) box.checked = false;
        fill();
        form.requestSubmit();
      });
    }
  })();
})();
