// What the server sends the study panel (mcp.rs: PanelOut, PanelCardsOut,
// ...), checked on the way in: a host passes along whatever it is given,
// so nothing reaches the screen until it has the shape it should. Inline
// in the panel's page ahead of panel.js, which reads these from
// `window.flashPayload`.

/**
 * @typedef {{ card_id: number, front_html: string, front_lang: string }} PanelCard
 * @typedef {{ at_ms: number, in_days: number, cards: number }} NextDue
 * @typedef {{ at_ms: number, cards: number }} DayAhead
 * @typedef {object} PanelGoal the goal a finished session counted toward (PanelGoalOut)
 * @property {string} deck the deck it is on, as boost_new_today names it
 * @property {number} percent learned, out of 100
 * @property {boolean} behind
 * @property {string} head
 * @property {string} caption
 * @property {string} percent_label
 * @property {number} catch_up new cards the button adds; 0 for none
 * @property {string} study_more
 * @typedef {"card" | "held" | "empty" | "caught_up"} StudyState
 * @typedef {object} StudyView a session's state (PanelOut)
 * @property {"study"} view
 * @property {number} session_id
 * @property {StudyState} state
 * @property {PanelCard | null} card
 * @property {PanelCard | null} next_card
 * @property {number} remaining
 * @property {number | null} reviewed null from a server that did not count them
 * @property {number} held_by_limit
 * @property {string[]} preload
 * @property {boolean} recorded
 * @property {string | null} deck_label
 * @property {string | null} deck
 * @property {string[]} tags
 * @property {NextDue | null} next_due
 * @property {DayAhead[]} week
 * @property {boolean} reminders_offer
 * @property {PanelGoal | null} goal
 * @property {string | null} site_url the site, for "See your progress"; null from an older server
 * @typedef {{ front_html?: string, back_html?: string }} Before
 * @typedef {{ card_id: number, front_html: string, front_lang: string, back_html: string, back_lang: string, tags: string[], before: Before | null }} ShownCard
 * @typedef {object} CardsView cards just made or edited (PanelCardsOut)
 * @property {"cards"} view
 * @property {"created" | "edited"} mode
 * @property {string | null} deck_label
 * @property {string | null} deck
 * @property {string | null} deck_url
 * @property {number[]} card_ids
 * @property {ShownCard[]} cards
 * @typedef {{ card_id: number, back_html: string, back_lang: string, comes_back_in_ms: number[] | null }} Answer
 * @typedef {object} FlashPayload the readers panel.js takes
 * @property {(value: unknown) => value is Record<string, unknown>} isObject
 * @property {(v: unknown) => v is string[]} isStrings
 * @property {(v: unknown) => string | null} str
 * @property {(v: unknown) => number} count
 * @property {(v: unknown) => PanelCard | null} cardOf
 * @property {(v: unknown) => StudyView | null} studyOf
 * @property {(v: unknown) => ShownCard[]} shownList
 * @property {(v: unknown) => CardsView | null} cardsOf
 * @property {(v: unknown) => Answer | null} answerOf
 */

(() => {
  "use strict";

  /** @param {unknown} value @returns {value is Record<string, unknown>} */
  function isObject(value) {
    return typeof value === "object" && value !== null && !Array.isArray(value);
  }
  /** @param {unknown} v */
  const isInt = (v) => typeof v === "number" && Number.isInteger(v);
  /** @param {unknown} v @returns {v is string[]} */
  const isStrings = (v) => Array.isArray(v) && v.every((s) => typeof s === "string");
  /** @param {unknown} v */
  const str = (v) => (typeof v === "string" ? v : null);
  /** @param {unknown} v */
  const count = (v) => (isInt(v) && typeof v === "number" && v >= 0 ? v : 0);

  /** @param {unknown} v @returns {PanelCard | null} */
  function cardOf(v) {
    if (!isObject(v)) return null;
    const id = v["card_id"];
    const front = v["front_html"];
    const lang = str(v["front_lang"]) ?? "";
    return typeof id === "number" && isInt(id) && typeof front === "string" ? { card_id: id, front_html: front, front_lang: lang } : null;
  }

  /** @param {unknown} v @returns {PanelGoal | null} */
  function goalOf(v) {
    if (!isObject(v)) return null;
    const deck = str(v["deck"]);
    const head = str(v["head"]);
    if (deck === null || head === null) return null;
    return {
      deck,
      percent: Math.min(count(v["percent"]), 100),
      behind: v["behind"] === true,
      head,
      caption: str(v["caption"]) ?? "",
      percent_label: str(v["percent_label"]) ?? "",
      catch_up: count(v["catch_up"]),
      study_more: str(v["study_more"]) ?? "",
    };
  }

  /** @type {readonly StudyState[]} */
  const STATES = ["card", "held", "empty", "caught_up"];

  /**
   * @param {unknown} v
   * @returns {StudyView | null}
   */
  function studyOf(v) {
    if (!isObject(v)) return null;
    if (v["view"] !== "study" && v["view"] !== undefined) return null;
    const session = v["session_id"];
    const state = STATES.find((s) => s === v["state"]);
    if (typeof session !== "number" || !isInt(session) || state === undefined) return null;
    const card = cardOf(v["card"]);
    if (state === "card" && card === null) return null;
    const next = v["next_due"];
    const week = v["week"];
    const reviewed = v["reviewed"];
    return {
      view: "study",
      session_id: session,
      state,
      card,
      next_card: cardOf(v["next_card"]),
      remaining: count(v["remaining"]),
      reviewed: typeof reviewed === "number" && isInt(reviewed) ? reviewed : null,
      held_by_limit: count(v["held_by_limit"]),
      preload: isStrings(v["preload"]) ? v["preload"] : [],
      recorded: v["recorded"] !== false,
      deck_label: str(v["deck_label"]),
      deck: str(v["deck"]),
      tags: isStrings(v["tags"]) ? v["tags"] : [],
      next_due:
        isObject(next) && typeof next["at_ms"] === "number" && typeof next["in_days"] === "number"
          ? { at_ms: next["at_ms"], in_days: next["in_days"], cards: count(next["cards"]) }
          : null,
      week: Array.isArray(week)
        ? week.flatMap((/** @type {unknown} */ d) =>
            isObject(d) && typeof d["at_ms"] === "number" ? [{ at_ms: d["at_ms"], cards: count(d["cards"]) }] : [],
          )
        : [],
      reminders_offer: v["reminders_offer"] === true,
      goal: goalOf(v["goal"]),
      site_url: str(v["site_url"]),
    };
  }

  /** @param {unknown} v @returns {ShownCard | null} */
  function shownOf(v) {
    const card = cardOf(v);
    if (card === null || !isObject(v)) return null;
    const back = v["back_html"];
    if (typeof back !== "string") return null;
    const before = v["before"];
    /** @type {Before | null} */
    let was = null;
    if (isObject(before)) {
      was = {};
      const front = str(before["front_html"]);
      const backWas = str(before["back_html"]);
      if (front !== null) was.front_html = front;
      if (backWas !== null) was.back_html = backWas;
    }
    return {
      ...card,
      back_html: back,
      back_lang: str(v["back_lang"]) ?? "",
      tags: isStrings(v["tags"]) ? v["tags"] : [],
      before: was,
    };
  }

  /** @param {unknown} v */
  function shownList(v) {
    return Array.isArray(v) ? v.flatMap((c) => shownOf(c) ?? []) : [];
  }

  /**
   * @param {unknown} v
   * @returns {CardsView | null}
   */
  function cardsOf(v) {
    if (!isObject(v) || v["view"] !== "cards") return null;
    const ids = v["card_ids"];
    if (!Array.isArray(ids) || ids.length === 0) return null;
    const cardIds = ids.flatMap((/** @type {unknown} */ id) => (typeof id === "number" && isInt(id) ? [id] : []));
    if (cardIds.length !== ids.length) return null;
    const cards = v["cards"];
    if (cards !== undefined && !Array.isArray(cards)) return null;
    return {
      view: "cards",
      mode: v["mode"] === "edited" ? "edited" : "created",
      deck_label: str(v["deck_label"]),
      deck: str(v["deck"]),
      deck_url: str(v["deck_url"]),
      card_ids: cardIds,
      cards: shownList(cards),
    };
  }

  /** @param {unknown} v @returns {Answer | null} */
  function answerOf(v) {
    if (!isObject(v)) return null;
    const id = v["card_id"];
    const back = v["back_html"];
    if (typeof id !== "number" || !isInt(id) || typeof back !== "string") return null;
    const times = v["comes_back_in_ms"];
    return {
      card_id: id,
      back_html: back,
      back_lang: str(v["back_lang"]) ?? "",
      comes_back_in_ms: Array.isArray(times) && times.every((t) => typeof t === "number") ? times : null,
    };
  }

  window.flashPayload = { isObject, isStrings, str, count, cardOf, studyOf, shownList, cardsOf, answerOf };
})();
