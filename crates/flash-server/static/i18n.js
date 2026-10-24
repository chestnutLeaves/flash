(function () {
  "use strict";

  /** @typedef {string | { v: string } | { s: string, k: [string, Part[]][], d: string }} Part */
  /** @typedef {{ category: string, rule: string }} Plural */
  /** @typedef {{ pair: string, start: string, middle: string, end: string, special: { when: string, then: string } | null }} Joiners */
  /** @typedef {{ and: Joiners, or: Joiners, unit: Joiners }} Lists */

  /** @type {Map<string, Part[]> | null} */
  let messages = null;
  let readFor = "";
  /** @type {string | null} */
  let format = null;
  /**
   * The language's plural forms, CLDR's rules (the header carries them):
   * evaluated here rather than by the browser's Intl.PluralRules, whose
   * CLDR may be years older than the server's.
   * @type {Plural[]}
   */
  let plurals = [];
  /** @type {Intl.NumberFormat | null} */
  let numbers = null;
  /**
   * @type {"accented" | "bidi" | null}
   */
  let pseudo = null;
  let rtl = false;
  /**
   * @type {Lists | null}
   */
  let lists = null;

  const OPEN = "\uE000";
  const CLOSE = "\uE001";
  const UPPER = "ȦƁƇḒḖƑƓĦĪĴĶĿḾȠǾƤɊŘŞŦŬṼẆẊẎẐ";
  const LOWER = "ȧƀƈḓḗƒɠħīĵķŀḿƞǿƥɋřşŧŭṽẇẋẏẑ";

  /**
   * @param {string} text
   * @param {"accented" | "bidi"} how
   * @returns {string}
   */
  function turn(text, how) {
    let out = how === "accented" ? "[" : "";
    let rest = text;
    while (rest !== "") {
      const next = rest.search(/[<&\uE000]/u);
      const cut = next === -1 ? rest.length : next;
      if (cut > 0) {
        out += how === "accented" ? accent(rest.slice(0, cut)) : mirror(rest.slice(0, cut));
        rest = rest.slice(cut);
        continue;
      }
      if (rest.startsWith(OPEN)) {
        const end = rest.indexOf(CLOSE);
        out += end === -1 ? rest.slice(1) : rest.slice(1, end);
        rest = end === -1 ? "" : rest.slice(end + 1);
      } else if (rest.startsWith("<")) {
        const end = rest.indexOf(">");
        out += (end === -1 ? rest : rest.slice(0, end + 1)).replaceAll(OPEN, "").replaceAll(CLOSE, "");
        rest = end === -1 ? "" : rest.slice(end + 1);
      } else {
        const entity = /^&[A-Za-z0-9#]+;/.exec(rest);
        const taken = entity === null ? "&" : entity[0];
        out += taken;
        rest = rest.slice(taken.length);
      }
    }
    return how === "accented" ? `${out}]` : out;
  }

  /**
   * @param {string} words
   * @returns {string}
   */
  function accent(words) {
    let out = "";
    for (const c of words) {
      const code = c.charCodeAt(0);
      const upper = code >= 65 && code <= 90;
      const lower = code >= 97 && code <= 122;
      if (!upper && !lower) {
        out += c;
        continue;
      }
      const turned = (upper ? UPPER : LOWER).charAt(code - (upper ? 65 : 97));
      out += "aeiouAEIOU".includes(c) ? turned + turned : turned;
    }
    return out;
  }

  /**
   * @param {string} words
   * @returns {string}
   */
  function mirror(words) {
    return words.replace(/\S+/gu, (piece) =>
      /\p{L}/u.test(piece) ? `\u200F\u202E${piece}\u202C\u200F` : piece,
    );
  }

  /**
   * @param {unknown} value
   * @returns {"accented" | "bidi" | null}
   */
  function pseudoOf(value) {
    return value === "accented" ? "accented" : value === "bidi" ? "bidi" : null;
  }

  window.flashPseudo = turn;

  const RIGHT_TO_LEFT = /[\u0590-\u08FF\u200F\uFB1D-\uFDFF\uFE70-\uFEFF\u{10800}-\u{10FFF}\u{1E800}-\u{1EFFF}]/u;

  /**
   * @param {string} value
   * @param {boolean} readsRtl
   * @returns {string}
   */
  function isolate(value, readsRtl) {
    return value !== "" && (readsRtl || RIGHT_TO_LEFT.test(value)) ? `\u2068${value}\u2069` : value;
  }

  window.flashIsolate = isolate;

  /**
   * A number's plural operands (UTS #35), read from how it is written:
   * "1.50" shows two fraction digits, "1.1c6" is 1100000 written
   * compactly.
   * @param {string} written
   * @returns {Record<string, number> | null}
   */
  function operands(written) {
    const match = /^-?(\d+)(?:\.(\d+))?(?:c(\d+))?$/.exec(written);
    if (match === null) return null;
    const [, int = "", fraction = "", exponent = "0"] = match;
    const c = Number(exponent);
    const digits = int + fraction;
    const point = int.length + c;
    const whole = digits.slice(0, point).padEnd(point, "0");
    const shown = digits.slice(point);
    const trimmed = shown.replace(/0+$/, "");
    return {
      n: Number(shown === "" ? whole : `${whole}.${shown}`),
      i: Number(whole),
      v: shown.length,
      w: trimmed.length,
      f: Number(shown === "" ? "0" : shown),
      t: Number(trimmed === "" ? "0" : trimmed),
      c,
      e: c,
    };
  }

  /**
   * @param {string} condition
   * @param {Record<string, number>} o
   * @returns {boolean}
   */
  function holds(condition, o) {
    if (condition === "") return true;
    return condition.split(" or ").some((all) =>
      all.split(" and ").every((relation) => {
        const match = /^([niftvwce])(?: % (\d+))? (!?=) ([\d.,]+)$/.exec(relation.trim());
        if (match === null) return false;
        const [, operand = "", modulus, op, list = ""] = match;
        const value = o[operand];
        if (value === undefined) return false;
        const x = modulus === undefined ? value : value % Number(modulus);
        const within = list.split(",").some((item) => {
          const [low = "", high] = item.split("..");
          return Number.isInteger(x) && x >= Number(low) && x <= Number(high ?? low);
        });
        return op === "=" ? within : !within;
      }),
    );
  }

  /**
   * @param {Plural[]} forms
   * @param {number | string} value
   * @returns {string}
   */
  function pluralOf(forms, value) {
    // Digits as written, never an exponent (1e21 is 21 digits).
    const written =
      typeof value === "number"
        ? value.toLocaleString("en-US", { useGrouping: false, maximumFractionDigits: 20 })
        : value;
    const o = operands(written);
    if (o === null) return "other";
    const condition = (/** @type {Plural} */ p) => p.rule.split("@")[0]?.trim() ?? "";
    return forms.find((p) => p.category !== "other" && holds(condition(p), o))?.category ?? "other";
  }

  window.flashPlural = pluralOf;

  /**
   * @param {unknown} value
   * @returns {Part | null}
   */
  function asPart(value) {
    if (typeof value === "string") return value;
    if (typeof value !== "object" || value === null) return null;
    if ("v" in value && typeof value.v === "string") return { v: value.v };
    if (!("s" in value && "k" in value && "d" in value)) return null;
    const { s, k, d } = value;
    if (typeof s !== "string" || typeof d !== "string" || !Array.isArray(k)) return null;
    /** @type {[string, Part[]][]} */
    const keys = [];
    for (const pair of /** @type {unknown[]} */ (k)) {
      if (!Array.isArray(pair) || pair.length !== 2) return null;
      const [key, parts] = /** @type {unknown[]} */ (pair);
      const read = asParts(parts);
      if (typeof key !== "string" || read === null) return null;
      keys.push([key, read]);
    }
    return { s, k: keys, d };
  }

  /**
   * @param {unknown} value
   * @returns {Part[] | null}
   */
  function asParts(value) {
    if (!Array.isArray(value)) return null;
    /** @type {Part[]} */
    const out = [];
    for (const item of /** @type {unknown[]} */ (value)) {
      const part = asPart(item);
      if (part === null) return null;
      out.push(part);
    }
    return out;
  }

  /**
   * @param {readonly string[]} items
   * @param {Joiners} joiners
   * @returns {string}
   */
  function joinList(items, joiners) {
    const first = items[0];
    if (first === undefined) return "";
    if (items.length === 1) return first;
    const end = items[items.length - 1] ?? "";
    /** @type {(joiner: string) => string} */
    const last = (joiner) =>
      joiners.special !== null && new RegExp(joiners.special.when, "u").test(end) ? joiners.special.then : joiner;
    if (items.length === 2) return first + last(joiners.pair) + end;
    return first + joiners.start + items.slice(1, -1).join(joiners.middle) + last(joiners.end) + end;
  }

  window.flashJoinList = joinList;

  /**
   * @param {unknown} value
   * @returns {Joiners | null}
   */
  function joinersOf(value) {
    if (typeof value !== "object" || value === null) return null;
    if (!("pair" in value) || typeof value.pair !== "string") return null;
    if (!("start" in value) || typeof value.start !== "string") return null;
    if (!("middle" in value) || typeof value.middle !== "string") return null;
    if (!("end" in value) || typeof value.end !== "string") return null;
    const given = "special" in value ? value.special : null;
    /** @type {{ when: string, then: string } | null} */
    let special = null;
    if (typeof given === "object" && given !== null && "when" in given && "then" in given) {
      if (typeof given.when === "string" && typeof given.then === "string") {
        special = { when: given.when, then: given.then };
      }
    }
    return { pair: value.pair, start: value.start, middle: value.middle, end: value.end, special };
  }

  /**
   * @param {object} data
   * @returns {Lists | null}
   */
  function listsOf(data) {
    if (!("lists" in data) || typeof data.lists !== "object" || data.lists === null) return null;
    const given = data.lists;
    const and = joinersOf("and" in given ? given.and : null);
    const or = joinersOf("or" in given ? given.or : null);
    const unit = joinersOf("unit" in given ? given.unit : null);
    return and === null || or === null || unit === null ? null : { and, or, unit };
  }

  /**
   * @param {string} kind
   * @returns {object[]}
   */
  function islands(kind) {
    const out = [];
    for (const island of document.querySelectorAll(`script[data-flash-${kind}]`)) {
      try {
        /** @type {unknown} */
        const data = JSON.parse(island.textContent);
        if (typeof data === "object" && data !== null) out.push(data);
      } catch {
        // An island that does not parse words nothing.
      }
    }
    return out;
  }

  /**
   * @returns {{ lang: string, folders: string[], format: string | null, plurals: Plural[], pseudo: "accented" | "bidi" | null, rtl: boolean, lists: Lists | null }[]}
   */
  function headers() {
    const out = [];
    for (const data of islands("lang")) {
      if (!("lang" in data) || typeof data.lang !== "string") continue;
      if (!("folders" in data) || !Array.isArray(data.folders)) continue;
      const names = /** @type {unknown[]} */ (data.folders).filter((f) => typeof f === "string");
      /** @type {Plural[]} */
      const forms = [];
      const given = "plurals" in data && Array.isArray(data.plurals) ? /** @type {unknown[]} */ (data.plurals) : [];
      for (const form of given) {
        if (typeof form !== "object" || form === null) continue;
        if (!("category" in form) || typeof form.category !== "string") continue;
        if (!("rule" in form) || typeof form.rule !== "string") continue;
        forms.push({ category: form.category, rule: form.rule });
      }
      out.push({
        lang: data.lang,
        folders: names,
        format: "format" in data && typeof data.format === "string" ? data.format : null,
        plurals: forms,
        pseudo: pseudoOf("pseudo" in data ? data.pseudo : null),
        rtl: "dir" in data && data.dir === "rtl",
        lists: listsOf(data),
      });
    }
    return out;
  }

  /**
   * @returns {{ folder: string, m: object }[]}
   */
  function folders() {
    const out = [];
    for (const data of islands("i18n")) {
      if (!("folder" in data) || typeof data.folder !== "string") continue;
      if (!("m" in data) || typeof data.m !== "object" || data.m === null) continue;
      out.push({ folder: data.folder, m: data.m });
    }
    return out;
  }

  function load() {
    const want = document.documentElement.lang;
    if (messages !== null && readFor === want) return messages;
    readFor = want;
    messages = new Map();
    const all = headers();
    const chosen = all.find((header) => header.lang === want) ?? all[0];
    format = chosen?.format ?? null;
    plurals = chosen?.plurals ?? [];
    pseudo = chosen?.pseudo ?? null;
    rtl = chosen?.rtl ?? false;
    lists = chosen?.lists ?? null;
    const held = folders();
    for (const folder of chosen?.folders ?? []) {
      for (const island of held) {
        if (island.folder !== folder) continue;
        for (const [id, value] of Object.entries(island.m)) {
          const parts = asParts(value);
          if (parts !== null) messages.set(id, parts);
        }
      }
    }
    numbers = format === null ? null : new Intl.NumberFormat(format);
    return messages;
  }

  /**
   * @param {string | number | undefined} value
   * @returns {string}
   */
  function show(value) {
    if (typeof value === "number") return numbers === null ? String(value) : numbers.format(value);
    const text = isolate(value ?? "", rtl);
    return pseudo === null ? text : `${OPEN}${text}${CLOSE}`;
  }

  /**
   * @param {Part[]} parts
   * @param {Record<string, string | number>} vars
   * @returns {string}
   */
  function word(parts, vars) {
    let out = "";
    for (const part of parts) {
      if (typeof part === "string") out += part;
      else if ("v" in part) out += show(vars[part.v]);
      else out += choose(part, vars);
    }
    return out;
  }

  /**
   * @param {{ s: string, k: [string, Part[]][], d: string }} part
   * @param {Record<string, string | number>} vars
   * @returns {string}
   */
  function choose(part, vars) {
    const value = vars[part.s];
    /** @type {(key: string) => Part[] | undefined} */
    const find = (key) => part.k.find(([k]) => k === key)?.[1];
    let parts;
    if (typeof value === "number") {
      parts = find(String(value)) ?? find(pluralOf(plurals, value));
    } else if (typeof value === "string") {
      parts = find(value);
    }
    return word(parts ?? find(part.d) ?? [], vars);
  }

  /**
   * The message `id` in the reader's language, with `vars`; the id itself
   * when the page carries no such message.
   * @param {string} id
   * @param {Record<string, string | number>} [vars]
   * @returns {string}
   */
  window.flashT = function (id, vars) {
    const parts = load().get(id);
    if (parts === undefined) return id;
    const worded = word(parts, vars ?? {});
    return pseudo === null ? worded : turn(worded, pseudo);
  };

  /**
   * `items` as one list in the reader's language, as the server writes
   * it: `unit` a plain enumeration ("a, b, c" in English, "a, b y c" in
   * Spanish), `and` and `or` with the language's word for them.
   * @param {readonly string[]} items
   * @param {"and" | "or" | "unit"} kind
   * @returns {string}
   */
  window.flashList = function (items, kind) {
    load();
    return lists === null ? items.join(", ") : joinList(items, lists[kind]);
  };
})();
