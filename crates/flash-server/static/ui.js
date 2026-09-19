// Small shared UI behaviors (external file: CSP forbids inline script).

// Math rendering (KaTeX, self-hosted): render on load and after every
// htmx swap, scoped to the swapped subtree. \( \) inline, \[ \] display,
// no bare $ (false-positive jank).
(function () {
  function renderMath(root) {
    if (!window.renderMathInElement || !root) return;
    try {
      renderMathInElement(root, {
        delimiters: [
          { left: "\\(", right: "\\)", display: false },
          { left: "\\[", right: "\\]", display: true },
        ],
        throwOnError: false,
      });
    } catch (e) { /* malformed math renders as its source text */ }
  }
  document.addEventListener("DOMContentLoaded", function () {
    renderMath(document.body);
    document.body.addEventListener("htmx:afterSwap", function (e) {
      renderMath(e.target);
    });
  });
})();

// Custom audio player: progressive enhancement over every card <audio>
// element (the import pipeline emits bare `<audio controls>`; without JS
// that native player still works). Dead simple: play/pause + progress +
// duration. One clip at a time.
(function () {
  function fmt(t) {
    if (!isFinite(t)) return "0:00";
    var s = Math.round(t);
    return Math.floor(s / 60) + ":" + String(s % 60).padStart(2, "0");
  }

  var PLAY = '<svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M7 4.5v15l13-7.5z"></path></svg>';
  var PAUSE = '<svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><rect x="6" y="4.5" width="4" height="15"></rect><rect x="14" y="4.5" width="4" height="15"></rect></svg>';

  function enhance(root) {
    if (!root || !root.querySelectorAll) return;
    // Deck rows flatten collapsibles (closed <details> hides content no
    // matter the CSS, so open them; CSS renders them inline, inert).
    root.querySelectorAll(".card-rich-row details").forEach(function (d) { d.open = true; });
    root.querySelectorAll(".card-rich audio, audio.fl-audio-src").forEach(function (a) {
      if (a.dataset.flAudio) return;
      a.dataset.flAudio = "1";
      a.removeAttribute("controls");
      a.classList.add("fl-audio-hidden");

      var wrap = document.createElement("span");
      wrap.className = "fl-audio";
      var btn = document.createElement("button");
      btn.type = "button";
      btn.className = "fl-audio-btn";
      btn.setAttribute("aria-label", "Play audio");
      btn.innerHTML = PLAY;
      var track = document.createElement("span");
      track.className = "fl-audio-track";
      var fill = document.createElement("span");
      fill.className = "fl-audio-fill";
      track.appendChild(fill);
      var time = document.createElement("span");
      time.className = "fl-audio-time";
      time.textContent = "0:00";
      wrap.appendChild(btn);
      wrap.appendChild(track);
      wrap.appendChild(time);
      a.after(wrap);

      function setPlaying(playing) {
        btn.innerHTML = playing ? PAUSE : PLAY;
        btn.setAttribute("aria-label", playing ? "Pause audio" : "Play audio");
      }
      btn.addEventListener("click", function () {
        if (a.paused) {
          // One clip at a time.
          document.querySelectorAll("audio[data-fl-audio]").forEach(function (other) {
            if (other !== a) other.pause();
          });
          a.play().catch(function () {});
        } else {
          a.pause();
        }
      });
      // rAF-driven progress: sparse timeupdate events look laggy on short
      // clips, so track playback every frame while playing.
      var raf = 0;
      function paint() {
        if (a.duration) {
          fill.style.width = (a.currentTime / a.duration * 100) + "%";
          time.textContent = fmt(a.duration - a.currentTime);
        }
        if (!a.paused && !a.ended) raf = requestAnimationFrame(paint);
      }
      a.addEventListener("play", function () {
        setPlaying(true);
        cancelAnimationFrame(raf);
        raf = requestAnimationFrame(paint);
      });
      a.addEventListener("pause", function () {
        setPlaying(false);
        cancelAnimationFrame(raf);
      });
      a.addEventListener("loadedmetadata", function () { time.textContent = fmt(a.duration); });
      a.addEventListener("ended", function () {
        a.currentTime = 0;
        fill.style.width = "0%";
        time.textContent = fmt(a.duration);
        setPlaying(false);
        cancelAnimationFrame(raf);
      });
    });
  }

  // Pages that re-render card DOM on the fly need to re-enhance on demand.
  window.flashAudio = { enhance: enhance };

  document.addEventListener("DOMContentLoaded", function () {
    enhance(document.body);
    document.body.addEventListener("htmx:afterSwap", function (e) { enhance(e.target); });
  });
})();

document.addEventListener("DOMContentLoaded", function () {
  // Dismissing the Connect card is optimistic: gone on click, the POST
  // rides along in the background (keepalive survives navigation).
  document.querySelectorAll(".connect-cta-x").forEach(function (form) {
    form.addEventListener("submit", function (e) {
      e.preventDefault();
      var wrap = form.closest(".connect-cta-wrap");
      if (wrap) wrap.remove();
      fetch(form.action, { method: "POST", keepalive: true, credentials: "same-origin" }).catch(function () {});
    });
  });
});

// Any button with data-copy-target copies that element's text. Delegated:
// the buttons live in blocks htmx re-renders (#mine, #sharing), so a
// per-button listener bound at load would be gone after the first swap.
document.addEventListener("click", async function (e) {
  var btn = e.target.closest("[data-copy-target]");
  if (!btn) return;
  var el = document.getElementById(btn.dataset.copyTarget);
  if (!el) return;
  var old = btn.textContent;
  try {
    await navigator.clipboard.writeText(el.textContent.trim());
    btn.textContent = "Copied";
  } catch (_) {
    // Clipboard blocked or insecure context.
    btn.textContent = "Couldn't copy";
  }
  setTimeout(function () { btn.textContent = old; }, 1500);
});

// Plain (non-htmx) form submits: the import steps take seconds, so the
// submit button goes busy and says so instead of looking ignorable. The
// scroll position is remembered too: a POST → redirect → reload would
// otherwise snap the page to the top, which on a long settings page hides
// the very badge that says what happened.
document.addEventListener("DOMContentLoaded", function () {
  try {
    var saved = sessionStorage.getItem("flash-scroll");
    if (saved) {
      sessionStorage.removeItem("flash-scroll");
      var at = JSON.parse(saved);
      if (at.path === location.pathname && !location.hash) {
        window.scrollTo(0, at.y);
      }
    }
  } catch (_) {}
});
// Delegated: boosted forms are re-rendered by htmx swaps, and a listener
// bound per form at load would be gone after the first one.
document.addEventListener("submit", function (e) {
  // A form marked `data-instant` is answered by this page at once, with no
  // request (the deck lists' fold and "Show archived"): there is nothing
  // to wait for, so it never reads "Working…", which would also replace
  // the fold's drawn chevron with a word.
  var form = e.target.closest("form[method=\"post\"]:not([hx-post]):not([data-instant])");
  if (!form) return;
  var boosted = form.hasAttribute("hx-boost");
  if (!boosted) {
    // Only a real reload loses the scroll position.
    try {
      sessionStorage.setItem(
        "flash-scroll",
        JSON.stringify({ path: location.pathname, y: window.scrollY })
      );
    } catch (_) {}
  }
  var btn = form.querySelector("button[type=submit]");
  if (!btn || btn.dataset.busy) return;
  btn.dataset.busy = "1";
  btn.dataset.label = btn.textContent;
  btn.textContent = btn.dataset.busyLabel || "Working…";
  // Disable after the submit event so the button's value still posts. A
  // boosted form is swapped away on success and restored below on failure,
  // so it never needs disabling (htmx drops duplicate requests itself).
  if (!boosted) setTimeout(function () { btn.disabled = true; }, 0);
});
// A boosted submit that fails leaves its form in place: put the button back.
document.addEventListener("htmx:afterRequest", function (e) {
  if (!e.detail || !e.detail.failed || !e.target.querySelectorAll) return;
  e.target.querySelectorAll("button[data-busy]").forEach(function (btn) {
    btn.textContent = btn.dataset.label;
    delete btn.dataset.busy;
  });
});

// File picker readout + drag-and-drop onto the form card.
(function () {
  function humanSize(n) {
    if (n >= 1048576) return (n / 1048576).toFixed(1) + " MB";
    if (n >= 1024) return Math.round(n / 1024) + " KB";
    return n + " B";
  }
  function init(control) {
    if (control.dataset.flFileInit) return;
    control.dataset.flFileInit = "1";
    var input = control.querySelector("input[type=file]");
    var readout = control.querySelector(".fl-file-name");
    if (!input || !readout) return;
    var idle = readout.textContent;
    function show() {
      var f = input.files && input.files[0];
      if (f) {
        readout.textContent = f.name + " \u00b7 " + humanSize(f.size);
        readout.classList.add("has-file");
      } else {
        readout.textContent = idle;
        readout.classList.remove("has-file");
      }
    }
    input.addEventListener("change", show);
    var zone = control.closest("[data-fl-drop]");
    if (zone) {
      // dragleave fires for every child the cursor crosses; count
      // enters/leaves so the highlight only drops when we truly leave.
      var depth = 0;
      zone.addEventListener("dragenter", function (e) {
        e.preventDefault();
        depth++;
        zone.classList.add("is-drag");
      });
      zone.addEventListener("dragover", function (e) { e.preventDefault(); });
      zone.addEventListener("dragleave", function () {
        depth = Math.max(0, depth - 1);
        if (depth === 0) zone.classList.remove("is-drag");
      });
      zone.addEventListener("drop", function () { depth = 0; zone.classList.remove("is-drag"); });
      zone.addEventListener("drop", function (e) {
        if (!e.dataTransfer || !e.dataTransfer.files.length) return;
        e.preventDefault();
        input.files = e.dataTransfer.files;
        show();
      });
    }
    show();
  }
  document.addEventListener("DOMContentLoaded", function () {
    document.querySelectorAll("[data-fl-file]").forEach(init);
  });
})();

// Status chips (saved, created, ...) are confirmations, not state: they
// fade out a moment after they appear, on load and after htmx swaps.
(function () {
  function toastBadges(root) {
    (root || document).querySelectorAll(".fl-badge-toast:not([data-toasted])").forEach(function (el) {
      el.dataset.toasted = "1";
      setTimeout(function () {
        el.classList.add("is-gone");
        setTimeout(function () { el.remove(); }, 450);
      }, 2200);
    });
  }
  document.addEventListener("DOMContentLoaded", function () {
    toastBadges();
    // A full load that carried a status flag (?saved=… after a no-JS
    // submit, or a link someone kept) must not replay it on refresh.
    if (location.search && document.querySelector("[data-toasted]")) {
      try { history.replaceState(null, "", location.pathname + location.hash); } catch (_) {}
    }
  });
  document.addEventListener("htmx:afterSwap", function () { toastBadges(); });
})();

// The theme picker: a theme applies the moment it is picked. The page
// repaints at once, and the cookie that makes the next load match is set
// by the same post the form makes, sent in the background with nothing
// waiting on it (that cookie is the server's alone to write). The Save
// button is for a browser without script, and is put away here.
(function () {
  document.addEventListener("DOMContentLoaded", function () {
    var form = document.querySelector("[data-theme-form]");
    if (!form) return;
    var save = form.querySelector('button[type="submit"]');
    if (save) save.hidden = true;
    form.querySelector('select[name="theme"]').addEventListener("change", function (e) {
      document.documentElement.setAttribute("data-theme", e.target.value);
      fetch(form.action, {
        method: "POST",
        body: new URLSearchParams(new FormData(form)),
        credentials: "same-origin",
        redirect: "manual",
      }).catch(function () { /* the page already changed; the next pick retries */ });
    });
  });
})();

// A deck's page: the "Subdecks" button in the page head opens and closes
// the panel under it. Closed on every load and never remembered. The head
// can be swapped in place (unarchiving a subdeck brings the page's own deck
// back), which brings a fresh button, so the button is told the panel's
// state again after a swap.
(function () {
  function sync() {
    var panel = document.getElementById("subdecks");
    var button = document.querySelector("[data-subdecks-toggle]");
    if (panel && button) button.setAttribute("aria-expanded", panel.hidden ? "false" : "true");
  }
  document.addEventListener("click", function (e) {
    var button = e.target.closest && e.target.closest("[data-subdecks-toggle]");
    var panel = document.getElementById("subdecks");
    if (!button || !panel) return;
    e.preventDefault(); // without script it is a link to the panel
    panel.hidden = !panel.hidden;
    sync();
    if (!panel.hidden) {
      var field = panel.querySelector('input[name="name"]');
      if (field && !panel.querySelector("tr[data-deck]")) field.focus();
    }
  });
  // It is a link that acts as a button, so Space presses it as Enter does.
  document.addEventListener("keydown", function (e) {
    if (e.key !== " " || !e.target.matches || !e.target.matches("[data-subdecks-toggle]")) return;
    e.preventDefault();
    e.target.click();
  });
  document.addEventListener("htmx:afterSwap", sync);
})();

// Deck lists: showing archived decks and folding a deck shut. Neither
// changes anything on the server, so neither makes a request: every row is
// already in the page, a click shows or hides rows at once, and the choice
// goes in a cookie so the server renders the next load, a refresh
// included, the way it was left (no flash of the other state, which is
// what keeping it in localStorage would cost). Every deck starts open; the
// cookie names only the ones folded shut. The forms still post without
// script. The rules here are `deck_list_rows` in web.rs, and the cookie
// attributes are `pref_cookie_header` in auth.rs.
(function () {
  var FOLDED = "flash-decks-folded";
  var ARCHIVED = "flash-decks-archived";
  var MAX_FOLDED = 200;

  function setCookie(name, value) {
    document.cookie = name + "=" + value + "; Path=/; SameSite=Lax; Max-Age=" +
      (value ? 31536000 : 0) + (location.protocol === "https:" ? "; Secure" : "");
  }

  function foldedIds() {
    var m = document.cookie.match(new RegExp("(?:^|; )" + FOLDED + "=([^;]*)"));
    return (m && m[1] ? m[1].split(".") : []).filter(function (id) { return /^[1-9]\d*$/.test(id); });
  }

  // Shows and hides the rows of one table from their depth and state.
  function lay(tree) {
    var showArchived = tree.getAttribute("data-show-archived") !== "0";
    var rows = Array.prototype.slice.call(tree.querySelectorAll("tr[data-deck]"));
    var showable = rows.filter(function (row) {
      return showArchived || !row.hasAttribute("data-archived");
    });
    rows.forEach(function (row) { row.hidden = true; });
    var foldedAt = null;
    showable.forEach(function (row, i) {
      var depth = Number(row.getAttribute("data-depth"));
      var next = showable[i + 1];
      var hasSubdecks = !!next && Number(next.getAttribute("data-depth")) > depth;
      var shut = row.hasAttribute("data-collapsed");
      var fold = row.querySelector("[data-deck-fold]");
      if (fold) {
        fold.classList.toggle("is-off", !hasSubdecks);
        var button = fold.querySelector("button");
        button.setAttribute("aria-expanded", shut ? "false" : "true");
        button.title = (shut ? "Show" : "Hide") + " subdecks";
        fold.querySelector('input[name="collapsed"]').value = shut ? "0" : "1";
      }
      // A fold ends at the next deck that is not under the folded one.
      if (foldedAt !== null && depth <= foldedAt) foldedAt = null;
      if (foldedAt !== null) return;
      row.hidden = false;
      if (hasSubdecks && shut) foldedAt = depth;
    });
    var none = tree.parentNode.querySelector("[data-all-archived]");
    if (none) {
      none.hidden = showable.length > 0;
      tree.hidden = showable.length === 0;
    }
  }

  document.addEventListener("submit", function (e) {
    var form = e.target;
    if (form.matches && form.matches("[data-deck-fold]")) {
      e.preventDefault();
      var row = form.closest("tr[data-deck]");
      var id = row.getAttribute("data-deck");
      var shut = !row.hasAttribute("data-collapsed");
      row.toggleAttribute("data-collapsed", shut);
      var ids = foldedIds().filter(function (other) { return other !== id; });
      if (shut) ids.push(id);
      ids.sort(function (a, b) { return a - b; });
      setCookie(FOLDED, ids.slice(0, MAX_FOLDED).join("."));
      lay(form.closest("[data-deck-tree]"));
    } else if (form.matches && form.matches("[data-show-archived-form]")) {
      var tree = document.querySelector("#decks-main [data-deck-tree]");
      if (!tree) return; // nothing to lay out here: let the form post
      e.preventDefault();
      var show = tree.getAttribute("data-show-archived") === "0";
      tree.setAttribute("data-show-archived", show ? "1" : "0");
      setCookie(ARCHIVED, show ? "1" : "");
      form.querySelector('input[name="show"]').value = show ? "0" : "1";
      form.querySelector("button").textContent = show ? "Hide archived" : "Show archived";
      lay(tree);
    }
  });
})();
