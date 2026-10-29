// Small shared UI behaviors (external file: CSP forbids inline script).
(function () {
  "use strict";

  /**
   * @param {string} id
   * @param {Record<string, string | number>} [vars]
   * @returns {string}
   */
  const tx = (id, vars) => (window.flashT === undefined ? id : window.flashT(id, vars));

  const { one, all, closestTo, composing } = window.flashDom;

  /**
   * An htmx swap's target as somewhere to look for elements.
   * @param {EventTarget | null} target
   * @returns {ParentNode | null}
   */
  function swapped(target) {
    return target instanceof Element || target instanceof Document ? target : null;
  }

  /** @param {() => void} fn */
  function onReady(fn) {
    document.addEventListener("DOMContentLoaded", fn);
  }

  // Math rendering (KaTeX, self-hosted): render on load and after every
  // htmx swap, scoped to the swapped subtree. \( \) inline, \[ \] display,
  // no bare $ (false-positive jank).
  {
    /** @param {EventTarget | null} root */
    const renderMath = (root) => {
      const render = window.renderMathInElement;
      if (render === undefined || !(root instanceof HTMLElement)) return;
      try {
        render(root, {
          delimiters: [
            { left: "\\(", right: "\\)", display: false },
            { left: "\\[", right: "\\]", display: true },
          ],
          throwOnError: false,
        });
      } catch {
        // Malformed math renders as its source text.
      }
    };
    onReady(() => {
      renderMath(document.body);
      document.body.addEventListener("htmx:afterSwap", (e) => {
        renderMath(e.target);
      });
    });
  }

  // The device's time zone, for the pages that show times in it
  // (`when.rs`), which say which zone they used (`data-zone` on <html>).
  // Only those pages keep it, in a cookie the server reads: a display
  // preference holding a zone name and nothing else. When the page's zone
  // is not this device's, it is loaded once more in the right one. Once
  // per tab and zone, so a browser that drops the cookie never loops.
  (function () {
    const shown = document.documentElement.getAttribute("data-zone");
    if (shown === null || shown === "") return;
    const COOKIE = "flash-tz";
    let zone = "";
    try {
      zone = Intl.DateTimeFormat().resolvedOptions().timeZone;
    } catch {
      return;
    }
    if (!/^[A-Za-z0-9_+\-/]{1,64}$/.test(zone)) return;
    const kept = /(?:^|; )flash-tz=([^;]*)/.exec(document.cookie)?.[1];
    if (kept !== zone) {
      const secure = location.protocol === "https:" ? "; Secure" : "";
      document.cookie = `${COOKIE}=${zone}; Path=/; SameSite=Lax; Max-Age=31536000${secure}`;
    }
    if (shown === zone) return;
    const key = `flash-tz-reloaded:${zone}`;
    try {
      if (sessionStorage.getItem(key) !== null) return;
      sessionStorage.setItem(key, "1");
    } catch {
      return;
    }
    location.reload();
  })();

  // Custom audio player: progressive enhancement over every card <audio>
  // element (the import pipeline emits bare `<audio controls>`; without JS
  // that native player still works). Dead simple: play/pause + progress +
  // duration. One clip at a time.
  {
    /** @param {number} t seconds */
    const fmt = (t) => {
      if (!Number.isFinite(t)) return "0:00";
      const s = Math.round(t);
      return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
    };

    const PLAY =
      '<svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M7 4.5v15l13-7.5z"></path></svg>';
    const PAUSE =
      '<svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><rect x="6" y="4.5" width="4" height="15"></rect><rect x="14" y="4.5" width="4" height="15"></rect></svg>';

    /** @param {HTMLAudioElement} a */
    const player = (a) => {
      a.dataset["flAudio"] = "1";
      a.removeAttribute("controls");
      a.classList.add("fl-audio-hidden");

      const wrap = document.createElement("span");
      wrap.className = "fl-audio";
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "fl-audio-btn";
      btn.setAttribute("aria-label", tx("js-ui-play-audio"));
      btn.innerHTML = PLAY;
      const track = document.createElement("span");
      track.className = "fl-audio-track";
      const fill = document.createElement("span");
      fill.className = "fl-audio-fill";
      track.appendChild(fill);
      const time = document.createElement("span");
      time.className = "fl-audio-time";
      time.textContent = "0:00";
      wrap.appendChild(btn);
      wrap.appendChild(track);
      wrap.appendChild(time);
      a.after(wrap);

      /** @param {boolean} playing */
      const setPlaying = (playing) => {
        btn.innerHTML = playing ? PAUSE : PLAY;
        btn.setAttribute("aria-label", playing ? tx("js-ui-pause-audio") : tx("js-ui-play-audio"));
      };
      btn.addEventListener("click", () => {
        if (a.paused) {
          // One clip at a time.
          for (const other of all(document, "audio[data-fl-audio]", HTMLAudioElement)) {
            if (other !== a) other.pause();
          }
          a.play().catch(() => {
            // A clip that will not play leaves the button as it was.
          });
        } else {
          a.pause();
        }
      });
      // A known length: not NaN before the metadata, nor 0.
      const known = () => !Number.isNaN(a.duration) && a.duration !== 0;
      // rAF-driven progress: sparse timeupdate events look laggy on short
      // clips, so track playback every frame while playing.
      let raf = 0;
      const paint = () => {
        if (known()) {
          fill.style.width = `${(a.currentTime / a.duration) * 100}%`;
          time.textContent = fmt(a.duration - a.currentTime);
        }
        if (!a.paused && !a.ended) raf = requestAnimationFrame(paint);
      };
      a.addEventListener("play", () => {
        setPlaying(true);
        cancelAnimationFrame(raf);
        raf = requestAnimationFrame(paint);
      });
      a.addEventListener("pause", () => {
        setPlaying(false);
        cancelAnimationFrame(raf);
      });
      a.addEventListener("loadedmetadata", () => {
        time.textContent = fmt(a.duration);
      });
      a.addEventListener("ended", () => {
        a.currentTime = 0;
        fill.style.width = "0%";
        time.textContent = fmt(a.duration);
        setPlaying(false);
        cancelAnimationFrame(raf);
      });
    };

    /** @param {ParentNode} root */
    const enhance = (root) => {
      // Deck rows flatten collapsibles (closed <details> hides content no
      // matter the CSS, so open them; CSS renders them inline, inert).
      for (const d of all(root, ".card-rich-row details", HTMLDetailsElement)) d.open = true;
      for (const a of all(root, ".card-rich audio, audio.fl-audio-src", HTMLAudioElement)) {
        if (a.dataset["flAudio"] === undefined) player(a);
      }
    };

    onReady(() => {
      enhance(document.body);
      document.body.addEventListener("htmx:afterSwap", (e) => {
        const root = swapped(e.target);
        if (root !== null) enhance(root);
      });
    });
  }

  // Dismissing the Connect card is optimistic: gone on click, the POST
  // rides along in the background (keepalive survives navigation).
  onReady(() => {
    for (const form of all(document, ".connect-cta-x", HTMLFormElement)) {
      form.addEventListener("submit", (e) => {
        e.preventDefault();
        form.closest(".connect-cta-wrap")?.remove();
        fetch(form.action, { method: "POST", keepalive: true, credentials: "same-origin" }).catch(() => {
          // The card is back on the next load; nothing else is lost.
        });
      });
    }
  });

  // Any button with data-copy-target copies that element's text. Delegated:
  // the buttons may live in blocks htmx re-renders, so a per-button
  // listener bound at load would be gone after the first swap.
  {
    /**
     * @param {HTMLElement} button
     * @param {HTMLElement} source
     */
    const copy = async (button, source) => {
      const old = button.textContent;
      try {
        await navigator.clipboard.writeText(source.textContent.trim());
        button.textContent = tx("js-ui-copied");
      } catch {
        // Clipboard blocked or insecure context.
        button.textContent = tx("js-ui-copy-failed");
      }
      setTimeout(() => {
        button.textContent = old;
      }, 1500);
    };
    document.addEventListener("click", (e) => {
      const button = e.target instanceof Element ? e.target.closest("[data-copy-target]") : null;
      if (!(button instanceof HTMLElement)) return;
      const source = document.getElementById(button.dataset["copyTarget"] ?? "");
      if (source === null) return;
      copy(button, source).catch((err) => {
        console.error("ui.js: copying:", err);
      });
    });
  }

  // Plain (non-htmx) form submits: the import steps take seconds, so the
  // submit button goes busy and says so instead of looking ignorable. The
  // scroll position is remembered too: a POST → redirect → reload would
  // otherwise snap the page to the top, which on a long settings page hides
  // the very badge that says what happened.
  onReady(() => {
    try {
      const saved = sessionStorage.getItem("flash-scroll");
      if (saved === null) return;
      sessionStorage.removeItem("flash-scroll");
      /** @type {unknown} */
      const at = JSON.parse(saved);
      if (typeof at !== "object" || at === null || !("path" in at) || !("y" in at)) return;
      if (at.path === location.pathname && typeof at.y === "number" && location.hash === "") {
        window.scrollTo(0, at.y);
      }
    } catch {
      // No storage, or something else in it: the page stays at the top.
    }
  });
  // Delegated: boosted forms are re-rendered by htmx swaps, and a listener
  // bound per form at load would be gone after the first one.
  document.addEventListener("submit", (e) => {
    // A submit something else stopped (a confirmation turned down, an
    // upload still running) is not on its way: nothing to wait for.
    if (e.defaultPrevented) return;
    // A form marked `data-instant` is answered by this page at once, with no
    // request (the deck lists' fold and "Show archived"): there is nothing
    // to wait for, so it never reads "Working…", which would also replace
    // the fold's drawn chevron with a word.
    const form =
      e.target instanceof Element ? e.target.closest('form[method="post"]:not([hx-post]):not([data-instant])') : null;
    if (form === null) return;
    const boosted = form.hasAttribute("hx-boost");
    if (!boosted) {
      // Only a real reload loses the scroll position.
      try {
        sessionStorage.setItem("flash-scroll", JSON.stringify({ path: location.pathname, y: window.scrollY }));
      } catch {
        // No storage: the page comes back at the top.
      }
    }
    const btn = form.querySelector("button[type=submit]");
    if (!(btn instanceof HTMLButtonElement) || btn.dataset["busy"] !== undefined) return;
    btn.dataset["busy"] = "1";
    // The markup, not the text: a button can carry an icon.
    btn.dataset["label"] = btn.innerHTML;
    btn.textContent = btn.dataset["busyLabel"] ?? "Working…";
    // Disable after the submit event so the button's value still posts. A
    // boosted form is swapped away on success and restored below on failure,
    // so it never needs disabling (htmx drops duplicate requests itself).
    if (!boosted) {
      setTimeout(() => {
        btn.disabled = true;
      }, 0);
    }
  });
  // A boosted submit that fails leaves its form in place: put the button back.
  document.addEventListener("htmx:afterRequest", (e) => {
    const root = swapped(e.target);
    if (!e.detail.failed || root === null) return;
    for (const btn of all(root, "button[data-busy]", HTMLButtonElement)) {
      btn.innerHTML = btn.dataset["label"] ?? "";
      delete btn.dataset["busy"];
    }
  });

  // File picker readout + drag-and-drop onto the form card.
  {
    /** @param {number} n bytes */
    const humanSize = (n) => {
      if (n >= 1048576) return `${(n / 1048576).toFixed(1)} MB`;
      if (n >= 1024) return `${Math.round(n / 1024)} KB`;
      return `${n} B`;
    };
    /** @param {HTMLElement} control */
    const init = (control) => {
      if (control.dataset["flFileInit"] !== undefined) return;
      control.dataset["flFileInit"] = "1";
      const input = one(control, "input[type=file]", HTMLInputElement);
      const readout = one(control, ".fl-file-name", HTMLElement);
      const idle = readout.textContent;
      const show = () => {
        const f = input.files?.[0];
        if (f === undefined) {
          readout.textContent = idle;
          readout.classList.remove("has-file");
        } else {
          readout.textContent = `${f.name} · ${humanSize(f.size)}`;
          readout.classList.add("has-file");
        }
      };
      input.addEventListener("change", show);
      const zone = control.closest("[data-fl-drop]");
      if (zone !== null) {
        // dragleave fires for every child the cursor crosses; count
        // enters/leaves so the highlight only drops when we truly leave.
        let depth = 0;
        zone.addEventListener("dragenter", (e) => {
          e.preventDefault();
          depth++;
          zone.classList.add("is-drag");
        });
        zone.addEventListener("dragover", (e) => {
          e.preventDefault();
        });
        zone.addEventListener("dragleave", () => {
          depth = Math.max(0, depth - 1);
          if (depth === 0) zone.classList.remove("is-drag");
        });
        zone.addEventListener("drop", (e) => {
          depth = 0;
          zone.classList.remove("is-drag");
          if (!(e instanceof DragEvent) || e.dataTransfer === null || e.dataTransfer.files.length === 0) return;
          e.preventDefault();
          input.files = e.dataTransfer.files;
          show();
        });
      }
      show();
    };
    onReady(() => {
      for (const control of all(document, "[data-fl-file]", HTMLElement)) init(control);
    });
  }

  // Status chips (saved, created, ...) are confirmations, not state: they
  // fade out a moment after they appear, on load and after htmx swaps.
  {
    const toastBadges = () => {
      for (const el of all(document, ".fl-badge-toast:not([data-toasted])", HTMLElement)) {
        el.dataset["toasted"] = "1";
        setTimeout(() => {
          el.classList.add("is-gone");
          setTimeout(() => {
            el.remove();
          }, 450);
        }, 2200);
      }
    };
    onReady(() => {
      toastBadges();
      // A full load that carried a status flag (?saved=… after a no-JS
      // submit, or a link someone kept) must not replay it on refresh.
      if (location.search !== "" && document.querySelector("[data-toasted]") !== null) {
        try {
          history.replaceState(null, "", location.pathname + location.hash);
        } catch {
          // The address keeps its flag; a refresh shows the chip again.
        }
      }
    });
    document.addEventListener("htmx:afterSwap", toastBadges);
  }

  // The theme picker: a theme applies the moment it is picked. The page
  // repaints at once, and the cookie that makes the next load match is set
  // by the same post the form makes, sent in the background with nothing
  // waiting on it (that cookie is the server's alone to write). The Save
  // button is for a browser without script, and is put away here.
  onReady(() => {
    const form = document.querySelector("[data-theme-form]");
    if (!(form instanceof HTMLFormElement)) return;
    const save = form.querySelector('button[type="submit"]');
    if (save instanceof HTMLElement) save.hidden = true;
    const select = one(form, 'select[name="theme"]', HTMLSelectElement);
    select.addEventListener("change", () => {
      document.documentElement.setAttribute("data-theme", select.value);
      const body = new URLSearchParams();
      for (const [name, value] of new FormData(form)) {
        if (typeof value === "string") body.append(name, value);
      }
      fetch(form.action, { method: "POST", body, credentials: "same-origin", redirect: "manual" }).catch(() => {
        // The page already changed; the next pick retries.
      });
    });
  });

  // Deck lists: showing archived decks and folding a deck shut. Neither
  // changes anything on the server, so neither makes a request: every row is
  // already in the page, a click shows or hides rows at once, and the choice
  // goes in a cookie so the server renders the next load, a refresh
  // included, the way it was left (no flash of the other state, which is
  // what keeping it in localStorage would cost). Every deck starts open; the
  // cookie names only the ones folded shut. The forms still post without
  // script. The rules here are `deck_list_rows` in web.rs, and the cookie
  // attributes are `pref_cookie_header` in auth.rs.
  {
    const FOLDED = "flash-decks-folded";
    const ARCHIVED = "flash-decks-archived";
    const MAX_FOLDED = 200;

    /**
     * @param {string} name
     * @param {string} value empty clears it
     */
    const setCookie = (name, value) => {
      const age = value === "" ? 0 : 31536000;
      const secure = location.protocol === "https:" ? "; Secure" : "";
      document.cookie = `${name}=${value}; Path=/; SameSite=Lax; Max-Age=${age}${secure}`;
    };

    const foldedIds = () => {
      const kept = new RegExp(`(?:^|; )${FOLDED}=([^;]*)`).exec(document.cookie)?.[1] ?? "";
      return kept.split(".").filter((id) => /^[1-9]\d*$/.test(id));
    };

    /**
     * Shows and hides the rows of one table from their depth and state.
     * @param {HTMLElement} tree
     */
    const lay = (tree) => {
      const showArchived = tree.getAttribute("data-show-archived") !== "0";
      const rows = all(tree, "tr[data-deck]", HTMLTableRowElement);
      const showable = rows.filter((row) => showArchived || !row.hasAttribute("data-archived"));
      for (const row of rows) row.hidden = true;
      /** @type {number | null} */
      let foldedAt = null;
      showable.forEach((row, i) => {
        const depth = Number(row.getAttribute("data-depth"));
        const next = showable[i + 1];
        const hasSubdecks = next !== undefined && Number(next.getAttribute("data-depth")) > depth;
        const shut = row.hasAttribute("data-collapsed");
        const fold = row.querySelector("[data-deck-fold]");
        if (fold !== null) {
          fold.classList.toggle("is-off", !hasSubdecks);
          const button = one(fold, "button", HTMLButtonElement);
          button.setAttribute("aria-expanded", shut ? "false" : "true");
          button.title = shut ? tx("js-ui-show-subdecks") : tx("js-ui-hide-subdecks");
          one(fold, 'input[name="collapsed"]', HTMLInputElement).value = shut ? "0" : "1";
        }
        // A fold ends at the next deck that is not under the folded one.
        if (foldedAt !== null && depth <= foldedAt) foldedAt = null;
        if (foldedAt !== null) return;
        row.hidden = false;
        if (hasSubdecks && shut) foldedAt = depth;
      });
      const none = tree.parentElement?.querySelector("[data-all-archived]");
      if (none instanceof HTMLElement) {
        none.hidden = showable.length > 0;
        tree.hidden = showable.length === 0;
      }
    };

    document.addEventListener("submit", (e) => {
      const form = e.target;
      if (!(form instanceof HTMLFormElement)) return;
      if (form.matches("[data-deck-fold]")) {
        const row = form.closest("tr[data-deck]");
        const tree = form.closest("[data-deck-tree]");
        // Not in a deck table after all: let the form post.
        if (row === null || !(tree instanceof HTMLElement)) return;
        e.preventDefault();
        const id = row.getAttribute("data-deck") ?? "";
        const shut = !row.hasAttribute("data-collapsed");
        row.toggleAttribute("data-collapsed", shut);
        const ids = foldedIds().filter((other) => other !== id);
        if (shut) ids.push(id);
        ids.sort((a, b) => Number(a) - Number(b));
        setCookie(FOLDED, ids.slice(0, MAX_FOLDED).join("."));
        lay(tree);
      } else if (form.matches("[data-show-archived-form]")) {
        const tree = document.querySelector("#decks-main [data-deck-tree]");
        if (!(tree instanceof HTMLElement)) return; // nothing to lay out here: let the form post
        e.preventDefault();
        const show = tree.getAttribute("data-show-archived") === "0";
        tree.setAttribute("data-show-archived", show ? "1" : "0");
        setCookie(ARCHIVED, show ? "1" : "");
        one(form, 'input[name="show"]', HTMLInputElement).value = show ? "0" : "1";
        one(form, "button", HTMLButtonElement).textContent = show ? tx("js-ui-hide-archived") : tx("js-ui-show-archived");
        lay(tree);
      }
    });
  }

  {
    /** @param {Element | null} keep */
    const closeMenus = (keep) => {
      for (const menu of all(document, "details.study-more[open]", HTMLDetailsElement)) {
        if (menu !== keep) menu.open = false;
      }
    };
    document.addEventListener("click", (e) => {
      const picked = closestTo(e.target, ".study-more-item");
      closeMenus(picked === null ? closestTo(e.target, "details.study-more") : null);
    });
    document.addEventListener("keydown", (e) => {
      if (e.key !== "Escape" || composing(e)) return;
      const open = document.querySelector("details.study-more[open]");
      if (open === null) return;
      closeMenus(null);
      e.preventDefault();
      const summary = open.querySelector("summary");
      if (summary instanceof HTMLElement) summary.focus();
    });
  }

  {
    /** @param {Element} opener */
    const openGoal = (opener) => {
      const dialog = document.getElementById("goal-dialog");
      const htmx = window.htmx;
      if (!(dialog instanceof HTMLDialogElement) || htmx === undefined) {
        if (opener instanceof HTMLAnchorElement) window.location.assign(opener.href);
        return;
      }
      const params = new URLSearchParams({
        back: opener.getAttribute("data-goal-back") ?? "",
        from: opener.getAttribute("data-goal-from") ?? "0",
      });
      const path = `/decks/${opener.getAttribute("data-goal-open") ?? ""}/goal/form?${params.toString()}`;
      htmx
        .ajax("GET", path, { target: "#goal-dialog-body", swap: "innerHTML" })
        .then(() => {
          if (!dialog.open) dialog.showModal();
          const field = dialog.querySelector("[data-goal-date]");
          if (field instanceof HTMLInputElement) field.focus();
        })
        .catch((err) => {
          console.error("ui.js: loading the goal form", path, err);
        });
    };
    document.addEventListener("click", (e) => {
      const opener = closestTo(e.target, "[data-goal-open]");
      if (opener !== null) {
        e.preventDefault();
        openGoal(opener);
        return;
      }
      const pick = closestTo(e.target, "[data-goal-pick]");
      const form = pick === null ? null : pick.closest("form");
      if (pick !== null && form !== null) {
        const field = one(form, "[data-goal-date]", HTMLInputElement);
        field.value = pick.getAttribute("data-goal-pick") ?? "";
        for (const chip of all(form, "[data-goal-pick]", HTMLButtonElement)) {
          chip.setAttribute("aria-pressed", chip === pick ? "true" : "false");
        }
        field.dispatchEvent(new Event("goal-pick"));
        return;
      }
      const dialog = document.getElementById("goal-dialog");
      if (!(dialog instanceof HTMLDialogElement)) return;
      // Cancel, or a click on the backdrop (the dialog itself, outside its box).
      if (closestTo(e.target, "[data-goal-close]") !== null || e.target === dialog) dialog.close();
    });
    document.addEventListener("input", (e) => {
      const field = e.target;
      if (!(field instanceof HTMLInputElement) || !field.matches("[data-goal-date]")) return;
      const form = field.closest("form");
      if (form === null) return;
      for (const chip of all(form, "[data-goal-pick]", HTMLButtonElement)) {
        chip.setAttribute("aria-pressed", chip.getAttribute("data-goal-pick") === field.value ? "true" : "false");
      }
    });
  }

  // 新建刷题使用独立弹窗，避免创建参数与刷题记录混在同一信息层级。
  onReady(() => {
    const dialog = document.querySelector("[data-custom-study-dialog]");
    const form = document.querySelector("[data-custom-study-form]");
    if (!(dialog instanceof HTMLDialogElement) || !(form instanceof HTMLFormElement)) return;
    const deck = one(form, "[data-custom-study-deck]", HTMLSelectElement);
    const tags = all(form, "[data-custom-study-tag]", HTMLInputElement);
    const packed = one(form, "[data-custom-study-tags]", HTMLInputElement);
    const count = one(form, "[data-custom-study-count]", HTMLInputElement);
    const hint = one(form, "[data-custom-study-count-hint]", HTMLElement);
    let automatic = count.value;

    const open = () => {
      if (!dialog.open) dialog.showModal();
      const selectedDeck = deck.value !== "";
      const target = selectedDeck ? deck : form.querySelector('input[name="mode"]:checked');
      if (target instanceof HTMLElement) target.focus();
    };
    const close = () => {
      dialog.close();
    };
    dialog.addEventListener("close", () => {
      if (window.location.hash === "#custom-study-form") {
        window.history.replaceState(null, "", `${window.location.pathname}${window.location.search}`);
      }
    });

    document.addEventListener("click", (event) => {
      if (closestTo(event.target, "[data-custom-study-open]") !== null) {
        event.preventDefault();
        open();
        return;
      }
      if (closestTo(event.target, "[data-custom-study-close]") !== null || event.target === dialog) {
        close();
      }
    });
    if (window.location.hash === "#custom-study-form") open();

    // 多选标签写入受边界约束的单个字段；用户手改数量后只更新可选数量提示。
    const refresh = async () => {
      packed.value = tags.filter((tag) => tag.checked).map((tag) => tag.value).join(",");
      const params = new URLSearchParams();
      if (deck.value !== "") params.set("deck", deck.value);
      if (packed.value !== "") params.set("tags", packed.value);
      try {
        const response = await fetch(`/study/custom/count?${params.toString()}`, {
          credentials: "same-origin",
          headers: { Accept: "text/plain" },
        });
        if (!response.ok) return;
        const available = (await response.text()).trim();
        if (count.value === automatic || count.value === "") count.value = available;
        automatic = available;
        hint.textContent = `${hint.getAttribute("data-prefix") ?? ""}: ${available}`;
      } catch {
        // The form remains usable with its last known count while offline.
      }
    };
    deck.addEventListener("change", refresh);
    for (const tag of tags) tag.addEventListener("change", refresh);
  });
})();
