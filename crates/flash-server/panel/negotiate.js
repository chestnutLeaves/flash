(() => {
  "use strict";

  /**
   * @typedef {{ kind: "any" } | { kind: "only", list: string[] } | { kind: "all_but", list: string[] }} Regions
   * @typedef {{ kind: "yes" } | { kind: "no" } | { kind: "zone", zones: { prefixes: string[], exact: string[] } }} Regionless
   * @typedef {object} Language a row of flash_core::locale::LANGUAGES
   * @property {string} tag
   * @property {string} lang
   * @property {string[]} scripts
   * @property {Regions} regions
   * @property {Regionless} regionless
   * @property {string[]} standsInFor
   * @typedef {{ lang: string, script: string | null, region: string | null }} Pref
   * @typedef {{ locale: string, formatTag: string }} Negotiated
   */

  /**
   * @param {unknown} value
   * @returns {value is Record<string, unknown>}
   */
  function isObject(value) {
    return typeof value === "object" && value !== null && !Array.isArray(value);
  }

  /**
   * @param {unknown} value
   * @returns {string[] | null}
   */
  function strings(value) {
    if (!Array.isArray(value)) return null;
    /** @type {string[]} */
    const out = [];
    for (const item of value) {
      if (typeof item !== "string") return null;
      out.push(item);
    }
    return out;
  }

  /**
   * @param {unknown} value
   * @returns {Regions | null}
   */
  function regions(value) {
    if (!isObject(value)) return null;
    const kind = value["kind"];
    if (kind === "any") return { kind };
    const list = strings(value["list"]);
    if (list === null) return null;
    if (kind === "only" || kind === "all_but") return { kind, list };
    return null;
  }

  /**
   * @param {unknown} value
   * @returns {Regionless | null}
   */
  function regionless(value) {
    if (!isObject(value)) return null;
    const kind = value["kind"];
    if (kind === "yes" || kind === "no") return { kind };
    if (kind !== "zone") return null;
    const zones = value["zones"];
    if (!isObject(zones)) return null;
    const prefixes = strings(zones["prefixes"]);
    const exact = strings(zones["exact"]);
    return prefixes === null || exact === null ? null : { kind, zones: { prefixes, exact } };
  }

  /**
   * @param {unknown} value
   * @returns {Language | null}
   */
  function row(value) {
    if (!isObject(value)) return null;
    const tag = value["tag"];
    const answers = value["answers"];
    if (typeof tag !== "string" || !isObject(answers)) return null;
    const lang = answers["lang"];
    const scripts = strings(answers["scripts"]);
    const r = regions(answers["regions"]);
    const rl = regionless(answers["regionless"]);
    const standsInFor = strings(answers["stands_in_for"]);
    if (typeof lang !== "string" || scripts === null || r === null || rl === null || standsInFor === null) {
      return null;
    }
    return { tag, lang, scripts, regions: r, regionless: rl, standsInFor };
  }

  /**
   * @param {unknown} value the parsed JSON `{"languages": [...]}`
   * @returns {Language[]}
   */
  function table(value) {
    if (!isObject(value) || !Array.isArray(value["languages"])) return [];
    /** @type {Language[]} */
    const out = [];
    for (const item of /** @type {unknown[]} */ (value["languages"])) {
      const r = row(item);
      if (r !== null) out.push(r);
    }
    return out;
  }

  /**
   * @param {string} raw
   * @returns {Pref | null}
   */
  function parsePref(raw) {
    const tag = raw.trim();
    if (tag === "" || tag.length > 35) return null;
    const parts = tag.split(/[-_]/);
    const lang = (parts[0] ?? "").toLowerCase();
    if (!/^[a-z]{2,3}$/.test(lang)) return null;
    /** @type {string | null} */
    let script = null;
    /** @type {string | null} */
    let region = null;
    for (const part of parts.slice(1)) {
      if (script === null && /^[A-Za-z]{4}$/.test(part)) {
        script = part.charAt(0).toUpperCase() + part.slice(1).toLowerCase();
        continue;
      }
      if (/^([A-Za-z]{2}|\d{3})$/.test(part)) region = part.toUpperCase();
      break;
    }
    return { lang, script, region };
  }

  /**
   * @param {Pref} p
   * @returns {string}
   */
  const prefTag = (p) => [p.lang, p.script, p.region].filter((s) => s !== null).join("-");

  /**
   * @param {Language} r
   * @param {Pref} pref
   * @param {string | null} zone
   * @returns {number | null}
   */
  function score(r, pref, zone) {
    if (r.lang !== pref.lang) return null;
    let s = 0;
    if (pref.script !== null) {
      const asked = pref.script.toLowerCase();
      if (r.scripts.some((x) => x.toLowerCase() === asked)) s += 8;
      else if (r.scripts.length > 0) s -= 8;
    }
    if (pref.region !== null) {
      const g = r.regions;
      if (g.kind === "any") s += 1;
      else if (g.kind === "only") s += g.list.includes(pref.region) ? 4 : -4;
      else s += g.list.includes(pref.region) ? -4 : 1;
    } else {
      const g = r.regionless;
      if (g.kind === "yes") s += 1;
      else if (g.kind === "no") s -= 4;
      else if (zone !== null && (g.zones.prefixes.some((p) => zone.startsWith(p)) || g.zones.exact.includes(zone))) {
        s += 2;
      }
    }
    return s;
  }

  /**
   * Which of the `shipped` rows answers these preferences (most wanted
   * first), by flash_core::locale::negotiate's rules; null when none does.
   * The table's first row is English, the root.
   * @param {readonly string[]} tags
   * @param {string | null} zone
   * @param {unknown} rawTable the parsed `{"languages": [...]}`
   * @param {readonly string[]} shipped the tags the page carries words in
   * @returns {Negotiated | null}
   */
  function negotiate(tags, zone, rawTable, shipped) {
    const every = table(rawTable);
    const rows = every.filter((r) => shipped.includes(r.tag));
    const root = every[0]?.tag ?? null;
    /** @type {Negotiated | null} */
    let standIn = null;
    for (const raw of tags) {
      const pref = parsePref(raw);
      if (pref === null) continue;
      /** @type {{ s: number, tag: string } | null} */
      let found = null;
      for (const r of rows) {
        const s = score(r, pref, zone);
        if (s !== null && (found === null || s > found.s)) found = { s, tag: r.tag };
      }
      if (found !== null) {
        if (standIn !== null && found.tag === root) continue;
        return { locale: found.tag, formatTag: prefTag(pref) };
      }
      if (standIn === null) {
        const r = rows.find((x) => x.standsInFor.includes(pref.lang));
        if (r !== undefined) standIn = { locale: r.tag, formatTag: prefTag(pref) };
      }
    }
    return standIn;
  }

  window.flashNegotiate = negotiate;
})();
