// The live glow along the bottom of the study page while voice study is
// on: a wash of the theme accent that moves with sound and never changes
// colour. External file: the CSP forbids inline script.
//
// `window.flashGlow`:
//   show() / hide()
//   mode("listen"|"speak"|"think"|"still"|"breathe")
//
// Only `listen` and `speak` run a frame loop, which reads a loudness
// from flashSpeech and writes it to the `--glow` custom property; the
// stylesheet turns that into transform and opacity. `think` and
// `breathe` are CSS keyframes, `still` is static. Under reduced motion
// the loop never starts and the stylesheet holds one static state.
(function () {
  var el = document.getElementById("voice-glow");
  if (!el) return;

  var ATTACK = 0.35;
  var RELEASE = 0.08;
  var FLOOR = 0.15;
  var DEAD_BAND = 0.012;

  var reduced = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  var raf = 0;
  var mode = "still";
  var value = 0;
  var written = -1;
  var frame = 0;

  function write(v) {
    if (Math.abs(v - written) < DEAD_BAND) return;
    written = v;
    el.style.setProperty("--glow", v.toFixed(3));
  }

  function tick() {
    raf = 0;
    if (mode !== "listen" && mode !== "speak") return;
    var speech = window.flashSpeech;
    var target = speech ? speech.level(mode === "listen" ? "in" : "out") : 0;
    value += (target - value) * (target > value ? ATTACK : RELEASE);
    frame += 1;
    if (frame % 2 === 0) write(Math.max(FLOOR, value));
    raf = requestAnimationFrame(tick);
  }

  function stopLoop() {
    if (raf) cancelAnimationFrame(raf);
    raf = 0;
  }

  function setMode(next) {
    mode = next;
    el.dataset.mode = next;
    if (reduced || el.hidden || (next !== "listen" && next !== "speak")) {
      stopLoop();
      value = 0;
      write(next === "still" ? 0 : FLOOR);
      return;
    }
    if (!raf) raf = requestAnimationFrame(tick);
  }

  window.flashGlow = {
    show: function () { el.hidden = false; setMode(mode); },
    hide: function () { el.hidden = true; stopLoop(); write(0); },
    mode: setMode
  };

  window.addEventListener("pagehide", stopLoop);
  document.addEventListener("visibilitychange", function () {
    if (document.hidden) stopLoop();
    else setMode(mode);
  });
})();
