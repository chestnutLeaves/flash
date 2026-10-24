(function () {
  "use strict";

  /**
   * @param {string} id
   * @param {Record<string, string | number>} [vars]
   * @returns {string}
   */
  const tx = (id, vars) => (window.flashT === undefined ? id : window.flashT(id, vars));

  const { one, maybe, all, closestTo, composing } = window.flashDom;

  function typingInField() {
    const el = document.activeElement;
    return (
      el instanceof HTMLInputElement ||
      el instanceof HTMLTextAreaElement ||
      el instanceof HTMLSelectElement ||
      (el instanceof HTMLElement && el.isContentEditable)
    );
  }

  const section = maybe(document, "[data-deck-cards]", HTMLElement);
  if (section === null) return;
  const deckId = section.getAttribute("data-deck-cards") ?? "";

  function listState() {
    const el = one(document, "#cards-state", HTMLElement);
    return {
      total: Number(el.getAttribute("data-total") ?? "0"),
      shown: Number(el.getAttribute("data-shown") ?? "0"),
      bulk: el.getAttribute("data-bulk") ?? `/decks/${deckId}/cards/bulk`,
      archived: el.hasAttribute("data-archived"),
    };
  }

  let allMatching = false;
  let last = -1;

  const boxes = () => all(document, "#cards tbody tr[data-card] input[type=checkbox]", HTMLInputElement);
  const selectedIds = () => boxes().filter((b) => b.checked).map((b) => b.value);

  function sync() {
    const list = boxes();
    const ids = list.filter((b) => b.checked);
    const state = listState();
    for (const b of list) b.closest("tr")?.classList.toggle("is-selected", b.checked);
    const header = maybe(document, "#deck-all", HTMLInputElement);
    if (header !== null) {
      header.checked = ids.length > 0 && ids.length === list.length;
      header.indeterminate = ids.length > 0 && ids.length < list.length;
    }
    if (ids.length < list.length) allMatching = false;
    const bar = one(document, "#deck-bulk", HTMLElement);
    bar.hidden = ids.length === 0;
    if (ids.length === 0) confirming(false);
    const count = allMatching ? state.total : ids.length;
    one(bar, "#deck-sel-n", HTMLElement).textContent = tx("js-deck-selected", { count });
    const more = one(bar, "[data-bulk-all]", HTMLButtonElement);
    more.hidden = allMatching || ids.length !== list.length || state.total <= state.shown;
    more.textContent = tx("js-deck-select-all", { count: state.total });
    one(bar, "[data-bulk=archive]", HTMLButtonElement).hidden = state.archived;
    one(bar, "[data-bulk=unarchive]", HTMLButtonElement).hidden = !state.archived;
    one(bar, "#deck-bulk-delete", HTMLButtonElement).textContent = tx("js-deck-delete-n", { count });
  }

  /** @param {boolean} on */
  function confirming(on) {
    const bar = one(document, "#deck-bulk", HTMLElement);
    one(bar, ".deck-bulk-main", HTMLElement).hidden = on;
    one(bar, ".deck-bulk-confirm", HTMLElement).hidden = !on;
    if (on) one(bar, "#deck-bulk-delete", HTMLButtonElement).focus();
  }

  function clearSelection() {
    for (const b of boxes()) b.checked = false;
    allMatching = false;
    last = -1;
    sync();
  }

  document.addEventListener("click", (e) => {
    const box = e.target;
    if (!(box instanceof HTMLInputElement) || box.closest("#cards tbody tr[data-card]") === null) return;
    const list = boxes();
    const at = list.indexOf(box);
    if (e.shiftKey && last >= 0 && at >= 0) {
      const [from, to] = at < last ? [at, last] : [last, at];
      for (const b of list.slice(from, to + 1)) b.checked = box.checked;
    }
    last = at;
    sync();
  });

  document.addEventListener("change", (e) => {
    const header = e.target;
    if (!(header instanceof HTMLInputElement) || header.id !== "deck-all") return;
    for (const b of boxes()) b.checked = header.checked;
    allMatching = false;
    sync();
  });

  function filterParams() {
    const params = new URLSearchParams();
    const q = maybe(document, "#deck-q", HTMLInputElement);
    if (q !== null && q.value.trim() !== "") params.set("q", q.value.trim());
    const archived = maybe(document, "#deck-filter [data-status=archived][aria-current=true]", HTMLButtonElement);
    if (archived !== null) params.set("status", "archived");
    const decks = all(document, "#deck-filter input[data-facet]:checked", HTMLInputElement).map(
      (b) => b.getAttribute("data-facet") ?? "",
    );
    if (decks.length > 0) params.set("decks", decks.join(","));
    const tags = all(document, "#deck-filter input[data-tag]:checked", HTMLInputElement).map(
      (b) => b.getAttribute("data-tag") ?? "",
    );
    if (tags.length > 0) params.set("tags", tags.join(","));
    if (tags.length > 1 && matchAll) params.set("match", "all");
    return params;
  }

  let matchAll = maybe(document, "#cards [data-match=all][aria-current=true]", HTMLButtonElement) !== null;

  function reload() {
    const htmx = window.htmx;
    if (htmx === undefined) return;
    const query = filterParams().toString();
    const path = `/decks/${deckId}/cards${query === "" ? "" : `?${query}`}`;
    htmx.ajax("GET", path, { target: "#cards", swap: "innerHTML" }).catch((err) => {
      console.error("deck.js: loading the list", path, err);
    });
  }

  function countFilter() {
    const n = all(document, "#cards .deck-chip", HTMLElement).length;
    const badge = one(document, "#deck-filter-n", HTMLElement);
    badge.textContent = String(n);
    badge.hidden = n === 0;
  }

  /** @type {number | undefined} */
  let searching;
  document.addEventListener("input", (e) => {
    const field = e.target;
    if (!(field instanceof HTMLInputElement)) return;
    if (field.id === "deck-q") {
      window.clearTimeout(searching);
      searching = window.setTimeout(reload, 300);
    } else if (field.id === "deck-tagq") {
      const want = field.value.trim().toLowerCase();
      for (const opt of all(document, "#deck-filter [data-tag-opt]", HTMLElement)) {
        opt.hidden = want !== "" && !(opt.getAttribute("data-tag-opt") ?? "").includes(want);
      }
    }
  });

  document.addEventListener("change", (e) => {
    const box = e.target;
    if (box instanceof HTMLInputElement && box.closest("#deck-filter") !== null && box.type === "checkbox") reload();
  });

  /** @param {"active" | "archived"} status */
  function setStatus(status) {
    for (const b of all(document, "#deck-filter [data-status]", HTMLButtonElement)) {
      if (b.getAttribute("data-status") === status) b.setAttribute("aria-current", "true");
      else b.removeAttribute("aria-current");
    }
  }

  function closeFilter() {
    const filter = maybe(document, "#deck-filter", HTMLDetailsElement);
    if (filter !== null) filter.open = false;
  }

  document.addEventListener("click", (e) => {
    if (closestTo(e.target, "#deck-filter") === null) closeFilter();
  });

  document.addEventListener("click", (e) => {
    const status = closestTo(e.target, "#deck-filter [data-status]");
    if (status !== null) {
      setStatus(status.getAttribute("data-status") === "archived" ? "archived" : "active");
      closeFilter();
      reload();
      return;
    }
    const match = closestTo(e.target, "#cards [data-match]");
    if (match !== null) {
      matchAll = match.getAttribute("data-match") === "all";
      reload();
      return;
    }
    const chip = closestTo(e.target, "#cards [data-unfilter]");
    if (chip !== null) {
      const kind = chip.getAttribute("data-unfilter");
      const value = chip.getAttribute("data-value") ?? "";
      const uncheck = (/** @type {string} */ selector) => {
        for (const b of all(document, selector, HTMLInputElement)) b.checked = false;
      };
      if (kind === "all") {
        uncheck("#deck-filter input[type=checkbox]");
        setStatus("active");
        const q = maybe(document, "#deck-q", HTMLInputElement);
        if (q !== null) q.value = "";
      } else if (kind === "status") {
        setStatus("active");
      } else if (kind === "deck") {
        uncheck(`#deck-filter input[data-facet="${CSS.escape(value)}"]`);
      } else if (kind === "tag") {
        uncheck(`#deck-filter input[data-tag="${CSS.escape(value)}"]`);
      }
      reload();
      return;
    }
    const from = closestTo(e.target, "[data-from]");
    if (from !== null) {
      const ref = from.getAttribute("data-from") ?? "";
      const facet = maybe(document, `#deck-filter input[data-facet="${CSS.escape(ref)}"]`, HTMLInputElement);
      if (facet === null) return;
      for (const b of all(document, "#deck-filter input[data-facet]", HTMLInputElement)) b.checked = b === facet;
      reload();
    }
  });

  /**
   * @param {string} action
   * @param {Record<string, string>} [extra]
   * @param {string[]} [ids]
   */
  function act(action, extra, ids) {
    const htmx = window.htmx;
    if (htmx === undefined) return;
    const state = listState();
    /** @type {Record<string, string>} */
    const values = { action, ...extra };
    if (ids !== undefined) values["ids"] = ids.join(",");
    else if (allMatching) {
      values["all"] = "true";
      values["expect"] = String(state.total);
    } else values["ids"] = selectedIds().join(",");
    for (const menu of all(document, "#deck-bulk details[open]", HTMLDetailsElement)) menu.open = false;
    htmx.ajax("POST", state.bulk, { target: "#cards", swap: "innerHTML", values }).catch((err) => {
      console.error("deck.js: acting on cards", action, err);
    });
  }

  document.addEventListener("click", (e) => {
    const all_ = closestTo(e.target, "[data-bulk-all]");
    if (all_ !== null) {
      for (const b of boxes()) b.checked = true;
      allMatching = true;
      sync();
      return;
    }
    const button = closestTo(e.target, "[data-bulk]");
    if (button === null) return;
    const kind = button.getAttribute("data-bulk") ?? "";
    switch (kind) {
      case "clear":
        clearSelection();
        return;
      case "ask-delete":
        confirming(true);
        return;
      case "cancel":
        confirming(false);
        return;
      case "move":
        act("move", { to: one(document, "#deck-bulk-to", HTMLSelectElement).value });
        return;
      case "tag":
      case "untag": {
        const field = one(document, "#deck-bulk-tags", HTMLInputElement);
        if (field.value.trim() === "") {
          field.focus();
          return;
        }
        act(kind, { tags: field.value });
        field.value = "";
        return;
      }
      case "archive":
      case "unarchive":
      case "delete":
        act(kind);
        return;
      default:
        return;
    }
  });

  document.addEventListener("click", (e) => {
    const button = closestTo(e.target, "#cards [data-row-act]");
    const row = button?.closest("tr[data-card]");
    if (button === null || row === null || row === undefined) return;
    const id = row.getAttribute("data-card") ?? "";
    const kind = button.getAttribute("data-row-act") ?? "";
    if (kind !== "delete") {
      act(kind, {}, [id]);
      return;
    }
    // A double click's second click is not the confirm.
    if (e.detail > 1) return;
    if (button.hasAttribute("data-armed")) {
      act("delete", {}, [id]);
      return;
    }
    const label = button.getAttribute("aria-label") ?? "";
    const title = button.title;
    button.setAttribute("data-armed", "");
    button.setAttribute("aria-label", tx("js-deck-row-confirm"));
    button.title = tx("js-deck-row-confirm");
    window.setTimeout(() => {
      button.removeAttribute("data-armed");
      button.setAttribute("aria-label", label);
      button.title = title;
    }, 3000);
  });

  let adding = false;

  /** @param {boolean} open */
  function setAdd(open) {
    const form = maybe(document, "#deck-add", HTMLFormElement);
    const button = maybe(document, "[data-add-open]", HTMLButtonElement);
    if (form === null || button === null) return;
    adding = open;
    form.hidden = !open;
    button.setAttribute("aria-expanded", String(open));
    one(button, "[data-add-label]", HTMLElement).textContent = tx(open ? "js-deck-close" : "js-deck-add");
    if (open) {
      const field = form.querySelector('.editor-field[data-field="front"]:not([hidden]), #card-front');
      if (field instanceof HTMLElement) field.focus();
    }
  }

  document.addEventListener("click", (e) => {
    if (closestTo(e.target, "[data-add-open]") !== null) setAdd(!adding);
    else if (closestTo(e.target, "[data-add-write]") !== null) setAdd(true);
    else if (closestTo(e.target, "[data-add-close]") !== null) setAdd(false);
  });

  document.addEventListener("click", (e) => {
    const more = closestTo(e.target, "[data-subs-all]");
    if (more !== null) {
      for (const row of all(document, "#deck-subdecks [data-sub-more]", HTMLElement)) row.hidden = false;
      more.remove();
      return;
    }
    if (closestTo(e.target, "[data-subnew-open]") !== null) {
      const nav = one(document, "#deck-subdecks", HTMLElement);
      const form = one(nav, "#deck-subnew", HTMLFormElement);
      nav.hidden = false;
      form.hidden = false;
      for (const b of all(document, "[data-subnew-open]", HTMLElement)) b.setAttribute("aria-expanded", "true");
      one(form, "input[name=name]", HTMLInputElement).focus();
      return;
    }
    if (closestTo(e.target, "[data-subnew-cancel]") !== null) {
      one(document, "#deck-subnew", HTMLFormElement).hidden = true;
      for (const b of all(document, "[data-subnew-open]", HTMLElement)) b.setAttribute("aria-expanded", "false");
    }
  });

  document.addEventListener("keydown", (e) => {
    // A menu or the editor that closed on this key has taken it (ui.js, editor.js).
    if (e.defaultPrevented || composing(e) || e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.key === "Escape") {
      const filter = maybe(document, "#deck-filter[open]", HTMLDetailsElement);
      if (filter !== null) {
        filter.open = false;
        maybe(filter, "summary", HTMLElement)?.focus();
        return;
      }
      if (document.querySelector("details[open], dialog[open]") !== null) return;
      if (one(document, ".deck-bulk-confirm", HTMLElement).hidden === false) confirming(false);
      else if (selectedIds().length > 0) clearSelection();
      return;
    }
    if (typingInField()) return;
    if (e.key === "/") {
      const q = maybe(document, "#deck-q", HTMLInputElement);
      if (q === null) return;
      e.preventDefault();
      q.focus();
    } else if (e.key === "n" || e.key === "N") {
      if (maybe(document, "#deck-add", HTMLFormElement) === null) return;
      e.preventDefault();
      setAdd(true);
    }
  });

  /** @type {number | undefined} */
  let toastTimer;
  document.body.addEventListener("htmx:afterSwap", (e) => {
    const id = e.detail.target?.id;
    if (id === "cards" || id === "deck-main") {
      allMatching = false;
      last = -1;
      matchAll = maybe(document, "#cards [data-match=all][aria-current=true]", HTMLButtonElement) !== null;
      countFilter();
      sync();
      if (id === "deck-main" && adding) setAdd(true);
    }
  });
  document.body.addEventListener("htmx:oobAfterSwap", (e) => {
    if (e.detail.target.id !== "deck-toast") return;
    window.clearTimeout(toastTimer);
    toastTimer = window.setTimeout(() => {
      const toast = maybe(document, "#deck-toast", HTMLElement);
      if (toast !== null && !toast.matches(":hover, :focus-within")) toast.hidden = true;
    }, 6000);
  });
  /** @param {CustomEvent<HtmxRequestDetail>} e */
  const failed = (e) => {
    if (e.detail.target.id !== "cards") return;
    const toast = one(document, "#deck-toast", HTMLElement);
    const said = e.detail.xhr.responseText.trim();
    const plain = (e.detail.xhr.getResponseHeader("content-type") ?? "").startsWith("text/plain");
    toast.replaceChildren(document.createTextNode(plain && said !== "" && said.length < 300 ? said : tx("js-deck-failed")));
    toast.hidden = false;
    window.clearTimeout(toastTimer);
    toastTimer = window.setTimeout(() => {
      toast.hidden = true;
    }, 6000);
  };
  document.body.addEventListener("htmx:responseError", failed);
  document.body.addEventListener("htmx:sendError", failed);

  // Looked up once: an Add swaps `section` out of the page.
  const page = section.closest(".page");
  const centre = () => {
    if (!(page instanceof HTMLElement)) return;
    const box = page.getBoundingClientRect();
    document.documentElement.style.setProperty("--deck-bar-x", `${String(box.left + box.width / 2)}px`);
  };
  window.addEventListener("resize", centre);
  centre();
  sync();
})();
