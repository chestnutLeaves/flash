// Voice study on the web: read the card, listen, grade, move on.
// External file: the CSP forbids inline script.
//
// Progressive: nothing here runs unless the page says the server grades
// (#study carries data-voice-input), the browser can speak at all, and
// it has a speech recognizer; without one the toggle stays hidden. The
// page keeps working untouched under the loop, and the keyboard still
// works: a manual reveal or grade simply swaps the card under the loop,
// which then picks up what is on screen.
//
// The loop mirrors the app's: read the front, listen. A keyword (a grade
// word, undo, repeat, skip, pause, yes, no) acts at once. Anything else
// goes to the server as one spoken turn with where the conversation is
// (the transcript, or the recording when the server asks for audio), and
// what comes back is acted on the same way: an answer reveals and is
// graded, a question is answered aloud, a request to hear the card again
// reads it again, and a word the server could not use is asked for once
// more. The transcript sent is the recognizer's best guess, unless a
// lower one carries the answer's own words (the card's
// data-answer-hints). The verdict is played and the remark shown, and
// the rating lands after a 3 s undo window (or a "Good?" confirm). The
// microphone is always
// closed before anything is spoken, so the page never grades its own
// voice. When the microphone is refused or the recognizer gives up, the
// session says so and turns itself off: it never grades without input.
//
// Speech goes through speech.js (a clip in the chosen voice when the
// server made one, the browser's voice otherwise) and the glow through
// glow.js; the preferences come from #study's data attributes and live
// in the settings sheet behind the gear.
(function () {
  var Recognition = window.SpeechRecognition || window.webkitSpeechRecognition;
  var speech = window.flashSpeech;
  var glow = window.flashGlow;
  var study = document.getElementById("study");
  var toggle = document.getElementById("voice-toggle");
  var dock = document.getElementById("voice-dock");
  if (!speech || !study || !toggle || !dock) return;
  if (!study.dataset.voiceInput) return;

  var END_OF_SPEECH_MS = 1500;
  var RESTART_MS = 300;
  var MAX_ALTERNATIVES = 5;
  var UNDO_WINDOW_MS = 3000;
  var MAX_RESTARTS = 6;
  var NOTICE_MS = 5000;
  var CLIP_MAX_BYTES = 1500000;
  var CLIP_TYPES = ["audio/webm;codecs=opus", "audio/mp4", "audio/ogg;codecs=opus"];

  // The fixed phrases: what is said when no clip in the chosen voice
  // exists yet, keyed like the server's phrase set.
  var PHRASE_TEXT = {
    correct: "Correct.",
    close: "Close.",
    not_quite: "Not quite.",
    again_q: "Again?",
    hard_q: "Hard?",
    good_q: "Good?",
    easy_q: "Easy?",
    rate_prompt: "Rate it: again, hard, good, or easy.",
    rate_again: "Say again, hard, good, or easy.",
    session_complete: "Session complete.",
    grading_busy: "Grading is busy right now.",
    grading_down: "Grading isn't available right now.",
    unclear: "Sorry, I didn't catch that."
  };
  var VERDICT_KEY = { 1: "not_quite", 2: "close", 3: "correct", 4: "correct" };
  var CONFIRM_KEY = { 1: "again_q", 2: "hard_q", 3: "good_q", 4: "easy_q" };
  var RATING_WORDS = { 1: "Again", 2: "Hard", 3: "Good", 4: "Easy" };

  var OFFLINE = "Can't reach Flash.";
  // What a refusal without a sentence of its own reads as.
  var ALLOWANCE_USED = "This month's AI allowance is used up, so voice grading stopped.";
  var MIC_COPY = "Voice study needs the microphone. Allow it in the browser and try again.";
  var STT_COPY = "Speech recognition isn't available right now, so voice study stopped.";

  // What the empty transcript line says while the microphone is open.
  var PLACEHOLDER = {
    front: "Say your answer, or a grade: again, hard, good, easy.",
    window: "Say undo to change it, or a different grade.",
    confirm: "Yes or no?",
    rate: "Again, hard, good, or easy?",
    paused: "Paused. Tap the microphone to continue."
  };

  // ---- what an utterance means (the same parser the mobile app carries) ----

  // The dialog's keywords: the words its prompts say out loud, matched
  // whole. Said alone they act at once, need no model, and are the whole
  // vocabulary when there is none. Anything else is what the student
  // said, for the server to read in any wording.
  var GRADES = {
    1: ["again", "one", "1"],
    2: ["hard", "two", "2"],
    3: ["good", "three", "3"],
    4: ["easy", "four", "4"]
  };
  var KEYWORDS = {
    skip: ["skip"],
    undo: ["undo"],
    repeat: ["repeat"],
    pause: ["pause", "stop"],
    yes: ["yes"],
    no: ["no"]
  };

  function normalize(text) {
    return text
      .toLowerCase()
      .replace(/[’']/g, "")
      .replace(/[^\p{L}\p{N}\s]/gu, " ")
      .replace(/\s+/g, " ")
      .trim();
  }

  // normalize, then runs of single letters joined: "a u" becomes "au".
  function normalizeAnswer(text) {
    var out = [];
    var run = "";
    var words = normalize(text).split(" ");
    for (var i = 0; i < words.length; i++) {
      var word = words[i];
      if (word.length === 1) { run += word; continue; }
      if (run) out.push(run);
      run = "";
      if (word) out.push(word);
    }
    if (run) out.push(run);
    return out.join(" ");
  }

  // The alternative to send as the answer: the first whose normalized
  // form is, or contains as whole words, one of the card's hints
  // normalized the same way; the top one otherwise. The top one is tried
  // first, so the pick only moves when the recognizer's best guess has
  // none of the answer's words. "hey you" for "A U" is not caught here;
  // the grader's prompt covers that.
  function pickTranscript(alternatives, hints) {
    var top = alternatives[0] || "";
    var wanted = [];
    for (var i = 0; i < hints.length; i++) {
      var h = normalizeAnswer(hints[i]);
      // A single letter would match almost anything.
      if (h.length >= 2 && wanted.indexOf(h) < 0) wanted.push(h);
    }
    if (!wanted.length) return top;
    for (var a = 0; a < alternatives.length; a++) {
      var norm = " " + normalizeAnswer(alternatives[a]) + " ";
      for (var w = 0; w < wanted.length; w++) {
        if (norm.indexOf(" " + wanted[w] + " ") >= 0) return alternatives[a];
      }
    }
    return top;
  }

  // Null for silence; a keyword's command; otherwise what was said, as
  // it was said, for the server.
  function parse(text) {
    var trimmed = text.trim();
    if (!trimmed) return null;
    var norm = normalize(trimmed);
    if (!norm) return null;
    for (var rating = 1; rating <= 4; rating++) {
      if (GRADES[rating].indexOf(norm) !== -1) return { kind: "grade", rating: rating };
    }
    for (var kind in KEYWORDS) {
      if (KEYWORDS[kind].indexOf(norm) !== -1) return { kind: kind };
    }
    return { kind: "answer", text: trimmed };
  }

  // The server's reading of a turn, as a command for the same switch the
  // keywords go through.
  function intentCommand(json) {
    switch (json.intent) {
      case "answer": return { kind: "graded", rating: json.rating, feedback: json.feedback };
      case "question": return { kind: "question", answer: json.answer, spoken: json.spoken_answer, spokenUrl: json.spoken_answer_url || null };
      case "grade": return { kind: "grade", rating: json.rating };
      case "repeat":
      case "skip":
      case "undo":
      case "unclear": return { kind: json.intent };
      default: return { kind: "unclear" };
    }
  }

  // ---- the preferences ----

  var prefs = {
    voiceId: study.dataset.voiceId || "",
    rate: parseFloat(study.dataset.speechRate) || 1,
    apply: study.dataset.voiceApply === "confirm" ? "confirm" : "auto",
    cues: study.dataset.voiceCues !== "off",
    // The server has a natural voice: card clips and phrase clips exist.
    audio: study.dataset.voiceAudio === "1"
  };
  var wantsAudio = study.dataset.voiceInput === "audio";

  // No recognizer, or nothing to speak with: the toggle stays hidden.
  if (!Recognition) return;
  if (!speech.canSpeak() && !prefs.audio) return;

  var phrases = {};
  function loadPhrases(voiceId) {
    if (!prefs.audio || !voiceId) { phrases = {}; return; }
    speech.phrases(voiceId).then(function (map) {
      if (voiceId === prefs.voiceId) phrases = map || {};
    });
  }
  function phrase(key) {
    var p = phrases[key];
    return { text: PHRASE_TEXT[key] || "", url: p && p.url ? p.url : null, key: key };
  }
  // A card side: the clip the page was rendered with, and where to ask
  // for it when that is stale or missing.
  function side(s, which) {
    return {
      text: which === "front" ? s.front : s.back,
      url: which === "front" ? s.audioFront : s.audioBack,
      probe: s.id ? "/study/cards/" + s.id + "/audio?side=" + which : null
    };
  }
  // The answer to a question: fresh text, with a clip the server is
  // making when it has a voice.
  function answer(command) {
    return { text: command.spoken, url: null, probe: command.spokenUrl };
  }

  function applyPrefs() {
    speech.setRate(prefs.rate);
    speech.setCues(prefs.cues);
  }

  // ---- the page ----

  var phaseEl = document.getElementById("voice-phase");
  var transcriptEl = document.getElementById("voice-transcript");
  var verdictLine = document.getElementById("voice-verdict-line");
  var verdictEl = document.getElementById("voice-verdict");
  var remarkEl = document.getElementById("voice-remark");
  var undoBtn = document.getElementById("voice-undo");
  var noticeEl = document.getElementById("voice-notice");
  var micBtn = document.getElementById("voice-mic");
  var repeatBtn = document.getElementById("voice-repeat");
  var settingsBtn = document.getElementById("voice-settings");
  var stopBtn = document.getElementById("voice-stop");
  var dialog = document.getElementById("voice-settings-dialog");
  var form = document.getElementById("voice-settings-form");
  var noteEl = document.getElementById("voice-settings-note");

  function card() { return study.querySelector(".study-card"); }
  function revealBtn() { return document.getElementById("reveal-btn"); }
  function gradeBtn(rating) { return document.getElementById("grade-" + rating); }
  // The answer's words for the recognizer, from data-answer-hints.
  function parseHints(json) {
    if (!json) return [];
    try {
      var parsed = JSON.parse(json);
      return Array.isArray(parsed) ? parsed : [];
    } catch (e) {
      return [];
    }
  }
  function state() {
    var c = card();
    var d = c ? c.dataset : {};
    return {
      id: c ? d.cardId : null,
      front: d.spokenFront || "",
      back: d.spokenBack || "",
      hints: parseHints(d.answerHints),
      audioFront: d.audioFront || null,
      audioBack: d.audioBack || null,
      revealed: !!gradeBtn(1),
      done: !!study.querySelector(".study-done")
    };
  }

  // Phase name: what the live region says, and how the glow moves.
  var PHASES = {
    starting: ["Starting", "breathe"],
    reading: ["Reading", "speak"],
    listening: ["Listening", "listen"],
    thinking: ["Thinking", "think"],
    grading: ["Grading", "think"],
    speaking: ["Speaking", "speak"],
    window: ["Listening", "listen"],
    confirming: ["Confirming", "listen"],
    paused: ["Paused", "still"],
    done: ["Done", "speak"]
  };
  function setPhase(name) {
    var p = PHASES[name] || PHASES.starting;
    dock.dataset.phase = name;
    phaseEl.textContent = p[0];
    if (glow) glow.mode(p[1]);
    if (name !== "listening" && name !== "window" && name !== "confirming" && name !== "paused") {
      transcriptEl.dataset.placeholder = "";
    }
  }
  function placeholder(text) { transcriptEl.dataset.placeholder = text || ""; }
  function showTranscript(text) { transcriptEl.textContent = text || ""; }
  function showVerdict(rating, remark) {
    verdictEl.textContent = RATING_WORDS[rating] ? RATING_WORDS[rating] + "." : "";
    remarkEl.textContent = remark || "";
    verdictLine.hidden = !verdictEl.textContent && !remarkEl.textContent;
  }
  function clearVerdict() {
    verdictEl.textContent = "";
    remarkEl.textContent = "";
    verdictLine.hidden = true;
  }
  var noticeTimer = null;
  function notice(text) {
    noticeEl.textContent = text;
    noticeEl.hidden = false;
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(function () { noticeEl.hidden = true; }, NOTICE_MS);
  }
  function markPending(rating) {
    for (var i = 1; i <= 4; i++) {
      var btn = gradeBtn(i);
      if (btn) btn.classList.toggle("is-pending", i === rating);
    }
    undoBtn.hidden = rating === null;
  }
  function showMuted(muted) {
    micBtn.setAttribute("aria-pressed", muted ? "true" : "false");
    micBtn.setAttribute("aria-label", muted ? "Resume" : "Pause");
    micBtn.title = muted ? "Resume (m)" : "Pause (m)";
  }

  // ---- the machine ----

  var m = {
    on: false,
    // Bumped by every cancel; async work compares its copy before continuing.
    gen: 0,
    cardId: null,
    // What the pending reveal is for: { kind: "grade", rating } (a grade
    // word), { kind: "graded", rating, feedback } (the server's verdict,
    // ahead of the reveal), or { kind: "tap" } (read the back and ask).
    awaiting: null,
    pending: null,
    stopListen: null,
    timer: null,
    windowTimer: null,
    // The server has no model, or the user grades themselves: the back
    // is read and a grade word asked for instead of calling the grader.
    graderOff: study.dataset.gradingMode === "self",
    stream: null,
    micDenied: false,
    // "user" (the button, m, "pause"), "dialog" (the settings sheet is up).
    pausedBy: null,
    // The voice changed since the card was rendered: its clip URLs name
    // the old voice's clips until the next swap renders it again.
    staleClips: false,
    // The settings save in flight, which the next clip waits behind.
    saving: null,
    // When a wait last ran out; later waits are short until a clip comes.
    voiceStalledAt: 0
  };

  function live(g) { return m.on && g === m.gen && !m.pausedBy; }

  function clearTimer() { clearTimeout(m.timer); m.timer = null; }
  function clearWindow() { clearTimeout(m.windowTimer); m.windowTimer = null; }
  function stopListening() {
    if (m.stopListen) m.stopListen();
    m.stopListen = null;
  }

  // Abandons whatever is in flight: timers, the microphone, the speaker.
  function cancel() {
    clearTimer();
    stopListening();
    speech.stop();
    m.gen += 1;
    return m.gen;
  }

  // Speaks with the microphone closed; true when the step is still
  // current afterwards. `utterance` is { text, url, probe, key }: `url`
  // a clip known to exist, `probe` where to ask for one until it does,
  // `key` a phrase to ask the voice's set for again. With a natural
  // voice on the server nothing is ever read by the browser's own: an
  // utterance waits for its clip, and goes unsaid when none comes.
  function speak(utterance, phase, g) {
    if (!live(g)) return Promise.resolve(false);
    stopListening();
    setPhase(phase);
    return awaitUtterance(utterance, g)
      .then(function (u) {
        if (prefs.audio) u.fallback = false;
        return speech.say(u);
      })
      .then(function () { return live(g); });
  }

  // How long an utterance waits for its clip. The server queued it
  // first in line (the card on screen at the request that showed it or
  // at a voice change, the answer at the turn), so it is usually
  // seconds away; once a wait has run out, the next ones are short
  // until a clip arrives, so a voice service that is down costs a few
  // seconds a turn, not half a minute.
  var CLIP_WAIT_MS = 30000;
  var STALLED_WAIT_MS = 5000;
  var CLIP_POLL_MS = 1000;
  var READY_HINT_MS = 2000;
  var WAITING_COPY = "Getting the voice ready";
  var UNVOICED_COPY = "That one is too long to read aloud.";
  var STALLED_COPY = "The voice isn't ready. Continuing without it.";

  function delay(ms) {
    return new Promise(function (resolve) { setTimeout(resolve, ms); });
  }

  // Resolves with the utterance, its `url` set when its clip is there.
  function awaitUtterance(u, g) {
    if (!prefs.audio) return Promise.resolve(u);
    if (u.url && !m.staleClips) return Promise.resolve(u);
    if (!u.probe && !u.key) { u.url = null; return Promise.resolve(u); }
    u.url = null;
    var started = Date.now();
    var budget = m.voiceStalledAt ? STALLED_WAIT_MS : CLIP_WAIT_MS;
    var hinted = false;
    function done(found) {
      if (hinted) placeholder("");
      if (found) m.voiceStalledAt = 0;
      return u;
    }
    // "ready" with the url, "none" when no clip will come, "later" otherwise.
    function ask() {
      if (u.key) {
        return speech.phrases(prefs.voiceId, true).then(function (map) {
          if (!live(g)) return "later";
          phrases = map || {};
          var p = phrases[u.key];
          if (p && p.url) { u.url = p.url; return "ready"; }
          return "later";
        }, function () { return "later"; });
      }
      return fetch(u.probe, { credentials: "same-origin" }).then(function (r) {
        if (r.status === 200) { u.url = u.probe; return "ready"; }
        if (r.status === 204) return "none";
        return "later";
      }, function () { return "later"; });
    }
    function attempt() {
      if (!live(g)) return done(false);
      return ask().then(function (answer) {
        if (!live(g)) return done(false);
        if (answer === "ready") return done(true);
        if (answer === "none") {
          if (u.text) notice(UNVOICED_COPY);
          return done(false);
        }
        var waited = Date.now() - started;
        if (waited >= budget) {
          m.voiceStalledAt = Date.now();
          notice(STALLED_COPY);
          return done(false);
        }
        if (waited >= READY_HINT_MS && !hinted) {
          hinted = true;
          placeholder(WAITING_COPY);
        }
        return delay(CLIP_POLL_MS).then(attempt);
      });
    }
    return (m.saving || Promise.resolve()).then(attempt);
  }

  // Listening broke: say so and stop. The notice lives outside the dock,
  // so it stays readable after the dock hides; nothing is graded without
  // an ear.
  function fail(message) {
    notice(message);
    off();
  }

  // ---- the microphone stream (the glow's level, and the recording) ----

  // Opened whenever the session is on, so the glow has something to
  // breathe with; the recorder uses it too when the server wants audio.
  // A refusal (the prompt declined, or the site blocked) ends the
  // session before anything is read. Any other failure is not fatal on
  // its own: the recognizer asks for the microphone itself, and a
  // refusal there reaches `fail` as a not-allowed error.
  function ensureStream() {
    if (m.stream || m.micDenied) return Promise.resolve();
    if (!navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) {
      m.micDenied = true;
      return Promise.resolve();
    }
    return navigator.mediaDevices.getUserMedia({ audio: true }).then(function (stream) {
      if (!m.on) { stream.getTracks().forEach(function (t) { t.stop(); }); return; }
      m.stream = stream;
      speech.attachMic(stream);
      setTracks(!m.pausedBy);
    }, function (err) {
      if (err && err.name === "NotAllowedError") { fail(MIC_COPY); return; }
      m.micDenied = true;
    });
  }
  function setTracks(on) {
    if (!m.stream) return;
    m.stream.getAudioTracks().forEach(function (t) { t.enabled = on; });
  }
  function dropStream() {
    speech.detachMic();
    if (!m.stream) return;
    m.stream.getTracks().forEach(function (t) { t.stop(); });
    m.stream = null;
  }
  function clipType() {
    if (!window.MediaRecorder) return null;
    for (var i = 0; i < CLIP_TYPES.length; i++) {
      if (MediaRecorder.isTypeSupported(CLIP_TYPES[i])) return CLIP_TYPES[i];
    }
    return null;
  }

  // One recorder per listening turn. `finish` hands back { mime, b64 }
  // or null when there is nothing usable; a null clip means the
  // transcript goes instead, never a failed turn.
  function recorder(record) {
    var type = record && m.stream && wantsAudio ? clipType() : null;
    if (!type) return { restart: function () {}, finish: function (cb) { cb(null); } };
    var rec = null;
    var chunks = [];
    function start() {
      chunks = [];
      try {
        rec = new MediaRecorder(m.stream, { mimeType: type });
        rec.ondataavailable = function (e) { if (e.data && e.data.size) chunks.push(e.data); };
        rec.onerror = function () { rec = null; };
        rec.start();
      } catch (e) {
        rec = null;
      }
    }
    start();
    return {
      // Nothing was heard yet: begin the clip again so silence is not kept.
      restart: function () {
        if (rec && rec.state !== "inactive") { rec.onstop = null; try { rec.stop(); } catch (e) { /* gone */ } }
        start();
      },
      finish: function (cb) {
        if (!rec || rec.state === "inactive") { cb(null); return; }
        var mime = rec.mimeType || type;
        rec.onstop = function () {
          var blob = new Blob(chunks, { type: mime });
          if (!blob.size || blob.size > CLIP_MAX_BYTES) { cb(null); return; }
          var reader = new FileReader();
          reader.onload = function () {
            var url = String(reader.result);
            cb({ mime: mime, b64: url.slice(url.indexOf(",") + 1) });
          };
          reader.onerror = function () { cb(null); };
          reader.readAsDataURL(blob);
        };
        try { rec.stop(); } catch (e) { cb(null); }
      }
    };
  }

  // ---- listening (the same recognizer wrapper the mobile app carries) ----

  // One utterance per call: partials to the dock, a single final
  // transcript when the speaker stops (the recognizer's own final, or
  // 1.5 s after the last partial). The engine's own timeouts end the
  // stream now and then; while nothing has been heard it is restarted.
  function listen(record, onFinal, onError) {
    var rec = null;
    var active = true;
    var delivered = false;
    var latest = "";
    var latestAlts = [];
    var restarts = 0;
    var silence = null;
    var restartTimer = null;
    var clip = recorder(record);

    function deliver() {
      if (!active || delivered || !latest.trim()) return;
      delivered = true;
      clearTimeout(silence);
      try { rec.stop(); } catch (e) { /* already stopped */ }
      var text = latest.trim();
      var alts = latestAlts.length ? latestAlts : [text];
      clip.finish(function (audio) {
        if (!active) return;
        active = false;
        onFinal(text, audio, alts);
      });
    }

    function fatal(message) {
      if (!active) return;
      active = false;
      clearTimeout(silence);
      clearTimeout(restartTimer);
      clip.finish(function () {});
      onError(message);
    }

    function restart() {
      if (!active || delivered || restartTimer) return;
      if (restarts >= MAX_RESTARTS) { fatal(STT_COPY); return; }
      restarts += 1;
      restartTimer = setTimeout(function () {
        restartTimer = null;
        if (!latest) clip.restart();
        start();
      }, RESTART_MS);
    }

    function start() {
      if (!active) return;
      rec = new Recognition();
      rec.lang = navigator.language || "en-US";
      rec.interimResults = true;
      rec.continuous = true;
      rec.maxAlternatives = MAX_ALTERNATIVES;
      rec.onresult = function (e) {
        if (!active || delivered) return;
        // Every transcript the recognizer offers, best first: the k-th
        // alternative is each result's k-th guess joined, a result with
        // fewer guesses lending its best. Only a final result carries
        // more than one.
        var width = 0;
        var final = false;
        for (var i = 0; i < e.results.length; i++) {
          if (e.results[i].length > width) width = e.results[i].length;
          if (e.results[i].isFinal) final = true;
        }
        var alts = [];
        for (var k = 0; k < width; k++) {
          var text = "";
          for (var j = 0; j < e.results.length; j++) {
            var r = e.results[j];
            text += (r[k] || r[0]).transcript;
          }
          if (text.trim() && alts.indexOf(text.trim()) < 0) alts.push(text.trim());
        }
        if (!alts.length) return;
        latest = alts[0];
        latestAlts = alts;
        showTranscript(latest);
        if (final) { deliver(); return; }
        clearTimeout(silence);
        silence = setTimeout(deliver, END_OF_SPEECH_MS);
      };
      rec.onspeechend = function () { if (latest.trim()) deliver(); };
      rec.onend = function () { if (active && !delivered) restart(); };
      rec.onerror = function (e) {
        if (!active) return;
        var code = e.error;
        if (code === "aborted") return;
        if (code === "not-allowed" || code === "service-not-allowed" || code === "audio-capture") {
          fatal(MIC_COPY);
          return;
        }
        if (code === "network" || code === "language-not-supported") {
          fatal(STT_COPY);
          return;
        }
        // no-speech and the rest are followed by onend, which restarts.
      };
      try {
        rec.start();
      } catch (e) {
        fatal(STT_COPY);
      }
    }

    start();

    return function stop() {
      if (!active) return;
      active = false;
      clearTimeout(silence);
      clearTimeout(restartTimer);
      clip.finish(function () {});
      try { rec.abort(); } catch (e) { /* gone */ }
    };
  }

  // Opens the microphone for one utterance, a beat after the speaker
  // stops. Never while paused: a pause closes the microphone, and only
  // the button opens it again.
  function listenFor(g, context) {
    if (!live(g)) return;
    stopListening();
    setPhase(context === "window" ? "window" : context === "confirm" ? "confirming" : "listening");
    placeholder(PLACEHOLDER[context]);
    clearTimer();
    m.timer = setTimeout(function () {
      m.timer = null;
      if (g !== m.gen) return;
      if (context === "front") speech.cue("tick");
      m.stopListen = listen(context === "front", function (top, clip, alts) {
        if (g !== m.gen) return;
        stopListening();
        // Of the recognizer's guesses, the one carrying the answer's words.
        var text = context === "front" ? pickTranscript(alts, state().hints) : top;
        showTranscript(text);
        var command = parse(text);
        if (command) onUtterance(context, command, clip, g);
        else listenFor(g, context);
      }, function (message) {
        if (g !== m.gen) return;
        fail(message);
      });
    }, RESTART_MS);
  }

  // ---- the turns ----

  function runCard() {
    var s = state();
    if (!m.on || !s.front || m.pausedBy) return;
    clearWindow();
    var g = cancel();
    showTranscript("");
    speak(side(s, "front"), "reading", g).then(function (ok) {
      if (ok) listenFor(g, "front");
    });
  }

  // Reveals the card for `awaiting`: what to do once the back is on screen.
  function reveal(g, awaiting) {
    var btn = revealBtn();
    if (!live(g) || !btn) return;
    m.awaiting = awaiting;
    setPhase("thinking");
    btn.click();
  }

  // The session and card the study card's own buttons post with.
  function vals() {
    var btn = revealBtn() || gradeBtn(1);
    try { return JSON.parse((btn && btn.getAttribute("hx-vals")) || "{}"); } catch (e) { return {}; }
  }

  // One spoken turn to the server: where the conversation is, and what
  // was said as the transcript or the recording.
  function postTurn(state, text, clip) {
    var v = vals();
    var body = new URLSearchParams();
    body.set("session", v.session);
    body.set("card", v.card);
    body.set("state", state);
    if (clip) {
      body.set("audio_mime", clip.mime);
      body.set("audio_b64", clip.b64);
    } else {
      body.set("transcript", text);
    }
    return fetch("/study/turn", {
      method: "POST",
      credentials: "same-origin",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: body.toString()
    }).then(function (res) {
      if (!res.ok) {
        // A refusal carries its sentence as the body; anything else
        // says nothing worth reading.
        return res.text().then(function (text) {
          return { status: res.status, json: null, text: text };
        });
      }
      return res.json().then(function (json) { return { status: res.status, json: json }; });
    });
  }

  function applyRating(rating) {
    clearWindow();
    cancel();
    m.pending = null;
    markPending(null);
    var btn = gradeBtn(rating);
    if (btn) { setPhase("thinking"); btn.click(); }
  }
  function applyPending() { if (m.pending !== null) applyRating(m.pending); }

  // The rating is on the table: confirm mode asks, otherwise it lands
  // after the undo window unless something else is said or tapped.
  function openWindow(g, rating) {
    m.pending = rating;
    markPending(rating);
    if (prefs.apply === "confirm") {
      speak(phrase(CONFIRM_KEY[rating]), "speaking", g).then(function (ok) {
        if (ok) listenFor(g, "confirm");
      });
      return;
    }
    clearWindow();
    m.windowTimer = setTimeout(function () { if (g === m.gen) applyPending(); }, UNDO_WINDOW_MS);
    listenFor(g, "window");
  }

  function ratePrompt(g, key) {
    m.pending = null;
    markPending(null);
    speak(phrase(key || "rate_prompt"), "speaking", g).then(function (ok) {
      if (ok) listenFor(g, "rate");
    });
  }

  // No grader: read the answer and ask for a grade.
  function rateYourself(g) {
    var s = state();
    if (!s.back) return;
    speak(side(s, "back"), "speaking", g).then(function (ok) {
      if (ok) ratePrompt(g);
    });
  }

  function runGradeWord(g, rating) {
    speak(side(state(), "back"), "speaking", g).then(function (ok) {
      if (ok) openWindow(g, rating);
    });
  }

  // The server graded the answer ahead of the reveal: the remark is
  // shown, never spoken; the verdict is a fixed phrase in the chosen
  // voice; then the window.
  function runGraded(g, awaiting) {
    showVerdict(awaiting.rating, awaiting.feedback);
    speak(phrase(VERDICT_KEY[awaiting.rating] || "correct"), "speaking", g).then(function (ok) {
      if (ok) openWindow(g, awaiting.rating);
    });
  }

  // Says something, then re-opens the window for the pending rating.
  function sayAndReopen(g, utterance) {
    var rating = m.pending;
    clearWindow();
    speak(utterance, "speaking", g).then(function (ok) {
      if (ok && rating !== null) openWindow(g, rating);
    });
  }

  // Says something, then listens on the front again.
  function sayAndListen(g, utterance) {
    speak(utterance, "speaking", g).then(function (ok) {
      if (ok) listenFor(g, "front");
    });
  }

  // What was said is not a keyword: the server reads it, where the
  // conversation is, and its reading goes through `act` like a keyword.
  // Without a grader (self-grading, or a server that said it has none)
  // it is taken as the answer on the front, or as a word the prompt did
  // not ask for.
  function runTurn(g, context, text, clip) {
    var asks = context === "front" || context === "window" || context === "confirm";
    var plain = { kind: "answer", text: text };
    if (m.graderOff || !asks) { act(context, plain, clip, g); return; }
    var state = context === "front" ? "front" : "window";
    clearWindow();
    setPhase("grading");
    postTurn(state, text, clip).then(function (res) {
      if (!live(g)) return;
      if (res.status === 404) {
        m.graderOff = true;
        notice(PHRASE_TEXT.grading_down);
        act(context, plain, clip, g);
        return;
      }
      if (res.status === 403) {
        // The plan said no (the month's allowance is used): the server's
        // sentence names the day it resumes, and the mode stops.
        fail(res.text || ALLOWANCE_USED);
        return;
      }
      if (!res.json) {
        speak(phrase(res.status === 429 ? "grading_busy" : "grading_down"), "speaking", g).then(function (ok) {
          if (ok) act(context, plain, clip, g);
        });
        return;
      }
      act(context, intentCommand(res.json), clip, g);
    }, function () {
      if (!live(g)) return;
      speak(OFFLINE, "speaking", g).then(function (ok) {
        if (ok) act(context, plain, clip, g);
      });
    });
  }

  function onUtterance(context, command, clip, g) {
    // On the front a true/false card's "yes" or "no" is the answer.
    var word = context === "front" && (command.kind === "yes" || command.kind === "no");
    if (command.kind === "answer") runTurn(g, context, command.text, clip);
    else if (word) runTurn(g, context, command.kind, clip);
    else act(context, command, clip, g);
  }

  // A keyword, or the server's reading of what was said, applied where
  // the conversation is.
  function act(context, command, clip, g) {
    switch (context) {
      case "front":
        switch (command.kind) {
          case "grade": reveal(g, { kind: "grade", rating: command.rating }); return;
          case "skip": reveal(g, { kind: "grade", rating: 1 }); return;
          case "repeat": runCard(); return;
          case "pause": pause("user"); return;
          case "graded": reveal(g, { kind: "graded", rating: command.rating, feedback: command.feedback }); return;
          // No grader: the answer reveals, and the back is read for a grade.
          case "answer": reveal(g, { kind: "tap" }); return;
          case "question": showVerdict(null, command.answer); sayAndListen(g, answer(command)); return;
          case "unclear": sayAndListen(g, phrase("unclear")); return;
          default: listenFor(g, "front"); return;
        }
      case "window":
      case "confirm":
        switch (command.kind) {
          case "grade": applyRating(command.rating); return;
          case "yes":
          case "skip": applyPending(); return;
          case "undo":
          case "no": clearWindow(); ratePrompt(g); return;
          case "pause": applyPending(); pause("user"); return;
          case "repeat": sayAndReopen(g, side(state(), "back")); return;
          case "question": showVerdict(m.pending, command.answer); sayAndReopen(g, answer(command)); return;
          case "unclear": sayAndReopen(g, phrase("unclear")); return;
          // No grader: a word the prompt did not ask for.
          case "answer": clearWindow(); ratePrompt(g, "rate_again"); return;
          default: listenFor(g, context); return;
        }
      case "rate":
        switch (command.kind) {
          case "grade": applyRating(command.rating); return;
          case "skip": applyRating(1); return;
          case "repeat": rateYourself(g); return;
          case "pause": pause("user"); return;
          default: ratePrompt(g, "rate_again"); return;
        }
    }
  }

  // ---- pausing ----

  // Everything stops, the microphone included: the recognizer is
  // aborted and the stream's tracks disabled, and nothing is heard or
  // shown until the button (or m) opens the microphone again.
  function pause(by) {
    if (m.pausedBy) return;
    clearWindow();
    cancel();
    m.pausedBy = by || "user";
    setPhase("paused");
    showTranscript("");
    placeholder(PLACEHOLDER.paused);
    showMuted(true);
    setTracks(false);
  }

  // Picks up where the card was: the question again, or the answer and
  // its window.
  function resumeCurrent() {
    var s = state();
    if (!m.on) return;
    if (s.done) { off(); return; }
    if (!s.revealed) { runCard(); return; }
    var rating = m.pending;
    var g = cancel();
    if (rating === null) { rateYourself(g); return; }
    speak(side(s, "back"), "speaking", g).then(function (ok) {
      if (ok) openWindow(g, rating);
    });
  }

  function resume() {
    if (!m.pausedBy) return;
    cancel();
    m.pausedBy = null;
    showMuted(false);
    setTracks(true);
    ensureStream().then(function () { if (m.on && !m.pausedBy) resumeCurrent(); });
  }

  // ---- the card changing under the loop ----

  // Every swap of #study: classify what is on screen rather than count
  // swaps, since a reveal and a review both land here, and so does a
  // manual keypress or a tap on the card.
  function onSwap() {
    if (!m.on) return;
    var s = state();
    if (s.done) {
      var g = cancel();
      m.pausedBy = null;
      speak(phrase("session_complete"), "done", g).then(off);
      return;
    }
    if (!s.revealed) {
      // A front is always a fresh turn: the next card, or the same card
      // back after an Again, which the queue returns under its own id.
      m.cardId = s.id;
      m.awaiting = null;
      m.pending = null;
      markPending(null);
      clearVerdict();
      showTranscript("");
      runCard();
      return;
    }
    var awaiting = m.awaiting;
    m.awaiting = null;
    // Paused: the answer waits on screen; resume reads it.
    if (m.pausedBy) return;
    var next = cancel();
    // A reveal the loop did not ask for (space, a tap on the card): the
    // answer is read and a grade asked for.
    if (!awaiting) { rateYourself(next); return; }
    if (awaiting.kind === "grade") runGradeWord(next, awaiting.rating);
    else if (awaiting.kind === "graded") runGraded(next, awaiting);
    else rateYourself(next);
  }

  document.body.addEventListener("htmx:beforeSwap", function (e) {
    if (!m.on || !e.target || e.target.id !== "study") return;
    // The next screen must not be talked or listened over by the last.
    clearTimer();
    clearWindow();
    stopListening();
    speech.stop();
    m.gen += 1;
  });
  document.body.addEventListener("htmx:afterSwap", function (e) {
    if (!e.target || e.target.id !== "study") return;
    // Rendered under the account's voice as it is now.
    m.staleClips = false;
    onSwap();
  });
  // A rating on its way, by voice or by a tap: the soft tone.
  document.addEventListener("click", function (e) {
    if (!m.on) return;
    if (e.target.closest(".grade-row .fl-btn[id^='grade-']")) speech.cue("done");
  }, true);

  // ---- on and off ----

  function on() {
    if (m.on) return;
    speech.unlock();
    m.on = true;
    m.pausedBy = null;
    toggle.setAttribute("aria-pressed", "true");
    document.body.dataset.voice = "on";
    dock.hidden = false;
    if (glow) glow.show();
    clearVerdict();
    showTranscript("");
    markPending(null);
    showMuted(false);
    applyPrefs();
    setPhase("starting");
    loadPhrases(prefs.voiceId);
    ensureStream().then(function () {
      if (!m.on) return;
      var s = state();
      m.cardId = s.id;
      if (s.done) { off(); return; }
      resumeCurrent();
    });
  }

  function off() {
    if (!m.on && dock.hidden) return;
    m.on = false;
    cancel();
    clearWindow();
    dropStream();
    m.awaiting = null;
    m.pending = null;
    m.pausedBy = null;
    markPending(null);
    toggle.setAttribute("aria-pressed", "false");
    delete document.body.dataset.voice;
    dock.hidden = true;
    if (glow) glow.hide();
    if (dialog && dialog.open) dialog.close();
  }

  // ---- the controls ----

  toggle.hidden = false;
  toggle.addEventListener("click", function () { if (m.on) off(); else on(); });
  stopBtn.addEventListener("click", off);
  micBtn.addEventListener("click", function () {
    speech.unlock();
    if (m.pausedBy) resume(); else pause("user");
  });
  repeatBtn.addEventListener("click", function () {
    if (!m.on || m.pausedBy) return;
    if (state().revealed) resumeCurrent(); else runCard();
  });
  undoBtn.addEventListener("click", function () {
    if (!m.on || m.pending === null || m.pausedBy) return;
    var g = cancel();
    clearWindow();
    ratePrompt(g);
  });

  // The settings sheet: the session pauses while it is up and picks up
  // where it was when it closes, under whatever changed.
  if (dialog && form && typeof dialog.showModal === "function") {
    settingsBtn.addEventListener("click", function () {
      if (m.on && !m.pausedBy) pause("dialog");
      dialog.showModal();
      if (speech.revealPicked) speech.revealPicked(form.querySelector(".voice-list"));
    });
    dialog.querySelectorAll("[data-voice-close]").forEach(function (b) {
      b.addEventListener("click", function () { dialog.close(); });
    });
    dialog.addEventListener("close", function () {
      if (m.pausedBy === "dialog") resume();
    });
    var saved = null;
    form.addEventListener("change", function () {
      var was = prefs.voiceId;
      var fd = new FormData(form);
      prefs.voiceId = String(fd.get("voice_id") || prefs.voiceId);
      prefs.rate = parseFloat(fd.get("speech_rate")) || 1;
      prefs.apply = fd.get("voice_apply") === "confirm" ? "confirm" : "auto";
      prefs.cues = fd.get("sound_cues") === "1";
      applyPrefs();
      if (prefs.voiceId !== was) {
        loadPhrases(prefs.voiceId);
        // The card's clips name the old voice, and the server learns
        // the new one only when the form's request lands (htmx sends it
        // a moment after the change): the next clip waits for that.
        m.staleClips = true;
        m.voiceStalledAt = 0;
        if (!m.saving) {
          m.saving = new Promise(function (resolve) { saved = resolve; });
        }
      }
    });
    var noteTimer = null;
    form.addEventListener("htmx:afterRequest", function (e) {
      if (saved) { saved(); saved = null; }
      m.saving = null;
      if (!noteEl) return;
      var ok = e.detail && e.detail.successful;
      noteEl.textContent = ok ? "Saved" : "Couldn't save. Check your connection and try again.";
      noteEl.hidden = false;
      clearTimeout(noteTimer);
      if (ok) noteTimer = setTimeout(function () { noteEl.hidden = true; }, 1800);
    });
  } else {
    settingsBtn.hidden = true;
  }

  document.addEventListener("keydown", function (e) {
    if (e.target.tagName === "INPUT" || e.target.tagName === "TEXTAREA" || e.target.tagName === "SELECT") return;
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if (dialog && dialog.open) return;
    if (e.key === "v" || e.key === "V") {
      e.preventDefault();
      if (m.on) off(); else on();
      return;
    }
    if ((e.key === "m" || e.key === "M") && m.on) {
      e.preventDefault();
      speech.unlock();
      if (m.pausedBy) resume(); else pause("user");
    }
  });
  // Leaving the page must not leave the microphone open.
  window.addEventListener("pagehide", off);
})();
