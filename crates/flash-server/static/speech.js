// One speaker for the study page: a clip in the chosen voice when the
// server has one, the browser's own voice otherwise, and a loudness
// reading of whatever is playing (or of the microphone) for the glow.
// External file: the CSP forbids inline script.
//
// `window.flashSpeech`:
//   unlock()               call on a gesture; opens the audio graph
//   say({text, url})       plays the clip, or speaks the text; a promise
//                          that settles when it ends or is cut
//   stop()                 cuts whatever is playing
//   speaking()             whether anything is playing right now
//   setRate(r)             playback rate for clips and the browser voice
//   attachMic(stream)      meters a microphone stream
//   detachMic()
//   level("in"|"out")      0..1 loudness, smoothed by the caller
//   cue(name) / setCues()  the earcons
//   phrases(voiceId)       the fixed phrase clips for a voice, cached
//
// `window.flashSpeak(text)` stays as a shim for callers that only know
// the old name.
(function () {
  var AudioCtx = window.AudioContext || window.webkitAudioContext;
  var synth = window.speechSynthesis;
  var hasSynth = !!synth && typeof SpeechSynthesisUtterance !== "undefined";

  var ctx = null;
  var audio = null;
  var routed = false;
  var outAnalyser = null;
  var inAnalyser = null;
  var inSource = null;
  var pendingMic = null;
  var buf = null;

  var rate = 1;
  var cuesOn = true;
  var cueClips = {};
  var phraseCache = {};

  // The one utterance in flight: settled by its end, or by the next one.
  var current = null;
  var synthPulseAt = 0;

  function clamp(v, lo, hi) { return Math.min(hi, Math.max(lo, v)); }

  function player() {
    if (audio) return audio;
    audio = new Audio();
    audio.preload = "auto";
    return audio;
  }

  function applyRate(el) {
    el.playbackRate = rate;
    if ("preservesPitch" in el) el.preservesPitch = true;
    if ("webkitPreservesPitch" in el) el.webkitPreservesPitch = true;
    if ("mozPreservesPitch" in el) el.mozPreservesPitch = true;
  }

  // Routes the player through an analyser so `level("out")` has a
  // reading. Only once: a media element can be attached to one graph.
  function route() {
    if (routed || !ctx) return;
    try {
      var el = player();
      var src = ctx.createMediaElementSource(el);
      outAnalyser = ctx.createAnalyser();
      outAnalyser.fftSize = 512;
      outAnalyser.smoothingTimeConstant = 0.4;
      src.connect(outAnalyser);
      outAnalyser.connect(ctx.destination);
      routed = true;
    } catch (e) {
      // Left unrouted: playback still works, the glow just rests.
    }
  }

  function unlock() {
    if (!ctx && AudioCtx) {
      try { ctx = new AudioCtx(); } catch (e) { ctx = null; }
    }
    if (ctx && ctx.state === "suspended") ctx.resume().catch(function () {});
    route();
    if (pendingMic) { var s = pendingMic; pendingMic = null; attachMic(s); }
    player();
  }

  function settle(done) {
    var c = current;
    if (!c) return;
    current = null;
    c.resolve(done);
  }

  function speakText(text) {
    return new Promise(function (resolve) {
      if (!hasSynth || !text) { resolve(true); return; }
      synth.cancel();
      var u = new SpeechSynthesisUtterance(text);
      u.rate = rate;
      var mine = { resolve: resolve, kind: "synth", utterance: u };
      current = mine;
      u.onend = function () { if (current === mine) settle(true); };
      u.onerror = function () { if (current === mine) settle(false); };
      u.onboundary = function () { synthPulseAt = performance.now(); };
      synthPulseAt = performance.now();
      synth.speak(u);
    });
  }

  function playClip(url, fallbackText, allowFallback) {
    return new Promise(function (resolve) {
      var el = player();
      var mine = { resolve: resolve, kind: "clip" };
      current = mine;
      var fell = false;
      function fallback() {
        if (fell || current !== mine) return;
        fell = true;
        current = null;
        cleanup();
        if (allowFallback && fallbackText) speakText(fallbackText).then(resolve);
        else resolve(false);
      }
      function finish() {
        if (current !== mine) return;
        cleanup();
        settle(true);
      }
      function cleanup() {
        el.removeEventListener("ended", finish);
        el.removeEventListener("error", fallback);
      }
      mine.cleanup = cleanup;
      el.addEventListener("ended", finish);
      el.addEventListener("error", fallback);
      try {
        el.src = url;
        applyRate(el);
        var p = el.play();
        if (p && p.catch) p.catch(fallback);
      } catch (e) {
        fallback();
      }
    });
  }

  // Cuts whatever is playing; its promise settles at once.
  function stop() {
    var c = current;
    current = null;
    if (hasSynth) synth.cancel();
    if (audio) {
      try { audio.pause(); } catch (e) { /* nothing playing */ }
      if (c && c.cleanup) c.cleanup();
    }
    if (c) c.resolve(false);
  }

  // `utterance` is { text, url, fallback } or a string. With `fallback`
  // false (the server has a natural voice) the browser's own never
  // reads: a missing clip goes unsaid, and one that fails to play is
  // simply over. Otherwise the text is read when there is no clip or
  // it fails.
  function say(utterance) {
    stop();
    var u = typeof utterance === "string" ? { text: utterance } : (utterance || {});
    var allowFallback = u.fallback !== false;
    if (u.url) return playClip(u.url, u.text || "", allowFallback);
    if (!u.text || !allowFallback) return Promise.resolve(true);
    return speakText(u.text);
  }

  function speaking() {
    if (current) return true;
    return hasSynth && synth.speaking;
  }

  function setRate(r) {
    rate = clamp(parseFloat(r) || 1, 0.5, 2);
    if (audio) applyRate(audio);
  }

  function attachMic(stream) {
    if (!stream) return;
    if (!ctx) { pendingMic = stream; return; }
    detachMic();
    try {
      inSource = ctx.createMediaStreamSource(stream);
      inAnalyser = ctx.createAnalyser();
      inAnalyser.fftSize = 512;
      inAnalyser.smoothingTimeConstant = 0.5;
      inSource.connect(inAnalyser);
    } catch (e) {
      inSource = null;
      inAnalyser = null;
    }
  }

  function detachMic() {
    pendingMic = null;
    if (inSource) { try { inSource.disconnect(); } catch (e) { /* gone */ } }
    inSource = null;
    inAnalyser = null;
  }

  function rms(analyser) {
    if (!analyser) return 0;
    if (!buf || buf.length !== analyser.fftSize) buf = new Float32Array(analyser.fftSize);
    analyser.getFloatTimeDomainData(buf);
    var sum = 0;
    for (var i = 0; i < buf.length; i++) sum += buf[i] * buf[i];
    return Math.sqrt(sum / buf.length);
  }

  // 0..1. Speech sits around 0.05..0.3 RMS, so it is lifted to fill the
  // range; the browser voice has no signal to read, so its words pulse.
  function level(which) {
    if (which === "in") return clamp(rms(inAnalyser) * 4, 0, 1);
    if (current && current.kind === "synth") {
      var age = performance.now() - synthPulseAt;
      return clamp(0.8 - age / 450, 0.2, 0.8);
    }
    if (!current || !audio || audio.paused) return 0;
    return clamp(rms(outAnalyser) * 3.5, 0, 1);
  }

  function cue(name) {
    if (!cuesOn) return;
    var el = cueClips[name];
    if (!el) {
      el = new Audio("/static/cue-" + name + ".wav");
      el.preload = "auto";
      cueClips[name] = el;
    }
    try {
      el.currentTime = 0;
      var p = el.play();
      if (p && p.catch) p.catch(function () {});
    } catch (e) { /* no gesture yet */ }
  }

  function setCues(on) { cuesOn = !!on; }

  // { key: { url, duration_ms, envelope } | null } for a voice, or {}
  // when the server has no natural voice. `fresh` skips the cache (a
  // phrase that was still being made).
  function phrases(voiceId, fresh) {
    if (!voiceId) return Promise.resolve({});
    if (!fresh && phraseCache[voiceId]) return phraseCache[voiceId];
    var p = fetch("/voice/phrases?voice=" + encodeURIComponent(voiceId), {
      credentials: "same-origin",
      headers: { Accept: "application/json" }
    }).then(function (res) {
      if (!res.ok) return {};
      return res.json().then(function (json) { return (json && json.phrases) || {}; });
    }).catch(function () { return {}; });
    phraseCache[voiceId] = p;
    return p;
  }

  window.flashSpeech = {
    unlock: unlock,
    say: say,
    stop: stop,
    speaking: speaking,
    setRate: setRate,
    attachMic: attachMic,
    detachMic: detachMic,
    level: level,
    cue: cue,
    setCues: setCues,
    phrases: phrases,
    canSpeak: function () { return hasSynth; }
  };
  window.flashSpeak = function (text) { return say({ text: text }); };

  // ---- the listen button on each row of a voice picker ----
  // Plays the voice's own sample line. The server makes every sample
  // ahead of time and the row carries its address; a row without one
  // (the warm-up is still running) asks for the voice's phrases and
  // waits a little for the clip. One sample plays at a time; pressing
  // the playing row stops it.
  var SAMPLE_WAIT_MS = 3000;
  var SAMPLE_ATTEMPTS = 8;
  var activeSample = null;

  function setSampleState(btn, state) {
    var name = btn.getAttribute("data-voice-name") || "this voice";
    if (state) btn.setAttribute("data-state", state);
    else btn.removeAttribute("data-state");
    var label = state === "playing" ? "Stop " + name
      : state === "preparing" ? "Getting " + name + " ready"
      : "Play " + name;
    btn.setAttribute("aria-label", label);
    btn.title = label;
  }

  function stopSample() {
    var was = activeSample;
    activeSample = null;
    if (was) setSampleState(was, null);
    stop();
  }

  // The sample's address: from the row, or from the phrase set once the
  // clip exists. Null when it is still not made after a few tries or
  // the row was released meanwhile.
  function sampleUrl(btn, voiceId) {
    var known = btn.getAttribute("data-sample-url");
    if (known) return Promise.resolve(known);
    var attempt = 0;
    function next(fresh) {
      return phrases(voiceId, fresh).then(function (map) {
        var sample = map && map.sample;
        if (sample && sample.url) {
          btn.setAttribute("data-sample-url", sample.url);
          return sample.url;
        }
        attempt += 1;
        if (attempt >= SAMPLE_ATTEMPTS || activeSample !== btn) return null;
        return new Promise(function (resolve) { setTimeout(resolve, SAMPLE_WAIT_MS); })
          .then(function () { return next(true); });
      });
    }
    return next(false);
  }

  document.addEventListener("click", function (e) {
    var btn = e.target.closest("[data-voice-sample]");
    if (!btn) return;
    e.preventDefault();
    unlock();
    if (activeSample === btn) { stopSample(); return; }
    stopSample();
    var voiceId = btn.getAttribute("data-voice-sample");
    if (!voiceId) return;
    activeSample = btn;
    setSampleState(btn, "preparing");
    sampleUrl(btn, voiceId).then(function (url) {
      if (activeSample !== btn) return;
      if (!url) { stopSample(); return; }
      setSampleState(btn, "playing");
      return say({ url: url, fallback: false }).then(function () {
        if (activeSample !== btn) return;
        activeSample = null;
        setSampleState(btn, null);
      });
    });
  });

  // Scrolls a voice list so its picked row sits in the middle, without
  // moving the page. Called at load for the settings card and by the
  // study page when its sheet opens (a closed dialog has no layout).
  function revealPicked(list) {
    if (!list) return;
    var picked = list.querySelector("input:checked");
    var row = picked && picked.closest(".voice-row");
    if (!row) return;
    list.scrollTop = Math.max(0, row.offsetTop - (list.clientHeight - row.offsetHeight) / 2);
  }
  window.flashSpeech.revealPicked = revealPicked;
  document.querySelectorAll(".voice-list").forEach(revealPicked);
})();
