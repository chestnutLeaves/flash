// Study-view keyboard shortcuts: space reveals, 1-4 grades. In the
// typed-answer input, Enter reveals (and other shortcuts stay inert so
// typing "1" doesn't grade).
document.addEventListener("keydown", function (e) {
  if (e.target && e.target.id === "type-input") {
    if (e.key === "Enter") {
      e.preventDefault();
      var btn = document.getElementById("reveal-btn");
      if (btn) btn.click();
    }
    return;
  }
  if (e.target.tagName === "INPUT" || e.target.tagName === "TEXTAREA") return;
  var el = null;
  if (e.key === " ") el = document.getElementById("reveal-btn");
  if (e.key >= "1" && e.key <= "4") el = document.getElementById("grade-" + e.key);
  if (el) { e.preventDefault(); el.click(); }
});

// Read the card aloud: the clip in the chosen voice when the server has
// made one (data-audio-front / data-audio-back on the card), the
// browser's own voice otherwise. The text is the server's own spoken
// form of the card (cloze blanks said as "blank", images and formulae
// named rather than skipped), carried on the card element. Playback
// lives in speech.js; this file only picks what to read.
(function () {
  var speech = window.flashSpeech;
  var study = document.getElementById("study");
  if (!speech) return;
  if (!speech.canSpeak() && !(study && study.dataset.voiceAudio)) return;

  function utterance() {
    var card = document.querySelector(".study-card");
    if (!card) return null;
    var d = card.dataset;
    // Once the answer is on screen it is what the reader wants read;
    // before that, the question.
    if (d.spokenBack) return { text: d.spokenBack, url: d.audioBack || null };
    if (d.spokenFront) return { text: d.spokenFront, url: d.audioFront || null };
    return null;
  }

  function speak() {
    var u = utterance();
    if (!u) return;
    // A second press stops rather than queues.
    if (speech.speaking()) {
      speech.stop();
      return;
    }
    speech.unlock();
    speech.say(u);
  }

  function show(root) {
    if (!root || !root.querySelectorAll) return;
    root.querySelectorAll("#speak-btn").forEach(function (btn) {
      if (btn.dataset.speakInit) return;
      btn.dataset.speakInit = "1";
      btn.hidden = false;
      btn.addEventListener("click", speak);
    });
  }

  document.addEventListener("keydown", function (e) {
    if (e.key !== "r" && e.key !== "R") return;
    if (e.target.tagName === "INPUT" || e.target.tagName === "TEXTAREA") return;
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    e.preventDefault();
    speak();
  });

  // The next card must not be read over by the last one's answer.
  document.addEventListener("htmx:beforeSwap", function (e) {
    if (e.target && e.target.id === "study") speech.stop();
  });

  document.addEventListener("DOMContentLoaded", function () {
    show(document);
    document.body.addEventListener("htmx:afterSwap", function (e) { show(e.target); });
  });
})();

// Tap the card to reveal while voice study is on (voice.js marks the
// body): the question is the whole card until the answer is up, and a
// tap is quicker than the button. Links, players and a text selection
// inside the card keep their own meaning.
document.addEventListener("click", function (e) {
  if (document.body.dataset.voice !== "on") return;
  if (!e.target.closest(".study-card .front")) return;
  if (e.target.closest("a,button,audio,video,input,summary,.fl-audio")) return;
  var sel = window.getSelection ? window.getSelection() : null;
  if (sel && sel.toString()) return;
  var btn = document.getElementById("reveal-btn");
  if (btn) btn.click();
});
