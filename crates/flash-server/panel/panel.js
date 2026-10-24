(() => {
  "use strict";

  const { isObject, isStrings, str, count, cardOf, studyOf, shownList, cardsOf, answerOf } = window.flashPayload;

  /**
   * Whether `node` is a `Ctor` made in another window. ChatGPT's frame
   * swaps the quick edit's editable fields for elements of its own
   * window after this script has run (checked 2026-10-12): the same kind
   * of element, which `instanceof` here does not recognize. Every DOM
   * prototype names its kind in its own `Symbol.toStringTag`.
   * @param {object} node
   * @param {{ name: string }} Ctor
   */
  function madeElsewhereAs(node, Ctor) {
    for (let p = Reflect.getPrototypeOf(node); p !== null; p = Reflect.getPrototypeOf(p)) {
      if (Object.getOwnPropertyDescriptor(p, Symbol.toStringTag)?.value === Ctor.name) return true;
    }
    return false;
  }

  /**
   * The element with id `id`, as a `Ctor`: the panel's own markup, so a
   * missing one is a bug, and says which.
   * @template {Element} T
   * @param {string} id
   * @param {new () => T} Ctor
   * @returns {T}
   */
  function byId(id, Ctor) {
    const found = document.getElementById(id);
    if (found instanceof Ctor) return found;
    if (found !== null && madeElsewhereAs(found, Ctor)) return /** @type {T} */ (/** @type {unknown} */ (found));
    throw new Error(`panel: no ${Ctor.name} #${id}`);
  }
  /** @param {string} id */
  const $ = (id) => byId(id, HTMLElement);
  /** @param {string} id */
  const button = (id) => byId(id, HTMLButtonElement);

  /**
   * Starts work nobody waits on; a failure is logged, never unhandled.
   * @param {Promise<unknown>} work
   */
  function run(work) {
    work.catch((/** @type {unknown} */ e) => {
      console.error("[flash-panel]", e);
    });
  }

  // ---- The host: MCP Apps over postMessage, or the older window.openai
  // bridge when a host only offers that.

  let nextId = 1;
  /** @type {Map<number, { resolve: (result: unknown) => void, reject: (error: Error) => void }>} */
  const pending = new Map();
  /** @type {"mcp" | "openai" | null} */
  let bridge = null;

  /**
   * Every message out and in goes to the console, shortened: a host that
   * refuses or drops a call leaves no other trace a developer can read.
   * @param {string} dir
   * @param {unknown} message
   */
  function trace(dir, message) {
    let text;
    try {
      text = JSON.stringify(message);
    } catch {
      text = typeof message;
    }
    console.info(`[flash-panel] ${dir} ${text.length > 400 ? `${text.slice(0, 400)}...` : text}`);
  }
  /** @param {Record<string, unknown>} message */
  function post(message) {
    const full = { jsonrpc: "2.0", ...message };
    trace("->", full);
    window.parent.postMessage(full, "*");
  }
  /**
   * @param {string} method
   * @param {Record<string, unknown>} params
   * @param {number} [timeoutMs]
   * @returns {Promise<unknown>}
   */
  function request(method, params, timeoutMs = 30000) {
    const id = nextId++;
    post({ id, method, params });
    return new Promise((resolve, reject) => {
      pending.set(id, { resolve, reject });
      setTimeout(() => {
        if (pending.delete(id)) reject(new Error("timeout"));
      }, timeoutMs);
    });
  }
  /**
   * @param {string} method
   * @param {Record<string, unknown>} [params]
   */
  function notify(method, params = {}) {
    post({ method, params });
  }

  window.addEventListener("message", (event) => {
    /** @type {unknown} */
    const m = event.data;
    if (event.source !== window.parent) {
      trace("<- (not from the host frame, ignored)", m);
      return;
    }
    trace("<-", m);
    if (!isObject(m) || m["jsonrpc"] !== "2.0") return;
    const id = m["id"];
    const method = m["method"];
    if (id != null && method == null) {
      if (typeof id !== "number") return;
      const waiting = pending.get(id);
      if (waiting === undefined) return;
      pending.delete(id);
      const error = m["error"];
      if (error != null) {
        const message = isObject(error) ? str(error["message"]) : null;
        waiting.reject(new Error(message ?? "error"));
      } else {
        waiting.resolve(m["result"]);
      }
      return;
    }
    switch (method) {
      case "ui/notifications/tool-result":
        showResult(m["params"]);
        break;
      case "ui/notifications/host-context-changed":
        applyContext(m["params"]);
        break;
      default:
        // Requests the page has nothing to add to (teardown, ping) are
        // answered empty so the host never waits on them.
        if (id != null) post({ id, result: {} });
    }
  });

  /** @returns {OpenAiBridge} */
  function openai() {
    const o = window.openai;
    if (o === undefined) throw new Error("panel: the window.openai bridge is gone");
    return o;
  }

  const host = {
    /**
     * @param {string} name
     * @param {Record<string, unknown>} args
     * @returns {Promise<unknown>}
     */
    async callTool(name, args) {
      if (bridge === "openai") return openai().callTool(name, args);
      const result = await request("tools/call", { name, arguments: args });
      if (isObject(result) && result["isError"] === true) {
        const content = result["content"];
        const first = Array.isArray(content) ? /** @type {unknown} */ (content[0]) : undefined;
        throw new Error((isObject(first) ? str(first["text"]) : null) ?? "error");
      }
      return result;
    },
    /** @param {string} text */
    say(text) {
      if (bridge === "openai") return openai().sendFollowUpMessage({ prompt: text });
      return request("ui/message", { role: "user", content: [{ type: "text", text }] });
    },
    /** @param {string} text */
    async tellModel(text) {
      if (bridge === "openai") {
        await openai().setWidgetState?.({ progress: text });
        return;
      }
      await request("ui/update-model-context", { content: [{ type: "text", text }] });
    },
    /**
     * A page of the site, in the user's browser; the host may ask first.
     * @param {string} url
     */
    async open(url) {
      const external = bridge === "openai" ? openai().openExternal : undefined;
      if (external !== undefined) {
        await external({ href: url });
        return;
      }
      await request("ui/open-link", { url });
    },
    /** Full screen, where the host has it; the host then says so in its context. */
    async expand() {
      if (bridge === "openai") {
        await openai().requestDisplayMode?.({ mode: "fullscreen" });
        return;
      }
      const result = await request("ui/request-display-mode", { mode: "fullscreen" });
      // The host answers with the mode it set, which may not be the one asked for.
      const mode = isObject(result) ? result["mode"] : null;
      if (mode === "fullscreen" || mode === "inline") setDisplay({ mode });
    },
  };

  /**
   * A tool result's structured content, or the same parsed from its text.
   * @param {unknown} result
   * @returns {unknown}
   */
  function structured(result) {
    if (!isObject(result)) return null;
    if (result["structuredContent"] != null) return result["structuredContent"];
    const content = result["content"];
    const first = Array.isArray(content) ? /** @type {unknown} */ (content[0]) : undefined;
    const text = isObject(first) ? str(first["text"]) : null;
    if (text === null) return null;
    try {
      return /** @type {unknown} */ (JSON.parse(text));
    } catch {
      return null;
    }
  }

  // ---- Look: the host's theme and font, and how it shows the panel.

  /**
   * @type {string | null}
   */
  let hostLocale = null;
  let hostLocaleTold = false;

  /**
   * @type {string | null}
   */
  let accountLang = null;

  /**
   * @param {string} id
   * @param {Record<string, string | number>} [vars]
   * @returns {string}
   */
  const tx = (id, vars) => (window.flashT === undefined ? id : window.flashT(id, vars));

  const headers = Array.from(document.querySelectorAll("script[data-flash-lang]"), (island) => {
    try {
      const data = /** @type {unknown} */ (JSON.parse(island.textContent));
      if (!isObject(data)) return null;
      const lang = str(data["lang"]);
      if (lang === null) return null;
      const script = str(data["script"]);
      return { lang, dir: data["dir"] === "rtl" ? "rtl" : "ltr", script: script ?? "Latn" };
    } catch {
      return null;
    }
  }).filter((header) => header !== null);

  const carried = headers.map((header) => header.lang);

  const rightToLeft = () => document.documentElement.dir === "rtl";

  const rootLang = carried[0] ?? document.documentElement.lang;

  /**
   * @type {unknown}
   */
  const languageTable = (() => {
    const island = document.querySelector("script[data-flash-languages]");
    if (island === null) return null;
    try {
      return /** @type {unknown} */ (JSON.parse(island.textContent));
    } catch {
      return null;
    }
  })();

  const browserZone = (() => {
    try {
      return Intl.DateTimeFormat().resolvedOptions().timeZone;
    } catch {
      return null;
    }
  })();

  /**
   * @param {string} tag
   * @returns {{ locale: string, formatTag: string } | null}
   */
  function catalogFor(tag) {
    return window.flashNegotiate?.([tag], browserZone, languageTable, carried) ?? null;
  }

  /**
   * @type {string | undefined}
   */
  let dateTag;

  function setLang() {
    const named = hostLocale ?? window.openai?.locale ?? navigator.language;
    const hostTag = typeof named === "string" ? named : null;
    const fromHost = hostTag === null ? null : catalogFor(hostTag);
    const lang = (accountLang === null ? null : catalogFor(accountLang)?.locale) ?? fromHost?.locale ?? rootLang;
    // The negotiated form of the host's tag: Intl refuses one like "es_MX".
    dateTag = lang === rootLang ? undefined : fromHost?.locale === lang ? fromHost.formatTag : lang;
    if (document.documentElement.lang === lang && document.documentElement.dataset["worded"] === "1") return;
    document.documentElement.lang = lang;
    const header = headers.find((h) => h.lang === lang);
    document.documentElement.dir = header?.dir ?? "ltr";
    document.documentElement.dataset["script"] = header?.script ?? "Latn";
    document.documentElement.dataset["worded"] = "1";
    for (const node of document.querySelectorAll("[data-t]")) {
      if (node instanceof HTMLElement) node.textContent = tx(node.dataset["t"] ?? "");
    }
    for (const node of document.querySelectorAll("[data-t-title]")) {
      if (node instanceof HTMLElement) node.title = tx(node.dataset["tTitle"] ?? "");
    }
    for (const node of document.querySelectorAll("[data-t-label]")) {
      if (node instanceof HTMLElement) node.setAttribute("aria-label", tx(node.dataset["tLabel"] ?? ""));
    }
    for (const node of document.querySelectorAll("[data-rating]")) {
      if (node instanceof HTMLElement) node.title = tx("panel-key", { key: node.dataset["rating"] ?? "" });
    }
  }

  /** @param {unknown} ctx */
  function applyContext(ctx) {
    if (!isObject(ctx)) return;
    const locale = str(ctx["locale"]);
    if (locale !== null && locale !== "") hostLocale = locale;
    setLang();
    const theme = ctx["theme"];
    if (theme === "light" || theme === "dark") document.documentElement.dataset["theme"] = theme;
    const styles = ctx["styles"];
    const vars = isObject(styles) ? styles["variables"] : undefined;
    const font = isObject(vars) ? str(vars["--font-sans"]) : null;
    if (font !== null && font !== "") document.documentElement.style.setProperty("--font-sans", font);
    const dims = ctx["containerDimensions"];
    const height = isObject(dims) ? (num(dims["height"]) ?? num(dims["maxHeight"])) : null;
    const modes = ctx["availableDisplayModes"];
    setDisplay({
      mode: ctx["displayMode"],
      modes: isStrings(modes) ? modes : undefined,
      height,
      insets: ctx["safeAreaInsets"],
    });
  }

  /** @param {unknown} v */
  const num = (v) => (typeof v === "number" && Number.isFinite(v) && v > 0 ? v : null);

  /**
   * How the host shows the panel: inline in the conversation, or full
   * screen (ChatGPT's side panel, Claude's own view). Full screen is
   * offered only where the host lists it; a host that never says shows
   * the panel inline.
   * @typedef {{ top: number, right: number, bottom: number, left: number }} Insets
   * @type {{ mode: "inline" | "fullscreen", modes: string[], height: number | null, insets: Insets }}
   */
  const display = { mode: "inline", modes: ["inline"], height: null, insets: { top: 0, right: 0, bottom: 0, left: 0 } };

  /**
   * Takes what the host said and lays the page out for it; anything the
   * host left out stays as it was.
   * @param {{ mode?: unknown, modes?: string[] | undefined, height?: number | null | undefined, insets?: unknown }} next
   */
  function setDisplay(next) {
    if (next.mode === "inline" || next.mode === "fullscreen") display.mode = next.mode;
    if (next.modes !== undefined) display.modes = next.modes;
    if (next.height !== undefined && next.height !== null) display.height = next.height;
    const insets = next.insets;
    if (isObject(insets)) {
      display.insets = {
        top: num(insets["top"]) ?? 0,
        right: num(insets["right"]) ?? 0,
        bottom: num(insets["bottom"]) ?? 0,
        left: num(insets["left"]) ?? 0,
      };
    }
    const root = document.documentElement;
    const full = display.mode === "fullscreen";
    root.classList.toggle("fullscreen", full);
    root.classList.toggle("overlaid", full && overlaid());
    for (const side of /** @type {const} */ (["top", "right", "bottom", "left"])) {
      root.style.setProperty(`--safe-${side}`, `${display.insets[side]}px`);
    }
    if (full && display.height !== null) root.style.setProperty("--full-h", `${display.height}px`);
    else root.style.removeProperty("--full-h");
    fitView();
  }

  /**
   * Whether the host's message box likely lies over the frame's bottom: a
   * full screen that fills most of the browser window (ChatGPT's full
   * window, 61% of it measured; its side panel 38% to 54%), never a phone.
   * No host says so (ChatGPT reports a zero inset), so the page reads it
   * from its own width against the window's.
   */
  function overlaid() {
    const w = window.innerWidth;
    const outer = window.outerWidth > 0 ? window.outerWidth : w;
    return w > 480 && w / outer > 0.6;
  }

  /** Fits the card of whichever view is on screen. */
  function fitView() {
    if (view === "study") fitFace();
    else if (view === "cards") fitCards(true);
  }

  /**
   * After a card's text is fitted. Expand is offered on every card where
   * the host has full screen. A card that still does not fit stops there,
   * in the fade, with Show all over it; where the host has no full screen,
   * its box grows to hold it instead. Full screen has the room.
   * @param {HTMLElement} face
   * @param {boolean} fits
   */
  function offerRoom(face, fits) {
    const full = display.mode === "fullscreen";
    const canExpand = display.modes.includes("fullscreen") && !full;
    button("expand").hidden = !canExpand;
    const pill = face.parentElement?.querySelector(".show-all");
    if (pill instanceof HTMLElement) pill.hidden = fits || !canExpand;
    face.classList.toggle("clip", !fits && canExpand);
    face.classList.toggle("grow", !fits && !canExpand && !full);
    alignSides(face);
    markMore(face);
  }

  /** The block that holds a session card's front and answer. */
  function frontBlock() {
    const fit = $("face").querySelector(".fit");
    if (!(fit instanceof HTMLElement)) throw new Error("panel: no .fit in #face");
    return fit;
  }

  /**
   * Show answer leaves the front where it was: the answer comes in under
   * it, and the front moves up only as far as the answer needs room.
   * (Centred again, the block would jump.) A new card centres again.
   * @param {HTMLElement} fit
   * @param {number} from where the block's top was, in the card
   */
  function holdFront(fit, from) {
    const face = $("face");
    const style = getComputedStyle(face);
    const top = Number.parseFloat(style.paddingTop);
    const bottom = Number.parseFloat(style.paddingBottom);
    fit.style.margin = "0";
    const room = face.clientHeight - top - bottom - fit.offsetHeight;
    fit.style.marginTop = `${String(Math.max(0, Math.min(from - top, room)))}px`;
  }

  // A side whose text is a line or two (with or without a picture) sits in
  // the middle of the box, as a flashcard; longer text reads from the left,
  // where a paragraph is easy to follow. Each side on its own: a one-word
  // front can sit over a paragraph of an answer. The text is measured with
  // the pictures out, so a picture is never counted as lines.
  /** @param {HTMLElement} face */
  function alignSides(face) {
    for (const side of face.querySelectorAll(".side")) {
      if (!(side instanceof HTMLElement)) continue;
      const line = Number.parseFloat(getComputedStyle(side).lineHeight);
      side.classList.add("bare");
      const lines = side.offsetHeight / (Number.isFinite(line) && line > 0 ? line : 24);
      side.classList.remove("bare");
      side.classList.toggle("center", lines <= 2.2);
    }
  }

  // The host sizes the frame from this, as the MCP Apps library measures
  // it: the page's own height with `html` at max-content (a frame's html
  // otherwise fills whatever height the host gave it), and the window's
  // width. Until the first report the frame may have no height at all.
  let lastSize = "";
  let sizing = false;
  function reportSize() {
    if (bridge === null || sizing) return;
    sizing = true;
    requestAnimationFrame(() => {
      sizing = false;
      const root = document.documentElement;
      const was = root.style.height;
      root.style.height = "max-content";
      const height = Math.ceil(root.getBoundingClientRect().height);
      root.style.height = was;
      const width = Math.ceil(window.innerWidth);
      const key = `${width}x${height}`;
      if (key === lastSize) return;
      lastSize = key;
      if (bridge === "mcp") notify("ui/notifications/size-changed", { width, height });
      // ChatGPT documents only its own call for this; it offers it beside
      // the standard bridge too, so both are sent where both exist.
      window.openai?.notifyIntrinsicHeight?.(height);
    });
  }
  const resized = new ResizeObserver(reportSize);
  resized.observe(document.documentElement);
  resized.observe(document.body);

  // ---- Which view shows. A result names its own in `view`; a result
  // from before there were views is a study session.

  /** @type {"study" | "cards" | null} */
  let view = null;
  let busy = false;
  /**
   * @type {{ card: number, answer: Answer | null, since: number, url?: string | null } | null}
   */
  let editing = null;
  // Once the host refuses the quick edit's tools (one holding an earlier
  // tool list), the pencil opens the site's editor instead.
  let quickOff = false;
  const editOffered = () => !(quickOff && panel?.site_url == null);
  let archiveOff = false;
  /** @type {number | null} */
  let archived = null;
  /** @type {(() => Promise<unknown> | void) | null} */
  let lastAction = null;

  /**
   * The server's own view of a result, in _meta, or, when a host passes
   * only what the model sees, the same built from that (media may then
   * not load, the text still does). Null when neither is one this page can
   * show: anything else (a message from a host that is not speaking this
   * protocol, a shape from some other build) is ignored rather than drawn,
   * so it never replaces the view on screen or throws.
   * @param {unknown} result
   * @returns {StudyView | CardsView | null}
   */
  function payloadFrom(result) {
    const v = viewFrom(result);
    return cardsOf(v) ?? studyOf(v);
  }

  /** @param {unknown} result @returns {unknown} */
  function viewFrom(result) {
    if (!isObject(result)) return null;
    const meta = result["_meta"] ?? result["meta"];
    if (isObject(meta) && meta["flash/panel"] != null) return meta["flash/panel"];
    const out = structured(result);
    if (!isObject(out)) return null;
    if (out["session_id"] != null) {
      const first = cardOf(out["first_card"]);
      return {
        view: "study",
        session_id: out["session_id"],
        state: first === null ? "caught_up" : "card",
        card: first,
        remaining: count(out["cards_due"]),
        held_by_limit: 0,
        preload: [],
        recorded: true,
      };
    }
    // create_cards and update_cards: the ids, and the cards fetched as
    // they come up.
    const ids = out["card_ids"];
    if (Array.isArray(ids) && ids.length > 0) {
      return { view: "cards", mode: out["created"] != null ? "created" : "edited", card_ids: ids, cards: [] };
    }
    return null;
  }

  /** @param {unknown} result */
  function showResult(result) {
    const next = payloadFrom(result);
    if (next === null) return;
    const v = viewFrom(result);
    const lang = isObject(v) ? str(v["ui_lang"]) : null;
    if (lang !== null && lang !== "") accountLang = lang;
    setLang();
    if (next.view === "cards") showCards(next);
    else showSession(next);
  }

  /** @param {"study" | "cards"} name */
  function showView(name) {
    view = name;
    $("loading").hidden = true;
    $("error").hidden = true;
    $("card").hidden = true;
    $("quick").hidden = true;
    $("done").hidden = true;
    $("track").hidden = true;
    button("expand").hidden = true;
    button("edit").hidden = true;
    button("archive").hidden = true;
    closeMore();
    editing = null;
    $("left").replaceChildren();
    $("cards").hidden = name !== "cards";
  }

  /**
   * @param {number} n
   * @param {string} one
   * @param {string} many
   */
  function plural(n, one, many) {
    return `${n} ${n === 1 ? one : many}`;
  }

  /**
   * @template {keyof HTMLElementTagNameMap} K
   * @param {K} tag
   * @param {string} [className]
   * @param {string} [text]
   */
  function el(tag, className, text) {
    const node = document.createElement(tag);
    if (className !== undefined && className !== "") node.className = className;
    if (text !== undefined) node.textContent = text;
    return node;
  }


  /**
   * The header: the deck as the site names it, its own name in bold, and
   * each tag as a chip. Names and tags are the user's own text, so they go
   * in as text, never markup.
   * @param {string | null} label
   * @param {string[]} tags
   */
  function renderScope(label, tags) {
    const deck = $("deck");
    const parts = (label ?? (tags.length > 0 ? "" : "Flash")).split(" › ");
    const own = parts.pop() ?? "";
    deck.replaceChildren();
    if (parts.length > 0) deck.append(`${parts.join(" › ")} › `);
    if (own !== "") deck.append(el("b", undefined, own));
    deck.hidden = own === "";
    for (const old of $("scope").querySelectorAll(".chip, .tags-label")) old.remove();
    $("scope").append(...tagChips(tags, true));
  }

  /**
   * @param {string[]} tags
   * @param {boolean} labelled
   * @returns {HTMLElement[]}
   */
  function tagChips(tags, labelled) {
    if (tags.length === 0) return [];
    const chips = tags.map((tag) => el("span", "chip", tag));
    return labelled ? [el("span", "tags-label", tx("panel-tags")), ...chips] : chips;
  }

  // Marks for a track, one per card, or a single bar past what fits.
  const MAX_MARKS = 40;
  /**
   * @param {HTMLElement} track
   * @param {string[]} kinds a class per mark ("" for a plain one)
   * @param {number} filled the bar's share, 0 to 1
   */
  function drawTrack(track, kinds, filled) {
    if (kinds.length > MAX_MARKS) {
      const fill = el("b");
      fill.style.width = `${100 * filled}%`;
      const rail = el("span");
      rail.append(fill);
      track.className = "track bar";
      track.replaceChildren(rail);
      return;
    }
    track.className = "track";
    track.replaceChildren(...kinds.map((kind) => el("span", kind)));
  }

  /**
   * When a card comes back, in the fewest words a button can carry.
   * Short, as the study page's buttons say it (web.rs comes_back_label):
   * 10m, 3h, 2d, 4mo, 1.5y.
   * @param {number} ms
   */
  function comesBack(ms) {
    const minutes = Math.round(ms / 60000);
    if (minutes < 60) return tx("interval-minutes", { count: Math.max(1, minutes) });
    const hours = Math.round(ms / 3600000);
    if (hours < 24) return tx("interval-hours", { count: hours });
    const days = Math.round(ms / 86400000);
    if (days < 31) return tx("interval-days", { count: days });
    const months = Math.round(days / 30.4);
    if (months < 12) return tx("interval-months", { count: months });
    return tx("interval-years", { count: Math.round(days / 36.5) / 10 });
  }

  /**
   * A card longer than the cap scrolls inside itself; it fades at the
   * bottom while there is more below. A new card starts at its top.
   * @param {HTMLElement} face
   */
  function markMore(face) {
    face.classList.toggle("more", face.scrollHeight - face.scrollTop - face.clientHeight > 4);
  }
  /** @param {HTMLElement} face */
  function freshFace(face) {
    face.scrollTop = 0;
    markMore(face);
  }
  for (const id of ["face", "shown"]) {
    const face = $(id);
    face.addEventListener(
      "scroll",
      () => {
        markMore(face);
      },
      { passive: true },
    );
    face.addEventListener(
      "load",
      () => {
        if (id === "face") fitFace();
        else fitShown();
      },
      true,
    );
  }

  /** @type {Set<string>} */
  const warmed = new Set();
  /** @param {string[]} urls */
  function warm(urls) {
    for (const url of urls) {
      if (warmed.has(url)) continue;
      warmed.add(url);
      const img = new Image();
      img.src = url;
    }
  }

  /** @param {boolean} on */
  function setBusy(on) {
    busy = on;
    for (const b of document.querySelectorAll("button")) b.disabled = on;
    if (!on && view === "cards") renderPager();
  }

  /**
   * @param {() => Promise<unknown> | void} action what Try again does
   * @param {unknown} err
   */
  function fail(action, err) {
    lastAction = action;
    const late = err instanceof Error && err.message === "timeout";
    $("error-text").textContent = tx(late ? "panel-no-answer-late" : "panel-no-answer");
    $("error").hidden = false;
    setBusy(false);
  }

  // ---- The study session.

  /** @type {StudyView | null} */
  let panel = null;
  /** @type {Answer | null} the back of the card on screen, once shown */
  let revealed = null;
  // Read back after an await: another tap, or the server's answer to an
  // earlier one, may have moved the session on meanwhile.
  const nowShowing = () => ({ panel, revealed });

  /** @typedef {"again" | "hard" | "good" | "easy"} Kind */
  /**
   * What this panel graded, by kind; and since when.
   * @typedef {{ graded: number, again: number, hard: number, good: number, easy: number, since: number }} Tally
   */
  /** @returns {Tally} */
  function freshTally() {
    return { graded: 0, again: 0, hard: 0, good: 0, easy: 0, since: Date.now() };
  }
  let tally = freshTally();

  /** @type {readonly Kind[]} */
  const KINDS = ["again", "hard", "good", "easy"];
  /** @param {unknown} rating @returns {1 | 2 | 3 | 4 | null} */
  function ratingOf(rating) {
    return rating === 1 || rating === 2 || rating === 3 || rating === 4 ? rating : null;
  }
  /** @param {1 | 2 | 3 | 4} rating */
  function kindOf(rating) {
    return KINDS[rating - 1] ?? "good";
  }

  // Every call goes through the host and back, which takes a good part of
  // a second, so the panel asks ahead: a card's back as soon as the card
  // (or the one before it) is on screen, so Show answer has it already.
  // Card id -> the promise of its answer; a failed one is dropped and
  // asked again on the tap.
  /** @type {Map<number, Promise<Answer>>} */
  const backs = new Map();
  /** @param {number} id */
  function askBack(id) {
    let asked = backs.get(id);
    if (asked === undefined) {
      asked = host.callTool("panel_show_answer", { card_id: id }).then((result) => {
        const answer = answerOf(structured(result));
        if (answer === null) throw new Error("panel: an answer of the wrong shape");
        return answer;
      });
      asked.catch(() => backs.delete(id));
      backs.set(id, asked);
    }
    return asked;
  }

  /**
   * @param {Kind} kind
   * @param {1 | -1} by
   */
  function tallyUp(kind, by) {
    tally.graded += by;
    tally[kind] += by;
  }

  /** @param {StudyView} next */
  function showSession(next) {
    const kept = saved(next.session_id);
    if (kept !== null && (kept.panel.reviewed ?? 0) >= (next.reviewed ?? 0)) {
      adopt(kept);
      return;
    }
    if (panel?.session_id !== next.session_id) tally = freshTally();
    panel = next;
    revealed = null;
    render(next);
  }

  // ---- One session, two copies of the page. ChatGPT shows full screen
  // in a second frame beside the inline one, and each starts from the same
  // tool result, which by then may show a card already graded. So the
  // latest state of a session is kept in this origin's storage, and each
  // copy follows the other's. The server stays the judge: its answer to
  // the next grade settles any difference. Storage may be off (a private
  // window, blocked site data); the panel then works as one copy.

  const STORE = "flash-panel:session:";
  /** @typedef {{ panel: StudyView, revealed: Answer | null, tally: Tally }} Saved */
  let lastSaved = "";
  function saveSession() {
    if (panel === null || view !== "study") return;
    const text = JSON.stringify({ panel, revealed, tally });
    if (text === lastSaved) return;
    lastSaved = text;
    try {
      localStorage.setItem(STORE + String(panel.session_id), text);
    } catch {
      // Storage is off; this copy carries on alone.
    }
  }

  /**
   * @param {string | null} text
   * @returns {Saved | null}
   */
  function savedOf(text) {
    if (text === null) return null;
    /** @type {unknown} */
    let v;
    try {
      v = JSON.parse(text);
    } catch {
      return null;
    }
    if (!isObject(v)) return null;
    const p = studyOf(v["panel"]);
    if (p === null) return null;
    const t = v["tally"];
    const fresh = freshTally();
    const kept = isObject(t)
      ? {
          graded: count(t["graded"]),
          again: count(t["again"]),
          hard: count(t["hard"]),
          good: count(t["good"]),
          easy: count(t["easy"]),
          since: num(t["since"]) ?? fresh.since,
        }
      : fresh;
    return { panel: p, revealed: answerOf(v["revealed"]), tally: kept };
  }

  /** @param {number} id */
  function saved(id) {
    try {
      return savedOf(localStorage.getItem(STORE + String(id)));
    } catch {
      return null;
    }
  }

  /** @param {Saved} kept */
  function adopt(kept) {
    tally = kept.tally;
    panel = kept.panel;
    revealed = null;
    // The other copy may have edited a card: its backs are asked again.
    backs.clear();
    render(panel);
    if (kept.revealed !== null && panel.card?.card_id === kept.revealed.card_id) showAnswer(kept.revealed);
  }

  window.addEventListener("storage", (e) => {
    if (panel === null || view !== "study" || busy || editing !== null || e.key !== STORE + String(panel.session_id)) return;
    const kept = savedOf(e.newValue);
    if (kept !== null) adopt(kept);
  });

  /**
   * A grade's answer names no scope; the session's stays as it started.
   * @param {StudyView} next
   * @param {StudyView} was
   * @returns {StudyView}
   */
  function keepScope(next, was) {
    return { ...next, deck_label: was.deck_label, deck: was.deck, tags: was.tags };
  }

  /**
   * How far through the session: a mark per review done, the card on
   * screen, then the queue. The marks say nothing of the grades: a run
   * of red would read as failing at what is only forgetting, which is
   * what the cards are for. An Again card comes back into the queue, so
   * the total can grow; that is the work there is. A result from before
   * the server counted reviews falls back on this panel's own count.
   * @param {StudyView} p
   */
  function renderProgress(p) {
    const done = p.reviewed ?? tally.graded;
    const track = $("track");
    track.hidden = p.state !== "card";
    if (track.hidden) return;
    const total = done + p.remaining;
    /** @type {string[]} */
    const kinds = Array.from({ length: done }, () => "done");
    kinds.push("now");
    while (kinds.length < total) kinds.push("");
    drawTrack(track, kinds, total > 0 ? done / total : 0);
  }

  /**
   * The header's right side while cards remain: how many are left, and
   * no score; the grades are counted up when the session is done.
   * @param {StudyView} p
   */
  function renderTally(p) {
    const left = $("left");
    left.replaceChildren();
    if (archived !== null) {
      const note = el("span", "archived-note", tx("panel-archived"));
      const undo = el("button", "link", tx("panel-undo"));
      undo.id = "undo-archive";
      undo.type = "button";
      const card = archived;
      undo.addEventListener("click", () => {
        run(unarchive(card));
      });
      note.append(undo);
      left.append(note);
      return;
    }
    if (p.state !== "card") return;
    left.append(el("span", "tally", tx("panel-to-go", { count: p.remaining })));
  }

  // The session's card is one size whatever it holds. A card too long
  // for it takes the largest text that fits, no smaller than MIN_FIT of
  // the usual. One too long even then offers Expand, where the host has
  // full screen (an inline card cannot be scrolled everywhere: on Claude's
  // phone app a swipe in it scrolls the chat); where it has none, that
  // card takes the height it needs. Full screen has the room: the usual
  // size, and the card scrolls if it must.
  const MIN_FIT = 0.75;
  function fitFace() {
    const face = $("face");
    face.classList.remove("grow");
    face.style.removeProperty("--fit-front");
    if (display.mode === "fullscreen") {
      face.style.setProperty("--fit", "1");
      offerRoom(face, true);
      return;
    }
    // With the answer showing, the front keeps the size it had alone, so
    // a reveal never shrinks what was just read: it is fitted without the
    // answer, held there, and only the answer takes what room is left.
    const answer = $("answer");
    if (answer.hidden === false) {
      answer.hidden = true;
      fitText(face);
      face.style.setProperty("--fit-front", face.style.getPropertyValue("--fit"));
      answer.hidden = false;
    }
    offerRoom(face, fitText(face));
  }

  /**
   * Sets the largest text size, down to MIN_FIT, at which the card fits.
   * @param {HTMLElement} face
   * @returns {boolean} whether it fits at all
   */
  function fitText(face) {
    const at = (/** @type {number} */ fit) => {
      face.style.setProperty("--fit", String(fit));
      return face.scrollHeight <= face.clientHeight + 1;
    };
    // Measuring lays the card out, which starts any font it lacks; one
    // still loading measures differently once it is in.
    const refit = () => {
      markMore(face);
      if (document.fonts.status !== "loading") return;
      document.fonts.ready
        .then(() => {
          fitView();
        })
        .catch((/** @type {unknown} */ err) => {
          console.warn("panel: fonts", err);
        });
    };
    if (face.clientHeight === 0 || at(1)) {
      refit();
      return true;
    }
    let fits = MIN_FIT;
    let over = 1;
    for (let i = 0; i < 6; i++) {
      const mid = (fits + over) / 2;
      if (at(mid)) fits = mid;
      else over = mid;
    }
    const fitted = at(fits);
    refit();
    return fitted;
  }

  // Which card's front is on screen and since when, so a grade says how
  // long the card took to answer.
  /** @type {{ id: number | null, at: number }} */
  let frontSince = { id: null, at: 0 };

  /** @param {StudyView} p */
  function render(p) {
    showView("study");
    renderScope(p.deck_label, p.tags);
    const card = p.state === "card" ? p.card : null;
    $("card").hidden = card === null;
    $("done").hidden = card !== null;
    renderProgress(p);
    renderTally(p);
    if (card !== null) {
      if (frontSince.id !== card.card_id) frontSince = { id: card.card_id, at: Date.now() };
      $("front").innerHTML = card.front_html;
      $("front").lang = card.front_lang;
      frontBlock().style.margin = "";
      $("face").classList.add("tap");
      $("answer").hidden = true;
      $("back").innerHTML = "";
      $("reveal").hidden = false;
      $("grades").hidden = true;
      warm(p.preload);
      fitFace();
      freshFace($("face"));
      askAhead(p);
      // The quick edit needs no site link: Le Chat passes the panel only
      // what the model reads, which has none (checked 2026-10-12). Once a
      // host has refused the quick edit, the pencil is the site's editor,
      // and needs it.
      button("edit").hidden = !editOffered();
      button("archive").hidden = archiveOff;
    } else {
      renderDone(p);
    }
    openInApp(sitePage("/study"));
    setBusy(false);
    saveSession();
  }

  /**
   * The backs of the card on screen and of the one expected after it.
   * @param {StudyView} p
   */
  function askAhead(p) {
    for (const card of [p.card, p.next_card]) {
      if (card !== null) {
        askBack(card.card_id).catch(() => {
          // Asked again on the tap.
        });
      }
    }
  }

  /**
   * @param {number} graded
   */
  function reviewedWords(graded) {
    const minutes = Math.round((Date.now() - tally.since) / 60000);
    const cards = tx("panel-cards-reviewed", { count: graded });
    return minutes < 1
      ? tx("panel-reviewed-quick", { reviewed: cards })
      : tx("panel-reviewed-in", { reviewed: cards, count: minutes });
  }

  /** @param {StudyView} p */
  function renderDone(p) {
    const graded = tally.graded;
    const summary = graded > 0 ? reviewedWords(graded) : "";
    const moreButton = $("more");
    const make = $("make");
    moreButton.hidden = p.state !== "held" || (p.goal?.catch_up ?? 0) > 0;
    make.hidden = p.state !== "empty";
    if (p.state === "held") {
      $("done-title").textContent = tx("panel-held-title");
      const held = tx("panel-held-text", { count: p.held_by_limit });
      $("done-text").textContent = summary === "" ? held : tx("panel-held-text-summary", { held, summary });
      // The true number: the boost lets ten more through, so with fewer
      // waiting, all of them come.
      const n = Math.min(BOOST, p.held_by_limit);
      moreButton.textContent = tx("panel-study-more", { count: n > 0 ? n : BOOST });
    } else if (p.state === "empty") {
      $("done-title").textContent = tx("panel-empty-title");
      $("done-text").textContent = tx("panel-empty-text");
    } else {
      $("done-title").textContent = graded > 0 ? tx("panel-done-title") : tx("panel-caught-up");
      $("done-text").textContent = summary === "" ? tx("panel-nothing-due") : summary;
    }
    $("acts").hidden = moreButton.hidden && make.hidden;
    // The site keeps the record of every review; with no cards yet, there
    // is nothing there to see.
    $("progress-row").hidden = p.state === "empty" || p.site_url === null;
    renderScore();
    renderGoal(p.goal);
    renderWeek(p.week, p.next_due);
    renderNext(p);
  }

  /**
   * @param {PanelGoal | null} goal
   */
  function renderGoal(goal) {
    const box = $("goal-box");
    box.hidden = goal === null;
    if (goal === null) return;
    box.classList.toggle("behind", goal.behind);
    const arc = byId("goal-arc", SVGCircleElement);
    const ring = 2 * Math.PI * arc.r.baseVal.value;
    const dash = (ring * goal.percent) / 100;
    arc.setAttribute("stroke-dasharray", `${dash.toFixed(2)} ${ring.toFixed(2)}`);
    $("goal-head").textContent = goal.head;
    $("goal-caption").textContent = goal.caption;
    const goalMore = button("goal-more");
    goalMore.hidden = goal.catch_up === 0;
    goalMore.textContent = goal.study_more;
    $("goal-pct").hidden = goal.catch_up > 0;
    $("goal-pct").textContent = goal.percent_label;
  }

  /** @type {Record<Kind, string>} */
  const SCORE_WORDS = {
    again: "panel-score-again",
    hard: "panel-score-hard",
    good: "panel-score-good",
    easy: "panel-score-easy",
  };

  // The grades given here, most common first in the usual order.
  function renderScore() {
    const score = $("score");
    /** @type {Kind[]} */
    const order = ["good", "easy", "hard", "again"];
    const pills = order.filter((kind) => tally[kind] > 0).map((kind) => el("span", `pill ${kind}`, tx(SCORE_WORDS[kind], { count: tally[kind] })));
    score.replaceChildren(...pills);
    score.hidden = pills.length === 0;
  }

  /**
   * The seven study days after today, a bar for each by how many cards
   * come due, the day the next ones come in full colour.
   * @param {DayAhead[]} week
   * @param {NextDue | null} next
   */
  function renderWeek(week, next) {
    const box = $("week");
    const most = Math.max(0, ...week.map((day) => day.cards));
    box.hidden = most === 0;
    if (most === 0) {
      box.replaceChildren();
      return;
    }
    const first = next !== null && next.in_days >= 1 ? next.in_days - 1 : -1;
    box.replaceChildren(
      ...week.map((day, i) => {
        const col = el("div", `col${i === first ? " next" : ""}`);
        const bar = el("i");
        bar.style.height = `${Math.max(3, Math.round((64 * day.cards) / most))}px`;
        const name = new Date(day.at_ms).toLocaleDateString(dateTag, { weekday: "short" });
        col.append(el("b", undefined, String(day.cards)), bar, el("span", undefined, name));
        return col;
      }),
    );
  }

  /**
   * When the next cards come: tomorrow, a weekday within the week, or a
   * number of days. "Later today" only for an account on exact times.
   * @param {NextDue} next
   */
  function nextWords(next) {
    const at = new Date(next.at_ms);
    const cards = next.cards;
    if (next.in_days === 0) {
      const time = at.toLocaleTimeString(dateTag, { hour: "numeric", minute: "2-digit" });
      const hour = Number.parseInt(time, 10);
      return tx("panel-next-today", { count: cards, time, hour: Number.isNaN(hour) ? -1 : hour });
    }
    if (next.in_days === 1) return tx("panel-next-tomorrow", { count: cards });
    if (next.in_days < 7) {
      return tx("panel-next-weekday", { count: cards, day: at.toLocaleDateString(dateTag, { weekday: "long" }) });
    }
    return tx("panel-next-days", { count: cards, days: next.in_days });
  }

  // Reminders are asked for once, here, and not taken back here (the
  // site's settings do that): `on` when the server turned them on, `text`
  // what it said either way.
  /** @type {{ on: boolean, text: string | null }} */
  const remind = { on: false, text: null };

  /** @param {StudyView} p */
  function renderNext(p) {
    const next = p.next_due;
    const offered = p.reminders_offer && remind.text === null;
    $("next-box").hidden = next === null && !offered && remind.text === null;
    $("next-due").textContent = next === null ? tx("panel-nothing-scheduled") : nextWords(next);
    $("remind").hidden = !offered;
    $("reminded").hidden = !remind.on;
    $("remind-text").textContent = remind.text ?? "";
    $("remind-text").hidden = remind.text === null;
  }

  async function turnOnReminders() {
    if (busy || remind.text !== null || panel === null) return;
    const p = panel;
    setBusy(true);
    let zone = "";
    try {
      zone = Intl.DateTimeFormat().resolvedOptions().timeZone;
    } catch {
      zone = "";
    }
    const fallback = tx("panel-remind-fallback");
    try {
      const out = structured(await host.callTool("panel_reminders", zone === "" ? {} : { timezone: zone }));
      remind.on = isObject(out) && out["on"] === true;
      remind.text = remind.on && isObject(out) ? (str(out["message"]) ?? fallback) : fallback;
    } catch {
      remind.text = fallback;
    }
    renderNext(p);
    setBusy(false);
  }

  // The back asked for ahead (`askBack`), so this is usually immediate.
  async function reveal() {
    trace("tap", { action: "show answer", busy, card: panel?.card?.card_id });
    if (busy || view !== "study" || panel?.card == null || revealed !== null) return;
    const id = panel.card.card_id;
    if (archived !== null) {
      archived = null;
      renderTally(panel);
    }
    setBusy(true);
    try {
      const answer = await askBack(id);
      if (nowShowing().panel?.card?.card_id !== id) {
        setBusy(false);
        return;
      }
      showAnswer(answer);
      setBusy(false);
      document.querySelector('#grades [data-rating="3"]')?.closest("button")?.focus();
    } catch (err) {
      fail(reveal, err);
    }
  }

  /** @param {Answer} answer */
  function showAnswer(answer) {
    revealed = answer;
    // A Try again that went through: the failure it answered is over.
    $("error").hidden = true;
    const fit = frontBlock();
    const from = fit.offsetTop;
    $("back").innerHTML = answer.back_html;
    $("back").lang = answer.back_lang;
    $("answer").hidden = false;
    $("reveal").hidden = true;
    $("face").classList.remove("tap");
    const times = answer.comes_back_in_ms;
    for (const b of document.querySelectorAll("#grades button")) {
      if (!(b instanceof HTMLButtonElement)) continue;
      const ms = times?.[Number(b.dataset["rating"]) - 1];
      const when = b.querySelector(".when");
      if (when !== null) when.textContent = ms === undefined ? "" : comesBack(ms);
    }
    $("grades").hidden = false;
    fitFace();
    holdFront(fit, from);
    // A front that fills the card leaves the answer below the fold.
    const face = $("face");
    const top = $("answer").offsetTop;
    if (top > face.scrollTop + face.clientHeight - 60) face.scrollTop = top - 24;
    markMore(face);
    saveSession();
  }

  // A grade shows the card expected next at once and is recorded behind
  // it; the server's answer then says which card really comes (it can
  // differ when the chat graded one too) and the panel follows it. One
  // grade is recorded at a time: a quicker one waits for the last.
  /** @type {Promise<StudyView> | null} */
  let recording = null;

  /** @param {1 | 2 | 3 | 4} rating */
  async function grade(rating) {
    trace("tap", { action: "grade", rating, busy, revealed: revealed !== null });
    if (busy || view !== "study" || panel?.card == null || revealed === null) return;
    const card = panel.card.card_id;
    archived = null;
    if (recording !== null) {
      setBusy(true);
      try {
        await recording;
      } catch {
        return; // its failure is on screen, with Try again
      }
      const now = nowShowing();
      if (now.panel?.card?.card_id !== card || now.revealed === null) {
        setBusy(false);
        return;
      }
    }
    const before = { panel, revealed };
    /** @type {Record<string, unknown>} */
    const args = { session_id: panel.session_id, card_id: card, rating };
    if (frontSince.id === card) args["answer_ms"] = Date.now() - frontSince.at;
    const locale = hostLocale ?? window.openai?.locale ?? null;
    if (!hostLocaleTold && typeof locale === "string" && locale !== "") {
      args["ui_locale"] = locale.slice(0, 35);
      hostLocaleTold = true;
    }
    const kind = kindOf(rating);
    tallyUp(kind, 1);
    if (panel.next_card !== null) {
      panel = {
        ...panel,
        card: panel.next_card,
        next_card: null,
        remaining: Math.max(1, panel.remaining - 1),
        reviewed: (panel.reviewed ?? 0) + 1,
      };
      revealed = null;
      render(panel);
    } else {
      setBusy(true);
    }
    const asked = host.callTool("panel_grade", args).then((result) => {
      const next = studyOf(structured(result));
      if (next === null) throw new Error("panel: a grade's answer of the wrong shape");
      return next;
    });
    recording = asked;
    try {
      const next = await asked;
      recording = null;
      if (!next.recorded) tallyUp(kind, -1);
      settle(next);
      tellModel();
    } catch (err) {
      recording = null;
      tallyUp(kind, -1);
      // Back to the graded card, answer showing, so the grade can be given again.
      panel = before.panel;
      revealed = null;
      render(panel);
      showAnswer(before.revealed);
      fail(() => grade(rating), err);
    }
  }

  /**
   * The server's view after a grade. When it shows the card already on
   * screen, only its counts and the card after it are taken, so a back
   * being read stays where it is.
   * @param {StudyView} next
   */
  function settle(next) {
    if (panel === null) return;
    const merged = keepScope(next, panel);
    const same =
      next.state === "card" && panel.state === "card" && panel.card !== null && next.card?.card_id === panel.card.card_id;
    if (same) {
      panel = { ...merged, card: panel.card };
      renderProgress(panel);
      renderTally(panel);
      askAhead(panel);
      if (busy) setBusy(false);
      saveSession();
      return;
    }
    panel = merged;
    revealed = null;
    render(panel);
  }

  function tellModel() {
    if (panel === null) return;
    const text =
      `Flash study panel, session ${panel.session_id}: the user has graded ` +
      `${plural(tally.graded, "card", "cards")} in the panel (${tally.again} Again); ` +
      `${panel.remaining} left in the queue` +
      (panel.state === "card" ? "" : ", and the panel shows the session as finished") +
      ". To continue in chat instead, call start_study_session again.";
    host.tellModel(text).catch(() => {
      // The model hears of it on its next study call instead.
    });
  }

  // ---- The quick edit: the note's two fields behind the card on screen
  // (a reversed or cloze card shows only part of them), as the card has
  // them, images included. Typing and deleting only: no toolbar, a paste
  // comes in as plain text, nothing is dropped in, and the formatting
  // keys do nothing. Advanced edit, under the fields, opens the card's
  // own page on the site for the rest; a plain link, no sign-in with it.

  /**
   * @typedef {{ labels: [string, string], front_html: string, back_html: string, card_url: string | null }} EditSource
   * @typedef {{ card: PanelCard, next_card: PanelCard | null, answer: Answer, preload: string[] }} Edited
   */

  /** @param {unknown} v @returns {EditSource | null} */
  function editSourceOf(v) {
    if (!isObject(v)) return null;
    const labels = v["labels"];
    const front = str(v["front_html"]);
    const back = str(v["back_html"]);
    if (!isStrings(labels) || labels.length !== 2 || front === null || back === null) return null;
    return {
      labels: [labels[0] ?? tx("panel-front"), labels[1] ?? tx("panel-back")],
      front_html: front,
      back_html: back,
      card_url: webLink(v["card_url"]),
    };
  }

  /**
   * @param {unknown} v
   */
  function webLink(v) {
    const s = str(v);
    if (s === null) return null;
    try {
      const url = new URL(s);
      return url.protocol === "https:" || url.protocol === "http:" ? url.href : null;
    } catch {
      return null;
    }
  }

  /** @param {unknown} v @returns {Edited | null} */
  function editedOf(v) {
    if (!isObject(v)) return null;
    const card = cardOf(v["card"]);
    const answer = answerOf(v["answer"]);
    if (card === null || answer === null) return null;
    return { card, next_card: cardOf(v["next_card"]), answer, preload: isStrings(v["preload"]) ? v["preload"] : [] };
  }

  /**
   * @param {number | undefined} id
   * @param {string | null} [url]
   */
  function openAdvanced(id, url) {
    const at = url ?? (id === undefined ? null : sitePage(`/cards/${String(id)}`));
    if (at === null) return;
    host.open(at).catch(() => {
      // The host would not open it; the card is on the site all the same.
    });
  }

  /** @param {boolean} open */
  function showQuick(open) {
    $("quick").hidden = !open;
    $("card").hidden = open;
    $("quick-error").hidden = true;
    if (open) {
      button("edit").hidden = true;
      button("expand").hidden = true;
      button("advanced").hidden = editing?.url == null && panel?.site_url == null;
    } else {
      button("edit").hidden = !editOffered();
      // The card is back: whether it offers Expand is its own to say.
      fitFace();
    }
  }

  async function startEdit() {
    trace("tap", { action: "edit", busy, card: panel?.card?.card_id });
    if (busy || view !== "study" || panel?.card == null || editing !== null) return;
    const id = panel.card.card_id;
    if (quickOff) {
      openAdvanced(id);
      return;
    }
    setBusy(true);
    try {
      const source = editSourceOf(structured(await host.callTool("panel_edit_source", { card_id: id })));
      if (source === null) throw new Error("panel: a card to edit of the wrong shape");
      if (nowShowing().panel?.card?.card_id !== id) {
        setBusy(false);
        return;
      }
      editing = { card: id, answer: revealed, since: Date.now(), url: source.card_url };
      $("quick-front-label").textContent = source.labels[0];
      $("quick-back-label").textContent = source.labels[1];
      $("quick-front").innerHTML = source.front_html;
      $("quick-back").innerHTML = source.back_html;
      showQuick(true);
      setBusy(false);
      $("quick-front").focus();
    } catch (err) {
      // A host that will not run the quick edit still has the site's.
      console.warn("[flash-panel] quick edit", err);
      quickOff = true;
      setBusy(false);
      openAdvanced(id);
      button("edit").hidden = !editOffered();
    }
  }

  function closeMore() {
    $("card-more-menu").hidden = true;
    button("card-more").setAttribute("aria-expanded", "false");
  }

  function toggleMore() {
    const open = $("card-more-menu").hidden === true;
    $("card-more-menu").hidden = !open;
    button("card-more").setAttribute("aria-expanded", String(open));
    if (open) {
      const first = [button("edit"), button("archive")].find((b) => b.hidden !== true);
      first?.focus();
    }
  }

  async function archiveCard() {
    trace("tap", { action: "archive", busy, card: panel?.card?.card_id });
    closeMore();
    if (busy || view !== "study" || panel?.card == null || editing !== null) return;
    const id = panel.card.card_id;
    setBusy(true);
    try {
      const args = { session_id: panel.session_id, card_id: id, archived: true };
      const next = studyOf(structured(await host.callTool("panel_archive", args)));
      if (next === null) throw new Error("panel: an archive's answer of the wrong shape");
      if (nowShowing().panel?.card?.card_id !== id) {
        setBusy(false);
        return;
      }
      archived = id;
      panel = keepScope(next, panel);
      revealed = null;
      render(panel);
      tellArchived("archived a card");
    } catch (err) {
      // A host holding an earlier tool list will not run it again.
      console.warn("[flash-panel] archive", err);
      archiveOff = true;
      button("archive").hidden = true;
      fail(() => archiveCard(), err);
    }
  }

  /** @param {number} id the card Archive took out */
  async function unarchive(id) {
    trace("tap", { action: "undo archive", busy, card: id });
    if (busy || view !== "study" || panel === null) return;
    setBusy(true);
    try {
      const args = { session_id: panel.session_id, card_id: id, archived: false };
      const next = studyOf(structured(await host.callTool("panel_archive", args)));
      if (next === null) throw new Error("panel: an undo's answer of the wrong shape");
      archived = null;
      panel = keepScope(next, panel);
      revealed = null;
      render(panel);
      tellArchived("put an archived card back");
    } catch (err) {
      fail(() => unarchive(id), err);
    }
  }

  /** @param {string} what */
  function tellArchived(what) {
    if (panel === null) return;
    const text =
      `Flash study panel, session ${panel.session_id}: the user ${what} ` +
      `(an archived card stays in its deck but is never studied); ${panel.remaining} left in the queue.`;
    host.tellModel(text).catch(() => {
      // The model hears of it on its next study call instead.
    });
  }

  /** The front's time on screen leaves out the time spent editing. */
  function endEdit() {
    if (editing !== null && frontSince.id === editing.card) frontSince.at += Date.now() - editing.since;
    editing = null;
  }

  function cancelEdit() {
    if (editing === null) return;
    endEdit();
    showQuick(false);
    $("quick-front").replaceChildren();
    $("quick-back").replaceChildren();
  }

  /**
   * What a refused save says: the server's own sentence when it wrote one
   * for people, otherwise a plain one.
   * @param {unknown} err
   */
  function saveRefusal(err) {
    const message = err instanceof Error ? err.message : "";
    return /^[A-Z][^:]*[.!]/.test(message) ? message : tx("panel-save-failed");
  }

  async function saveEdit() {
    trace("tap", { action: "save edit", busy, card: editing?.card });
    if (busy || editing === null || panel === null) return;
    const was = editing;
    /** @type {Record<string, unknown>} */
    const args = {
      card_id: was.card,
      front_html: $("quick-front").innerHTML,
      back_html: $("quick-back").innerHTML,
    };
    const after = panel.next_card;
    if (after !== null) args["next_card_id"] = after.card_id;
    setBusy(true);
    try {
      const edited = editedOf(structured(await host.callTool("panel_save_edit", args)));
      if (edited === null) throw new Error("panel: an edit's answer of the wrong shape");
      const now = nowShowing().panel;
      if (now?.card?.card_id !== was.card || editing !== was) {
        setBusy(false);
        return;
      }
      endEdit();
      // The edit may have changed the next card too (a sibling): both
      // backs are asked again, this one's from the answer just given.
      backs.delete(was.card);
      if (after !== null) backs.delete(after.card_id);
      backs.set(was.card, Promise.resolve(edited.answer));
      panel = { ...now, card: edited.card, next_card: edited.next_card ?? now.next_card, preload: edited.preload };
      revealed = null;
      $("quick-front").replaceChildren();
      $("quick-back").replaceChildren();
      render(panel);
      if (was.answer !== null) showAnswer(edited.answer);
      tellEdited(was.card, edited);
    } catch (err) {
      $("quick-error").textContent = saveRefusal(err);
      $("quick-error").hidden = false;
      setBusy(false);
    }
  }

  /**
   * The model hears of the edit, as it hears of the grades: the card it
   * may have read in chat now reads differently.
   * @param {number} id
   * @param {Edited} edited
   */
  function tellEdited(id, edited) {
    if (panel === null) return;
    /** @param {string} html */
    const text = (html) => {
      // Parsed apart from the page, so none of its images load for this.
      const words = new DOMParser().parseFromString(html, "text/html").body.textContent.replace(/\s+/g, " ").trim();
      return words.length > 300 ? `${words.slice(0, 300)}...` : words;
    };
    const said =
      `Flash study panel, session ${panel.session_id}: the user edited card ${String(id)} in the panel. ` +
      `It now reads: front "${text(edited.card.front_html)}", back "${text(edited.answer.back_html)}". ` +
      `${panel.remaining} left in the queue.`;
    host.tellModel(said).catch(() => {
      // The model reads the card afresh when it next asks for it.
    });
  }

  // The fields take words, not formatting: these keep it that way. They
  // listen on the document, not on the fields, which ChatGPT swaps for
  // its own after this runs (see madeElsewhereAs).
  /** @param {Event} e */
  const inQuickField = (e) => {
    const fields = [document.getElementById("quick-front"), document.getElementById("quick-back")];
    return e.composedPath().some((target) => fields.includes(/** @type {HTMLElement} */ (target)));
  };
  document.addEventListener("beforeinput", (e) => {
    if (!inQuickField(e)) return;
    if (e.inputType.startsWith("format") || e.inputType === "insertFromDrop") e.preventDefault();
  });
  document.addEventListener("paste", (e) => {
    if (!inQuickField(e)) return;
    e.preventDefault();
    const words = e.clipboardData?.getData("text/plain") ?? "";
    const at = window.getSelection();
    if (words === "" || at === null || at.rangeCount === 0) return;
    const range = at.getRangeAt(0);
    range.deleteContents();
    const node = document.createTextNode(words);
    range.insertNode(node);
    range.setStartAfter(node);
    range.collapse(true);
    at.removeAllRanges();
    at.addRange(range);
  });
  document.addEventListener("drop", (e) => {
    if (inQuickField(e)) e.preventDefault();
  });
  /** @param {KeyboardEvent} e */
  const quickFieldKey = (e) => {
    if (!inQuickField(e)) return;
    const key = e.key.toLowerCase();
    if ((e.ctrlKey || e.metaKey) && (key === "b" || key === "i" || key === "u")) e.preventDefault();
  };

  /**
   * Starts a session in this panel: after more new cards are allowed, or
   * on the cards just made. A start names its own scope.
   * @param {Record<string, unknown>} scope
   */
  async function study(scope) {
    const next = payloadFrom(await host.callTool("start_study_session", scope));
    if (next?.view === "study") showSession(next);
  }

  // How many more new cards "Study N more" lets through today.
  const BOOST = 10;

  /**
   * @param {StudyView} p
   * @param {Record<string, unknown>} boost
   * @param {() => Promise<void>} retry
   */
  async function boostThenStudy(p, boost, retry) {
    setBusy(true);
    try {
      await host.callTool("boost_new_today", boost);
      /** @type {Record<string, unknown>} */
      const again = {};
      if (p.deck !== null) again["deck"] = p.deck;
      if (p.tags.length > 0) again["tags"] = p.tags;
      await study(again);
    } catch (err) {
      fail(retry, err);
    }
  }

  async function more() {
    if (busy || panel === null) return;
    const p = panel;
    /** @type {Record<string, unknown>} */
    const boost = { extra: BOOST };
    if (p.deck !== null) boost["deck"] = p.deck;
    await boostThenStudy(p, boost, more);
  }

  async function catchUp() {
    const p = panel;
    const goal = p?.goal;
    if (busy || p === null || goal === null || goal === undefined) return;
    await boostThenStudy(p, { extra: goal.catch_up, deck: goal.deck }, catchUp);
  }

  function makeCards() {
    const tags = panel?.tags ?? [];
    const label = panel?.deck_label ?? null;
    const text =
      label !== null
        ? tx("panel-ask-deck", { deck: label })
        : tags.length > 0
          ? tx("panel-ask-tags", { tags: window.flashList?.(tags, "unit") ?? tags.join(", ") })
          : tx("panel-ask-first");
    host.say(text).catch(() => {
      // The host would not take the message; the button stays to try again.
    });
  }

  // ---- The cards just made or edited, one at a time.

  /** @type {CardsView | null} */
  let written = null;
  /** @type {Map<number, ShownCard>} card id -> the card */
  const shown = new Map();
  let at = 0;

  /** @param {CardsView} next */
  function showCards(next) {
    written = next;
    shown.clear();
    for (const card of next.cards) shown.set(card.card_id, card);
    at = 0;
    showView("cards");
    const n = next.card_ids.length;
    const edited = next.mode === "edited";
    renderScope(next.deck_label, []);
    $("left").replaceChildren(
      el("span", edited ? "badge" : "badge new", tx(edited ? "panel-edited" : "panel-new-cards", { count: n })),
    );
    const studyButton = $("study");
    studyButton.hidden = !(next.mode === "created" && next.deck !== null);
    // Two words whatever the deck is called: the header names the deck.
    studyButton.textContent = tx("panel-study-now");
    $("see-all").hidden = next.deck_url === null;
    openInApp(next.deck_url);
    $("shown").style.minHeight = "";
    fitCards(true);
    run(goTo(0));
  }

  /**
   * A card's sides into `face` (the one on screen, or the measuring copy),
   * with what each said before when an edit changed it.
   * @param {HTMLElement} face
   * @param {ShownCard} card
   */
  function fillFace(face, card) {
    /** @param {string} name */
    const part = (name) => {
      const found = face.querySelector(`[data-part="${name}"]`);
      if (!(found instanceof HTMLElement)) throw new Error(`panel: no part ${name}`);
      return found;
    };
    /**
     * @param {"front" | "back"} name
     * @param {string} html
     * @param {string} lang
     * @param {string | undefined} was
     */
    const side = (name, html, lang, was) => {
      part(name).innerHTML = html;
      part(name).lang = lang;
      part(`was-${name}`).hidden = was === undefined;
      part(`was-${name}-body`).innerHTML = was ?? "";
      part(`was-${name}-body`).lang = "";
    };
    side("front", card.front_html, card.front_lang, card.before?.front_html);
    side("back", card.back_html, card.back_lang, card.before?.back_html);
    const tags = part("tags");
    tags.replaceChildren(...tagChips(card.tags, false));
    tags.hidden = card.tags.length === 0;
  }

  // The card keeps the height of the tallest card loaded, measured in a
  // hidden copy, so paging never moves the pager and buttons below it (or
  // the host's frame). An image that loads late measures again.
  let tallest = 0;
  /** @type {Set<number>} card ids already in `tallest` */
  const measured = new Set();
  /** @param {boolean} [again] */
  function fitCards(again = false) {
    const face = $("shown");
    if (view !== "cards" || face.offsetWidth === 0) return;
    if (again) {
      tallest = 0;
      measured.clear();
    }
    let probe = document.getElementById("probe");
    if (probe === null) {
      const copy = face.cloneNode(true);
      if (!(copy instanceof HTMLElement)) return;
      probe = copy;
      probe.id = "probe";
      probe.setAttribute("aria-hidden", "true");
      probe.addEventListener(
        "load",
        () => {
          fitCards(true);
        },
        true,
      );
      face.after(probe);
    }
    probe.style.width = `${face.offsetWidth}px`;
    for (const card of shown.values()) {
      if (measured.has(card.card_id)) continue;
      measured.add(card.card_id);
      fillFace(probe, card);
      tallest = Math.max(tallest, probe.offsetHeight);
    }
    face.style.minHeight = display.mode === "fullscreen" ? "" : `${tallest}px`;
    fitShown();
  }

  // The card on screen, once its box has its height: its text fitted, as
  // the study card's is (fitFace), then Expand or a box that grows.
  function fitShown() {
    const face = $("shown");
    if (view !== "cards" || face.offsetWidth === 0) return;
    face.classList.remove("grow");
    if (display.mode === "fullscreen") {
      face.style.setProperty("--fit", "1");
      offerRoom(face, true);
      return;
    }
    offerRoom(face, fitText(face));
  }
  let lastWidth = 0;
  window.addEventListener("resize", () => {
    if (window.innerWidth === lastWidth) return;
    lastWidth = window.innerWidth;
    document.documentElement.classList.toggle("overlaid", display.mode === "fullscreen" && overlaid());
    fitView();
  });

  function renderShown() {
    const id = written?.card_ids[at];
    const card = id === undefined ? undefined : shown.get(id);
    if (card === undefined) return;
    fillFace($("shown"), card);
    fitShown();
    freshFace($("shown"));
    renderPager();
  }

  function renderPager() {
    if (written === null) return;
    const n = written.card_ids.length;
    $("pager").hidden = n < 2;
    $("position").textContent = tx("panel-position", { at: at + 1, count: n });
    drawTrack(
      $("pager-track"),
      written.card_ids.map((_, i) => (i === at ? "now" : "")),
      (at + 1) / n,
    );
    button("prev").disabled = busy || at === 0;
    button("next").disabled = busy || at >= n - 1;
  }

  /**
   * Moves to card `wanted`, fetching it and the page after it when the
   * panel does not have it yet.
   * @param {number} wanted
   */
  async function goTo(wanted) {
    if (busy || written === null) return;
    const ids = written.card_ids;
    const index = Math.max(0, Math.min(ids.length - 1, wanted));
    const id = ids[index];
    if (id === undefined) return;
    if (!shown.has(id)) {
      const missing = ids
        .slice(index)
        .filter((other) => !shown.has(other))
        .slice(0, 20);
      setBusy(true);
      try {
        const page = structured(await host.callTool("panel_cards", { card_ids: missing }));
        for (const card of shownList(isObject(page) ? page["cards"] : null)) shown.set(card.card_id, card);
        fitCards();
        setBusy(false);
      } catch (err) {
        fail(() => goTo(index), err);
        return;
      }
    }
    at = index;
    $("error").hidden = true;
    renderShown();
  }

  async function studyWritten() {
    if (busy || written?.deck == null) return;
    const deck = written.deck;
    setBusy(true);
    try {
      await study({ deck });
    } catch (err) {
      fail(studyWritten, err);
    }
  }

  function seeAll() {
    if (written?.deck_url == null) return;
    host.open(written.deck_url).catch(() => {
      // The host would not open it; the link is the site's own all the same.
    });
  }

  /**
   * A page of the site the session came from, tagged as the panel's.
   * @param {string} path
   */
  function sitePage(path) {
    const base = panel?.site_url ?? null;
    if (base === null) return null;
    try {
      const url = new URL(path, base);
      url.searchParams.set("via", "panel");
      return url.href;
    } catch {
      return null;
    }
  }

  function openProgress() {
    const url = sitePage("/stats");
    if (url === null) return;
    host.open(url).catch(() => {
      // The host would not open it; nothing on the screen depends on it.
    });
  }

  // ChatGPT's own "Open in" button, which its full screen shows in the
  // header: the page of the site this view goes on to (the deck for cards
  // just made, the study page for a session). ChatGPT alone has it, and
  // takes only this server's own pages.
  let openInSet = "";
  /** @param {string | null} url */
  function openInApp(url) {
    const o = window.openai;
    if (url === null || url === openInSet || o?.setOpenInAppUrl === undefined) return;
    openInSet = url;
    try {
      Promise.resolve(o.setOpenInAppUrl({ href: url })).catch((/** @type {unknown} */ e) => {
        console.warn("[flash-panel] open in app", e);
      });
    } catch (e) {
      console.warn("[flash-panel] open in app", e);
    }
  }

  // A sideways swipe on the card pages; anything mostly vertical is the
  // page scrolling and is left alone.
  /** @type {{ x: number, y: number } | null} */
  let swipe = null;
  $("shown").addEventListener("pointerdown", (e) => {
    swipe = { x: e.clientX, y: e.clientY };
  });
  $("shown").addEventListener("pointerup", (e) => {
    if (swipe === null) return;
    const dx = e.clientX - swipe.x;
    const dy = e.clientY - swipe.y;
    swipe = null;
    const ahead = rightToLeft() ? dx > 0 : dx < 0;
    if (Math.abs(dx) > 40 && Math.abs(dx) > 2 * Math.abs(dy)) run(goTo(at + (ahead ? 1 : -1)));
  });
  $("shown").addEventListener("pointercancel", () => {
    swipe = null;
  });

  // ---- Wiring.

  $("reveal").addEventListener("click", () => {
    run(reveal());
  });
  // A tap on the front turns the card, unless it was on something of the
  // card's own (a link, a player) or ended a text selection.
  $("face").addEventListener("click", (e) => {
    if (view !== "study" || revealed !== null) return;
    const target = e.target instanceof Element ? e.target : null;
    if (target?.closest("a,audio,video,button,input,summary") != null) return;
    if ((window.getSelection()?.toString() ?? "") !== "") return;
    run(reveal());
  });
  for (const b of document.querySelectorAll("#grades button")) {
    if (!(b instanceof HTMLButtonElement)) continue;
    const rating = ratingOf(Number(b.dataset["rating"]));
    if (rating === null) continue;
    b.addEventListener("click", () => {
      run(grade(rating));
    });
  }
  $("more").addEventListener("click", () => {
    run(more());
  });
  $("goal-more").addEventListener("click", () => {
    run(catchUp());
  });
  $("make").addEventListener("click", makeCards);
  $("remind").addEventListener("click", () => {
    run(turnOnReminders());
  });
  $("prev").addEventListener("click", () => {
    run(goTo(at - 1));
  });
  $("next").addEventListener("click", () => {
    run(goTo(at + 1));
  });
  $("study").addEventListener("click", () => {
    run(studyWritten());
  });
  $("see-all").addEventListener("click", seeAll);
  $("progress").addEventListener("click", openProgress);
  $("card-more").addEventListener("click", toggleMore);
  $("edit").addEventListener("click", () => {
    closeMore();
    run(startEdit());
  });
  $("archive").addEventListener("click", () => {
    run(archiveCard());
  });
  document.addEventListener("click", (e) => {
    if (e.target instanceof Element && e.target.closest(".more-wrap") !== null) return;
    if ($("card-more-menu").hidden !== true) closeMore();
  });
  $("quick-save").addEventListener("click", () => {
    run(saveEdit());
  });
  $("quick-cancel").addEventListener("click", cancelEdit);
  $("advanced").addEventListener("click", () => {
    openAdvanced(editing?.card ?? panel?.card?.card_id, editing?.url);
  });
  for (const b of [$("expand"), ...document.querySelectorAll(".show-all")]) {
    b.addEventListener("click", () => {
      run(host.expand());
    });
  }
  $("retry").addEventListener("click", () => {
    const again = lastAction?.();
    if (again instanceof Promise) run(again);
  });
  // Keys work once the panel has focus (after a click in it); the chat
  // box usually has it. The buttons name their keys in a tooltip.
  document.addEventListener("keydown", (e) => {
    // eslint-disable-next-line @typescript-eslint/no-deprecated -- Safari ends a composition with a keydown whose isComposing is false; keyCode 229 is its only sign
    if (e.isComposing || e.keyCode === 229) return;
    quickFieldKey(e);
    const target = e.target instanceof Element ? e.target : null;
    if (e.key === "Escape" && $("card-more-menu").hidden !== true) {
      closeMore();
      button("card-more").focus();
      return;
    }
    // The quick edit's keys are words: Escape alone leaves it.
    if (editing !== null) {
      if (e.key === "Escape") cancelEdit();
      return;
    }
    if (target?.closest("input,textarea,select,[contenteditable]") != null) return;
    if (view === "cards") {
      if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
        e.preventDefault();
        const on = rightToLeft() ? "ArrowLeft" : "ArrowRight";
        run(goTo(at + (e.key === on ? 1 : -1)));
      }
      return;
    }
    if (view !== "study") return;
    const rating = ratingOf(Number(e.key));
    if (revealed === null && (e.key === " " || e.key === "Enter") && $("card").hidden === false) {
      if (target instanceof HTMLButtonElement && e.key === "Enter") return;
      e.preventDefault();
      run(reveal());
    } else if (revealed !== null && rating !== null) {
      e.preventDefault();
      run(grade(rating));
    }
  });

  // ---- Connect: the standard handshake first; the older bridge only
  // when a host never answers it.

  function useOpenAiBridge() {
    bridge = "openai";
    const o = openai();
    applyContext({ theme: o.theme });
    // The globals change for other reasons than a new result (the display
    // mode, the theme); the view is drawn again only for a new result, so
    // a session in progress never falls back to where it started.
    /** @type {unknown} */
    let lastOut;
    /** @type {unknown} */
    let lastMeta;
    const fromGlobals = () => {
      setDisplay({
        mode: o.displayMode,
        modes: o.requestDisplayMode !== undefined ? ["inline", "fullscreen"] : undefined,
        height: num(o.maxHeight),
        insets: o.safeArea?.insets,
      });
      const out = o.toolOutput;
      const meta = o.toolResponseMetadata;
      if ((out != null || meta != null) && (out !== lastOut || meta !== lastMeta)) {
        lastOut = out;
        lastMeta = meta;
        showResult({ structuredContent: out, _meta: meta });
      }
    };
    window.addEventListener("openai:set_globals", fromGlobals);
    fromGlobals();
    reportSize();
  }

  setLang();

  // The handshake's shape is the MCP Apps library's (`App.connect`):
  // `appInfo`, `appCapabilities`, `protocolVersion`. A host that checks
  // it refuses anything else, and the frame then never gets a height.
  request("ui/initialize", {
    protocolVersion: "2026-01-26",
    appInfo: { name: "flash-panel", version: "6" },
    appCapabilities: { availableDisplayModes: ["inline", "fullscreen"] },
  }).then(
    (result) => {
      if (bridge !== null) return;
      bridge = "mcp";
      applyContext(isObject(result) ? result["hostContext"] : null);
      notify("ui/notifications/initialized");
      reportSize();
    },
    () => {
      if (bridge === null) $("loading-text").textContent = tx("panel-needs-host");
    },
  );
  // A host that only offers the older bridge never answers the handshake.
  setTimeout(() => {
    if (bridge === null && window.openai !== undefined) useOpenAiBridge();
  }, 3000);
})();
