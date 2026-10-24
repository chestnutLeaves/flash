(function () {
  "use strict";

  const tx = (/** @type {string} */ id, /** @type {Record<string, string | number>} */ vars) =>
    window.flashT === undefined ? id : window.flashT(id, vars);

  const { one, all } = window.flashDom;

  const root = document.querySelector("[data-preview]");
  if (root === null) return;
  const cards = all(root, "[data-preview-card]", HTMLElement);
  if (cards.length === 0) return;

  const progress = one(document, "[data-preview-progress]", HTMLElement);
  const bar = one(document, "[data-preview-bar]", HTMLElement);
  const showRow = one(document, "[data-preview-show-row]", HTMLElement);
  const gradeRow = one(document, "[data-preview-grade-row]", HTMLElement);
  const done = one(document, "[data-preview-done]", HTMLElement);
  const score = one(document, "[data-preview-score]", HTMLElement);
  const scoreBar = one(document, "[data-preview-score-bar]", HTMLElement);
  const pct = one(document, "[data-preview-pct]", HTMLElement);
  const missedButton = one(document, "[data-preview-missed]", HTMLButtonElement);
  const backToday = one(document, "[data-preview-back-today]", HTMLAnchorElement);
  const end = one(document, "[data-preview-end]", HTMLButtonElement);

  let round = cards.map((_, i) => i);
  let at = 0;
  let over = false;
  let revealed = false;
  let got = 0;
  /** @type {number[]} */
  let missed = [];

  /** @param {HTMLElement} el @param {boolean} on */
  function show(el, on) {
    el.hidden = !on;
  }

  function backOf(/** @type {HTMLElement} */ card) {
    const back = card.querySelector("[data-preview-back]");
    if (!(back instanceof HTMLElement)) throw new Error("preview.js: a card has no back");
    return back;
  }

  function render() {
    cards.forEach((card, i) => {
      const onScreen = !over && round[at] === i;
      show(card, onScreen);
      show(backOf(card), onScreen && revealed);
    });
    bar.style.width = `${String(Math.round((at * 100) / round.length))}%`;
    show(progress, !over);
    show(showRow, !over && !revealed);
    show(gradeRow, !over && revealed);
    show(end, !over);
    show(done, over);
    if (over) finish();
  }

  function finish() {
    const total = at;
    const percent = Math.round((got * 100) / total);
    score.textContent = tx("js-preview-score", { got, total });
    scoreBar.style.width = `${String(percent)}%`;
    pct.textContent = tx("js-preview-percent", { percent });
    missedButton.hidden = missed.length === 0;
    missedButton.textContent = tx("js-preview-review-missed", { count: missed.length });
    backToday.classList.toggle("fl-btn-primary", missed.length === 0);
    backToday.classList.toggle("fl-btn-secondary", missed.length > 0);
    (missed.length > 0 ? missedButton : backToday).focus();
  }

  /** @param {number[]} next */
  function start(next) {
    round = next;
    at = 0;
    over = false;
    revealed = false;
    got = 0;
    missed = [];
    render();
    window.scrollTo({ top: 0 });
  }

  function reveal() {
    if (over || revealed) return;
    revealed = true;
    render();
  }

  /** @param {boolean} knew */
  function grade(knew) {
    if (over || !revealed) return;
    const card = round[at];
    if (card === undefined) return;
    if (knew) got += 1;
    else missed.push(card);
    at += 1;
    revealed = false;
    over = at >= round.length;
    render();
  }

  function stop() {
    if (at === 0) {
      window.location.assign(backToday.href);
      return;
    }
    over = true;
    revealed = false;
    render();
  }

  function typingText(/** @type {EventTarget | null} */ target) {
    if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) return true;
    return target instanceof HTMLElement && target.isContentEditable;
  }

  document.addEventListener("click", (event) => {
    const target = event.target instanceof Element ? event.target : null;
    if (target === null) return;
    if (target.closest("[data-preview-show]") !== null) reveal();
    const graded = target.closest("[data-preview-grade]");
    if (graded instanceof HTMLElement) grade(graded.dataset["previewGrade"] === "got");
    if (target.closest("[data-preview-end]") !== null) stop();
    if (target.closest("[data-preview-missed]") !== null) start(missed.slice());
    if (target.closest("[data-preview-restart]") !== null) start(cards.map((_, i) => i));
  });

  document.addEventListener("keydown", (event) => {
    if (event.ctrlKey || event.metaKey || event.altKey || typingText(event.target)) return;
    if (event.key === " " && !revealed && !over) {
      event.preventDefault();
      reveal();
    } else if (event.key === "1") grade(false);
    else if (event.key === "2") grade(true);
  });

  render();
})();
