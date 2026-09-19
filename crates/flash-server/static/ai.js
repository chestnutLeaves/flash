// Card generation: the composer on /generate (instructions on top, the
// material as chips below, each uploaded the moment it is dropped), the
// keep-all confirmation the review asks for, and the review's thread
// (Enter sends, the reply is announced, the drafts change in place).
// External file: the CSP forbids inline script.
//
// The rules the composer applies before it uploads anything (which
// types, how big, how many, what to say when it refuses) come from the
// form's data-gen-caps JSON, which the server writes; nothing here
// invents a limit or a sentence of its own.

(function () {
  "use strict";

  var PREFLIGHT_DEBOUNCE_MS = 250;
  var UNDO_MS = 8000;
  var IN_FLIGHT = 2;
  var BUSY_TRIES = 3;
  // The office extensions the server converts when it has a converter.
  // Without one they are refused here, with the server's own sentence,
  // before any bytes travel.
  var OFFICE_EXT = [".docx", ".doc", ".pptx", ".ppt", ".xlsx", ".xls", ".odt", ".odp", ".ods"];
  // The same links the server accepts: youtube.com (www, m), youtu.be,
  // the watch, shorts, live and embed forms, an eleven-character id.
  var YT = /https?:\/\/(?:www\.|m\.)?(?:youtube\.com\/(?:watch\?(?:[^\s#]*?&)?v=|shorts\/|live\/|embed\/)|youtu\.be\/)([A-Za-z0-9_-]{11})(?![A-Za-z0-9_-])[^\s]*/gi;

  // ---- the review's confirmations ----

  // Keep-all is a plain post (it ends on the deck page, which htmx must
  // not swap in), so its count confirmation is asked for here.
  document.addEventListener("submit", function (e) {
    var form = e.target.closest("form[data-gen-confirm]");
    if (!form) return;
    if (!window.confirm(form.dataset.genConfirm)) e.preventDefault();
  });

  // ---- the deck switchers' "new deck" entry ----

  // Deck 0 is "+ New deck…", and the name beside it belongs to that
  // choice alone: it shows and is required only while 0 is chosen. The
  // listener sits on the document because the review's toolbar is
  // re-rendered out of band, and one bound to the select itself would
  // be gone after the first swap.
  function syncNewDeck(select, focus) {
    var parent = select.parentNode;
    var field = parent ? parent.querySelector("[data-gen-newdeck]") : null;
    if (!field) return;
    var input = field.querySelector("[data-gen-deck-name]");
    var wanted = select.value === "0";
    field.hidden = !wanted;
    if (!input) return;
    input.required = wanted;
    if (wanted && focus) input.focus();
  }

  function syncEveryNewDeck() {
    var selects = document.querySelectorAll("[data-gen-deck]");
    for (var i = 0; i < selects.length; i++) syncNewDeck(selects[i], false);
  }

  document.addEventListener("change", function (e) {
    var select = e.target && e.target.closest && e.target.closest("[data-gen-deck]");
    if (select) syncNewDeck(select, true);
  });

  document.addEventListener("DOMContentLoaded", function () {
    var form = document.getElementById("gen-form");
    if (form) initComposer(form);
    if (document.getElementById("thread")) initThread();
    syncEveryNewDeck();
  });

  // ---- the review's thread ----

  // htmx replaces #thread on every send and every poll, so nothing is
  // bound to the block itself: the listeners sit on the document and
  // the state that matters (how many turns, whether a pass runs) is
  // read back off the block after each swap.
  function initThread() {
    var THREAD_MAX_HEIGHT = 160;
    var seen = snapshot();
    scrollThread();

    function thread() {
      return document.getElementById("thread");
    }

    function snapshot() {
      var t = thread();
      return t ? { turns: Number(t.dataset.turns || 0), active: t.dataset.active === "true" } : null;
    }

    function announce(text) {
      var live = document.getElementById("thread-live");
      if (live) live.textContent = text;
    }

    function scrollThread() {
      var list = document.getElementById("thread-list");
      if (list) list.scrollTop = list.scrollHeight;
    }

    function grow(box) {
      box.style.height = "auto";
      box.style.height = Math.min(box.scrollHeight, THREAD_MAX_HEIGHT) + "px";
    }

    function lastModelText() {
      var bubbles = document.querySelectorAll("#thread-list .gen-msg-model:not(.gen-msg-pending)");
      return bubbles.length ? bubbles[bubbles.length - 1].textContent.trim() : "";
    }

    // Focus goes back to the box after a reply only when nobody was
    // doing anything else: a person editing a draft keeps their place.
    function focusIsIdle() {
      var active = document.activeElement;
      return !active || active === document.body || !!active.closest("#thread");
    }

    document.addEventListener("keydown", function (e) {
      var box = e.target;
      if (!box || box.id !== "thread-message") return;
      if (e.key !== "Enter" || e.shiftKey || e.isComposing) return;
      e.preventDefault();
      if (box.value.trim() && box.form) box.form.requestSubmit();
    });
    document.addEventListener("input", function (e) {
      if (e.target && e.target.id === "thread-message") grow(e.target);
    });

    document.addEventListener("htmx:beforeRequest", function (e) {
      var form = e.detail && e.detail.elt;
      if (form && form.matches && form.matches(".gen-thread-composer")) announce("Sent. Working on the cards.");
    });

    document.addEventListener("htmx:responseError", function (e) {
      var form = e.detail && e.detail.elt;
      if (!form || !form.matches || !form.matches(".gen-thread-composer")) return;
      var slot = form.querySelector("[data-thread-fallback]");
      if (!slot) return;
      slot.textContent = "Couldn't send that. Check the connection and try again.";
      slot.hidden = false;
    });

    // The review body arrives out of band once a pass is done. The deck
    // chosen on the toolbar is carried across, and an editor left open
    // over a draft that may no longer exist is closed with a word.
    var deckChoice = null;
    var deckTyped = null;
    document.addEventListener("htmx:oobBeforeSwap", function (e) {
      var target = e.detail && e.detail.target;
      if (!target || target.id !== "review") return;
      var select = document.getElementById("review-deck");
      deckChoice = select ? select.value : null;
      var name = document.getElementById("review-deck-name");
      deckTyped = name ? name.value : null;
    });
    document.addEventListener("htmx:oobAfterSwap", function (e) {
      var target = e.detail && e.detail.target;
      if (!target || target.id !== "review") return;
      var select = document.getElementById("review-deck");
      if (select && deckChoice !== null) {
        for (var i = 0; i < select.options.length; i++) {
          if (select.options[i].value === deckChoice) { select.value = deckChoice; break; }
        }
      }
      deckChoice = null;
      // The fresh markup brings an empty name field: what was typed
      // goes back in, and the field is opened again if it is the choice.
      var name = document.getElementById("review-deck-name");
      if (name && deckTyped) name.value = deckTyped;
      deckTyped = null;
      syncEveryNewDeck();
      var dialog = document.getElementById("editor-dialog");
      if (dialog && dialog.firstElementChild) {
        dialog.innerHTML = "";
        announce("The cards changed, so the editor was closed.");
      }
    });

    document.addEventListener("htmx:afterSettle", function () {
      var now = snapshot();
      if (!now || !seen) { seen = now; return; }
      var changed = now.turns !== seen.turns || now.active !== seen.active;
      if (now.turns !== seen.turns) scrollThread();
      if (seen.active && !now.active) {
        var said = lastModelText();
        announce(said ? "Done. " + said : "Done.");
        var box = document.getElementById("thread-message");
        if (box && focusIsIdle()) box.focus();
      }
      if (changed) seen = now;
    });
  }

  // ---- the composer ----

  function initComposer(form) {
    var caps;
    try {
      caps = JSON.parse(form.dataset.genCaps || "");
    } catch (_) {
      return;
    }
    var byExt = {};
    var byMime = {};
    caps.kinds.forEach(function (k) {
      k.ext.forEach(function (e) { byExt[e.toLowerCase()] = k; });
      k.mime.forEach(function (m) { byMime[m.toLowerCase()] = k; });
    });

    var ui = {
      composer: form.querySelector("[data-gen-composer]"),
      text: form.querySelector("#gen-instruction"),
      chips: form.querySelector("[data-gen-chips]"),
      attach: form.querySelector("[data-gen-attach]"),
      file: form.querySelector("[data-gen-file]"),
      linkToggle: form.querySelector("[data-gen-link-toggle]"),
      link: form.querySelector("[data-gen-link]"),
      url: form.querySelector("[data-gen-url]"),
      linkAdd: form.querySelector("[data-gen-link-add]"),
      ids: form.querySelector("[data-gen-ids]"),
      summary: form.querySelector("[data-gen-summary]"),
      guard: form.querySelector("[data-gen-guard]"),
      live: form.querySelector("[data-gen-live]"),
      undo: form.querySelector("[data-gen-undo]"),
      undoBtn: form.querySelector("[data-gen-undo-btn]"),
      btn: document.getElementById("gen-btn"),
      btnHint: form.querySelector("[data-gen-btn-hint]"),
      deck: form.querySelector("[data-gen-deck]"),
      deckName: form.querySelector("[data-gen-deck-name]"),
      template: document.getElementById("gen-chip")
    };
    var state = {
      items: [],
      seq: 0,
      inFlight: 0,
      preflightSeq: 0,
      preflightTimer: 0,
      pf: null,
      undo: null,
      undoTimer: 0
    };

    // ---- what a file is, and whether it may go ----

    function extOf(name) {
      var at = name.lastIndexOf(".");
      return at > 0 ? name.slice(at).toLowerCase() : "";
    }

    function kindOf(file) {
      var ext = extOf(file.name);
      if (ext && byExt[ext]) return byExt[ext];
      var mime = (file.type || "").toLowerCase();
      if (mime && byMime[mime]) return byMime[mime];
      return null;
    }

    function liveCount(kind) {
      var n = 0;
      state.items.forEach(function (it) {
        if (it.state === "refused") return;
        if (!kind || it.kind === kind) n++;
      });
      return n;
    }

    // The refusals this page can make on its own, in the order a
    // person would want to hear them; null means upload it.
    function validate(file, isDir, batchSoFar) {
      if (isDir) return { kind: "folder", reason: caps.copy.folder };
      var kc = kindOf(file);
      if (!kc) {
        var office = OFFICE_EXT.indexOf(extOf(file.name)) >= 0;
        return { kind: office ? "document" : "file", reason: office ? caps.copy.document : caps.copy.unsupported };
      }
      if (kc.withheld) return { kind: kc.kind, reason: kc.withheld.reason, withheld: true };
      if (!file.size) return { kind: kc.kind, reason: caps.copy.empty };
      if (file.size > caps.file_max_bytes) return { kind: kc.kind, reason: caps.copy.too_large };
      if (file.name.length > caps.filename_max) return { kind: kc.kind, reason: caps.copy.name_too_long };
      if (liveCount(null) + batchSoFar.all >= caps.set.max_sources) {
        return { kind: kc.kind, reason: caps.copy.too_many_sources };
      }
      if (kc.kind === "image" && liveCount("image") + batchSoFar.images >= caps.set.max_images) {
        return { kind: kc.kind, reason: caps.copy.too_many_images };
      }
      return { kind: kc.kind, reason: null };
    }

    function fileKey(file) {
      return file.name + " " + file.size + " " + (file.lastModified || 0);
    }

    function findByKey(key) {
      for (var i = 0; i < state.items.length; i++) {
        if (state.items[i].key === key) return state.items[i];
      }
      return null;
    }

    // ---- items and their chips ----

    function makeItem(fields) {
      var item = {
        seq: ++state.seq,
        key: null,
        kind: "file",
        name: "",
        size: 0,
        file: null,
        url: null,
        text: null,
        state: "pending",
        pct: 0,
        id: null,
        line: "",
        reason: null,
        xhr: null,
        thumb: null,
        el: null,
        tries: 0,
        removed: false
      };
      Object.keys(fields).forEach(function (k) { item[k] = fields[k]; });
      state.items.push(item);
      item.el = chip(item);
      ui.chips.appendChild(item.el);
      render(item);
      return item;
    }

    function chip(item) {
      var el = ui.template.content.firstElementChild.cloneNode(true);
      el.dataset.seq = String(item.seq);
      el.querySelector(".gen-chip-sub").id = "gen-sub-" + item.seq;
      el.setAttribute("aria-describedby", "gen-sub-" + item.seq);
      var x = el.querySelector(".gen-chip-x");
      x.addEventListener("click", function () { remove(item); });
      el.addEventListener("keydown", function (e) {
        if ((e.key === "Delete" || e.key === "Backspace") && e.target === el) {
          e.preventDefault();
          remove(item);
        }
      });
      if (item.file && item.kind === "image") {
        var img = el.querySelector(".gen-chip-thumb");
        try {
          item.thumb = URL.createObjectURL(item.file);
          img.src = item.thumb;
          img.hidden = false;
          img.addEventListener("error", function () { img.hidden = true; });
        } catch (_) {}
      }
      return el;
    }

    function subtitle(item) {
      if (item.state === "refused") return item.reason || "";
      if (item.state === "uploading") return "Uploading " + item.pct + "%";
      if (item.state === "reading") return "Reading…";
      if (item.state === "pending") return item.tries ? caps.copy.busy : "Waiting…";
      return item.line || "";
    }

    function render(item) {
      var el = item.el;
      el.dataset.state = item.state;
      el.dataset.kind = item.kind;
      el.dataset.genPct = String(item.pct);
      el.querySelector(".gen-chip-name").textContent = item.name;
      el.querySelector(".gen-chip-name").title = item.name;
      el.querySelector(".gen-chip-sub").textContent = subtitle(item);
      el.querySelector(".gen-chip-x").setAttribute("aria-label", "Remove " + item.name);
      var icon = el.querySelector(".gen-chip-icon use");
      var known = ["pdf", "document", "text", "image", "video", "audio", "youtube", "folder"];
      icon.setAttribute("href", "#gen-i-" + (known.indexOf(item.kind) >= 0 ? item.kind : "file"));
      var ring = el.querySelector(".gen-chip-ring");
      var busy = item.state === "uploading" || item.state === "reading";
      ring.hidden = !busy;
      ring.style.setProperty("--p", String(item.state === "reading" ? 100 : item.pct));
      if (busy) {
        ring.setAttribute("role", "progressbar");
        ring.setAttribute("aria-valuemin", "0");
        ring.setAttribute("aria-valuemax", "100");
        ring.setAttribute("aria-valuenow", String(item.pct));
      } else {
        ring.removeAttribute("role");
      }
    }

    function announce(text) {
      if (ui.live) ui.live.textContent = text;
    }

    function say(text) {
      if (!ui.guard) return;
      ui.guard.textContent = text || "";
      ui.guard.hidden = !text;
    }

    function refuse(item, reason) {
      item.state = "refused";
      item.reason = reason;
      render(item);
      announce(item.name + ": " + reason);
      settle();
    }

    // ---- taking things in ----

    function addFiles(list) {
      var batch = { all: 0, images: 0 };
      var entries = [];
      if (list && typeof list.length === "number" && list[0] && typeof list[0].getAsFile === "function") {
        for (var i = 0; i < list.length; i++) {
          var it = list[i];
          if (it.kind !== "file") continue;
          var entry = typeof it.webkitGetAsEntry === "function" ? it.webkitGetAsEntry() : null;
          var f = it.getAsFile();
          if (!f) continue;
          entries.push({ file: f, isDir: !!(entry && entry.isDirectory) });
        }
      } else {
        for (var j = 0; j < list.length; j++) entries.push({ file: list[j], isDir: false });
      }
      entries.forEach(function (e) {
        var key = fileKey(e.file);
        var seen = findByKey(key);
        if (seen && seen.state !== "refused") {
          announce("Already attached: " + e.file.name);
          return;
        }
        if (seen) remove(seen, true);
        var verdict = validate(e.file, e.isDir, batch);
        var item = makeItem({
          key: key,
          kind: verdict.kind,
          name: e.file.name,
          size: e.file.size,
          file: e.file
        });
        if (verdict.reason) {
          refuse(item, verdict.reason);
          return;
        }
        batch.all++;
        if (item.kind === "image") batch.images++;
        pump();
      });
      say("");
      settle();
    }

    function addLink(url) {
      var m = YT.exec(url.trim());
      YT.lastIndex = 0;
      if (!m) {
        say("That doesn't look like a YouTube link.");
        return false;
      }
      var key = "yt " + m[1];
      var seen = findByKey(key);
      if (seen && seen.state !== "refused") {
        announce("Already attached: " + url.trim());
        return true;
      }
      if (seen) remove(seen, true);
      var kc = byKind("youtube");
      var item = makeItem({ key: key, kind: "youtube", name: url.trim(), url: url.trim() });
      if (!kc) {
        refuse(item, caps.copy.unsupported);
      } else if (kc.withheld) {
        refuse(item, kc.withheld.reason);
      } else if (liveCount(null) > caps.set.max_sources) {
        refuse(item, caps.copy.too_many_sources);
      } else {
        pump();
      }
      say("");
      settle();
      return true;
    }

    function addText(text, range) {
      var kc = byKind("text");
      var item = makeItem({ key: null, kind: "text", name: "Pasted text", text: text, size: text.length });
      if (!kc) {
        refuse(item, caps.copy.unsupported);
      } else if (kc.withheld) {
        refuse(item, kc.withheld.reason);
      } else if (text.length > caps.text_max_bytes) {
        refuse(item, "That text is longer than the limit.");
      } else if (liveCount(null) > caps.set.max_sources) {
        refuse(item, caps.copy.too_many_sources);
      } else {
        pump();
      }
      armUndo(item, text, range);
      settle();
    }

    function byKind(name) {
      for (var i = 0; i < caps.kinds.length; i++) {
        if (caps.kinds[i].kind === name) return caps.kinds[i];
      }
      return null;
    }

    // ---- the upload queue ----

    function pump() {
      for (var i = 0; i < state.items.length && state.inFlight < IN_FLIGHT; i++) {
        var item = state.items[i];
        if (item.state !== "pending" || item.retryAt > Date.now()) continue;
        send(item);
      }
    }

    function send(item) {
      state.inFlight++;
      if (item.file) return upload(item);
      item.state = "uploading";
      item.pct = 100;
      render(item);
      var body = new URLSearchParams();
      if (item.url) body.set("youtube_url", item.url);
      else body.set("text", item.text);
      fetch("/ai/sources", {
        method: "POST",
        credentials: "same-origin",
        headers: { "Content-Type": "application/x-www-form-urlencoded", "Accept": "application/json" },
        body: body.toString()
      }).then(function (r) {
        return r.text().then(function (t) { onReply(item, r.status, t); });
      }, function () {
        onReply(item, 0, "");
      });
    }

    function upload(item) {
      var xhr = new XMLHttpRequest();
      item.xhr = xhr;
      item.state = "uploading";
      item.pct = 0;
      render(item);
      xhr.open("POST", "/ai/sources");
      xhr.setRequestHeader("Accept", "application/json");
      xhr.responseType = "text";
      xhr.upload.onprogress = function (e) {
        if (!e.lengthComputable || item.removed) return;
        item.pct = Math.min(100, Math.round((e.loaded / e.total) * 100));
        if (item.pct === 100 && item.state === "uploading") item.state = "reading";
        render(item);
      };
      xhr.onload = function () { onReply(item, xhr.status, xhr.responseText); };
      xhr.onerror = function () { onReply(item, 0, ""); };
      xhr.onabort = function () { state.inFlight--; pump(); };
      var fd = new FormData();
      fd.append("file", item.file, item.file.name);
      xhr.send(fd);
    }

    function onReply(item, status, text) {
      state.inFlight--;
      item.xhr = null;
      var body = null;
      try { body = text ? JSON.parse(text) : null; } catch (_) {}
      if (item.removed) {
        // The X came while the bytes were in flight: the row exists
        // now, so take it back.
        if (body && body.id) discard(body.id);
        pump();
        return;
      }
      if (status === 201 && body && body.id) {
        item.id = body.id;
        item.line = body.line || "";
        if (body.url) item.name = body.url;
        var p = body.preflight || {};
        if (p.allowed === false) {
          refuse(item, p.reason || "This can't be read.");
        } else {
          item.state = "ready";
          render(item);
          announce(item.name + " ready. " + item.line);
          settle();
        }
      } else if (status === 429) {
        item.tries++;
        if (item.tries >= BUSY_TRIES) {
          refuse(item, caps.copy.busy);
        } else {
          item.state = "pending";
          item.retryAt = Date.now() + 1000 * Math.pow(2, item.tries - 1);
          render(item);
          setTimeout(pump, 1000 * Math.pow(2, item.tries - 1) + 20);
        }
      } else if (status === 413) {
        refuse(item, caps.copy.too_large);
      } else if (status === 401) {
        say("You were signed out. Reload and sign in again.");
        refuse(item, "Not signed in.");
      } else if (status === 0) {
        refuse(item, "Upload failed. Check the connection and try again.");
      } else {
        var message = body && body.error && body.error.message;
        refuse(item, message || "That couldn't be read.");
      }
      pump();
    }

    function discard(id) {
      try {
        fetch("/ai/sources/" + id, { method: "DELETE", credentials: "same-origin", keepalive: true });
      } catch (_) {}
    }

    // ---- taking things out ----

    function remove(item, quiet) {
      if (item.removed) return;
      item.removed = true;
      if (item.xhr) {
        try { item.xhr.abort(); } catch (_) {}
        item.xhr = null;
      } else if (item.state === "uploading" || item.state === "reading") {
        // A fetch in flight cannot be aborted here; onReply discards.
      }
      if (item.id) discard(item.id);
      if (item.thumb) {
        try { URL.revokeObjectURL(item.thumb); } catch (_) {}
      }
      var next = item.el.nextElementSibling || item.el.previousElementSibling;
      item.el.remove();
      state.items = state.items.filter(function (it) { return it !== item; });
      if (state.undo && state.undo.item === item) clearUndo();
      if (!quiet) {
        announce("Removed " + item.name);
        if (next) next.focus(); else ui.attach.focus();
      }
      settle();
      pump();
    }

    // ---- the set's reading, and the button ----

    function readyIds() {
      return state.items.filter(function (it) { return it.state === "ready" && it.id; })
        .map(function (it) { return it.id; });
    }

    function settle() {
      syncIds();
      syncGenerate();
      clearTimeout(state.preflightTimer);
      state.preflightTimer = setTimeout(preflight, PREFLIGHT_DEBOUNCE_MS);
    }

    function syncIds() {
      ui.ids.value = readyIds().join(",");
    }

    function preflight() {
      var ids = readyIds();
      if (!ids.length) {
        state.pf = null;
        summary("", true);
        syncGenerate();
        return;
      }
      var seq = ++state.preflightSeq;
      fetch("/ai/preflight", {
        method: "POST",
        credentials: "same-origin",
        headers: { "Content-Type": "application/json", "Accept": "application/json" },
        body: JSON.stringify({ source_ids: ids })
      }).then(function (r) {
        return r.text().then(function (t) {
          var j = null;
          try { j = JSON.parse(t); } catch (_) {}
          return { ok: r.ok, j: j };
        });
      }, function () {
        return { ok: false, j: null };
      }).then(function (res) {
        if (seq !== state.preflightSeq) return;
        if (res.ok && res.j) {
          state.pf = res.j;
        } else {
          state.pf = {
            allowed: false,
            reason: (res.j && res.j.error && res.j.error.message) || "Couldn't check the set.",
            line: ""
          };
        }
        summary(state.pf.line || "", state.pf.allowed !== false);
        syncGenerate();
      });
    }

    function summary(line, allowed) {
      if (!ui.summary) return;
      ui.summary.textContent = line;
      ui.summary.dataset.allowed = allowed ? "true" : "false";
    }

    // "+ New deck…" with nothing typed has nowhere to put the cards.
    function deckUnnamed() {
      return !!(ui.deck && ui.deck.value === "0" && (!ui.deckName || !ui.deckName.value.trim()));
    }

    function syncGenerate() {
      if (!ui.btn) return;
      var ready = 0, pending = 0, refused = 0;
      state.items.forEach(function (it) {
        if (it.state === "ready") ready++;
        else if (it.state === "refused") refused++;
        else pending++;
      });
      var busy = !!form.dataset.genBusy;
      var unnamed = deckUnnamed();
      var allowed = ready > 0 && !pending && !refused && state.pf && state.pf.allowed !== false;
      ui.btn.disabled = !(allowed && !busy && !unnamed);
      if (!ui.btnHint) return;
      if (busy) ui.btnHint.textContent = "Wait for the running generation to finish.";
      else if (!state.items.length) ui.btnHint.textContent = "Attach something to read first.";
      else if (pending) ui.btnHint.textContent = "Uploading…";
      else if (refused) ui.btnHint.textContent = "Remove what can't be read, then generate.";
      else if (state.pf && state.pf.allowed === false) ui.btnHint.textContent = state.pf.reason || "";
      else if (unnamed) ui.btnHint.textContent = "Name the new deck.";
      else if (allowed) ui.btnHint.textContent = "You'll see every card before any of it joins the deck.";
      else ui.btnHint.textContent = "Checking…";
    }

    // ---- the drop zone ----

    function hasFiles(e) {
      var t = e.dataTransfer;
      return !!(t && t.types && Array.prototype.indexOf.call(t.types, "Files") >= 0);
    }

    var depth = 0;
    ui.composer.addEventListener("dragenter", function (e) {
      if (!hasFiles(e)) return;
      e.preventDefault();
      depth++;
      ui.composer.classList.add("is-drag");
    });
    ui.composer.addEventListener("dragover", function (e) {
      if (!hasFiles(e)) return;
      e.preventDefault();
      e.dataTransfer.dropEffect = "copy";
    });
    ui.composer.addEventListener("dragleave", function (e) {
      if (!hasFiles(e)) return;
      depth = Math.max(0, depth - 1);
      if (!depth) ui.composer.classList.remove("is-drag");
    });
    ui.composer.addEventListener("drop", function (e) {
      depth = 0;
      ui.composer.classList.remove("is-drag");
      if (!hasFiles(e)) return;
      e.preventDefault();
      addFiles(e.dataTransfer.items && e.dataTransfer.items.length ? e.dataTransfer.items : e.dataTransfer.files);
    });
    // A drop that misses the composer must not open the file in the tab.
    document.addEventListener("dragover", function (e) { if (hasFiles(e)) e.preventDefault(); });
    document.addEventListener("drop", function (e) { if (hasFiles(e)) e.preventDefault(); });

    // ---- the deck ----

    // Choosing "+ New deck…" or typing its name moves the button
    // between disabled and ready; the reveal itself is the document's.
    if (ui.deck) ui.deck.addEventListener("change", syncGenerate);
    if (ui.deckName) ui.deckName.addEventListener("input", syncGenerate);

    // ---- the picker and the link row ----

    ui.attach.addEventListener("click", function () { ui.file.click(); });
    ui.file.addEventListener("change", function () {
      if (ui.file.files && ui.file.files.length) addFiles(ui.file.files);
      ui.file.value = "";
    });
    if (ui.linkToggle && ui.link) {
      ui.linkToggle.addEventListener("click", function () {
        var open = ui.link.hidden;
        ui.link.hidden = !open;
        ui.linkToggle.setAttribute("aria-expanded", open ? "true" : "false");
        if (open) ui.url.focus();
      });
      function takeLink() {
        var value = ui.url.value.trim();
        if (!value) return;
        if (addLink(value)) {
          ui.url.value = "";
          ui.link.hidden = true;
          ui.linkToggle.setAttribute("aria-expanded", "false");
          ui.linkToggle.focus();
        }
      }
      ui.linkAdd.addEventListener("click", takeLink);
      ui.url.addEventListener("keydown", function (e) {
        if (e.key === "Enter") {
          e.preventDefault();
          takeLink();
        } else if (e.key === "Escape") {
          ui.link.hidden = true;
          ui.linkToggle.setAttribute("aria-expanded", "false");
          ui.linkToggle.focus();
        }
      });
    }

    // ---- the instructions box: paste, links, the lift ----

    function liftLinks(text, needSpaceAfter) {
      // Returns the text with every finished YouTube link cut out, and
      // adds a chip per link; null when there was none to lift.
      var found = false;
      var out = text.replace(YT, function (match, id, offset, whole) {
        var end = offset + match.length;
        var after = whole.charAt(end);
        if (needSpaceAfter && end < whole.length && !/\s/.test(after)) return match;
        if (needSpaceAfter && end === whole.length) return match;
        found = true;
        addLink(match);
        return "";
      });
      YT.lastIndex = 0;
      return found ? out.replace(/[ \t]{2,}/g, " ") : null;
    }

    ui.text.addEventListener("paste", function (e) {
      var cd = e.clipboardData;
      if (!cd) return;
      if (cd.files && cd.files.length) {
        e.preventDefault();
        addFiles(cd.files);
        return;
      }
      var text = cd.getData("text/plain");
      if (!text) return;
      var trimmed = text.trim();
      YT.lastIndex = 0;
      var lone = YT.exec(trimmed);
      YT.lastIndex = 0;
      if (lone && lone.index === 0 && lone[0].length === trimmed.length) {
        e.preventDefault();
        addLink(trimmed);
        return;
      }
      var lifted = liftLinks(text, false);
      if (lifted !== null) {
        e.preventDefault();
        ui.text.setRangeText(lifted, ui.text.selectionStart, ui.text.selectionEnd, "end");
        return;
      }
      var lines = text.split(/\r?\n/).length;
      if (text.length > caps.lift.chars || lines >= caps.lift.lines) {
        e.preventDefault();
        var range = { start: ui.text.selectionStart, end: ui.text.selectionEnd };
        addText(trimmed, range);
      }
    });

    function liftTyped(atEnd) {
      var lifted = liftLinks(ui.text.value, !atEnd);
      if (lifted === null) return;
      var pos = ui.text.selectionStart;
      ui.text.value = lifted;
      try { ui.text.setSelectionRange(Math.min(pos, lifted.length), Math.min(pos, lifted.length)); } catch (_) {}
    }
    ui.text.addEventListener("input", function () { liftTyped(false); });
    ui.text.addEventListener("blur", function () { liftTyped(true); });

    function armUndo(item, text, range) {
      clearUndo();
      state.undo = { item: item, text: text, range: range };
      ui.undo.hidden = false;
      state.undoTimer = setTimeout(clearUndo, UNDO_MS);
    }
    function clearUndo() {
      clearTimeout(state.undoTimer);
      state.undo = null;
      if (ui.undo) ui.undo.hidden = true;
    }
    if (ui.undoBtn) {
      ui.undoBtn.addEventListener("click", function () {
        var u = state.undo;
        if (!u) return;
        clearUndo();
        remove(u.item, true);
        var start = Math.min(u.range.start, ui.text.value.length);
        var end = Math.min(u.range.end, ui.text.value.length);
        ui.text.setRangeText(u.text, start, end, "end");
        ui.text.focus();
        announce("Kept as instructions.");
      });
    }

    // ---- the form, and the page's life ----

    form.addEventListener("submit", function (e) {
      if (ui.btn && ui.btn.disabled) {
        e.preventDefault();
        return;
      }
      clearUndo();
      syncIds();
    });
    window.addEventListener("pageshow", function (e) {
      // Back from the job page: the chips belong to a queued job now,
      // so the page starts over rather than posting them twice.
      if (e.persisted) window.location.reload();
    });
    window.addEventListener("pagehide", function () {
      state.items.forEach(function (it) {
        if (it.xhr) { try { it.xhr.abort(); } catch (_) {} }
      });
    });

    syncGenerate();
  }
})();
