// The two sound cues of voice study, rendered from their recipes.
//
//   cue-tick.wav   a bell tap as the microphone opens: a 700 Hz sine with
//                  one inharmonic partial, touched rather than rung, 120 ms
//   cue-done.wav   a marimba pair as a rating posts: two wood taps a fifth
//                  apart, 90 ms between them, 160 ms
//
// Every client plays these same files, so the sound is one sound
// everywhere. To change a cue, edit its recipe here and run
//
//   node tools/cues/make-cues.mjs
//
// which overwrites both files under crates/flash-server/static. The noise
// comes from a fixed-seed generator and the math has no dependencies, so
// the output is the same on every machine.

import { writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const SAMPLE_RATE = 24000;
const PEAK = 0.3;
const TAU = Math.PI * 2;

const OUT_DIR = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'crates', 'flash-server', 'static');

// A small linear congruential generator: the same seed gives the same
// noise, so a render is reproducible.
function rng(seed) {
  let s = seed >>> 0;
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return (s / 4294967296) * 2 - 1;
  };
}

function alloc(seconds) {
  return new Float32Array(Math.ceil(seconds * SAMPLE_RATE));
}

// One struck tone added into `buf` from sample `start`: a sine at `f` with
// a linear attack and an exponential decay, an optional partial that dies
// faster than the fundamental, and an optional low-passed noise transient
// over the first `noiseMs`.
function strike(buf, start, f, len, o) {
  const n = Math.min(buf.length - start, Math.ceil(len * SAMPLE_RATE));
  const tau = len / o.decay;
  const noise = rng(o.seed || 7);
  let lp = 0;
  for (let i = 0; i < n; i++) {
    const t = i / SAMPLE_RATE;
    const env = Math.min(1, t / o.attack) * Math.exp(-t / tau);
    let s = Math.sin(TAU * f * t) * env;
    if (o.partial) s += Math.sin(TAU * f * o.partial * t) * env * o.partialGain * Math.exp(-t / (tau * 0.6));
    if (o.noiseMs && t < o.noiseMs / 1000) {
      lp += 0.35 * (noise() - lp);
      s += lp * o.noiseGain;
    }
    buf[start + i] += s;
  }
}

// Peak-normalise to PEAK and fade the last 3 ms so a decay that has not
// reached silence does not end on a click.
function finish(buf) {
  let peak = 0;
  for (const v of buf) peak = Math.max(peak, Math.abs(v));
  const g = peak > 0 ? PEAK / peak : 0;
  const tail = Math.min(buf.length, Math.round(0.003 * SAMPLE_RATE));
  for (let i = 0; i < buf.length; i++) {
    let v = buf[i] * g;
    if (i >= buf.length - tail) v *= (buf.length - i) / tail;
    buf[i] = v;
  }
  return buf;
}

function bellTap() {
  const len = 0.12;
  const b = alloc(len);
  strike(b, 0, 700, len, { attack: 0.003, decay: 5, partial: 2.76, partialGain: 0.4 });
  return finish(b);
}

function marimbaPair() {
  const gap = 0.09;
  const len = 0.07;
  const b = alloc(gap + len);
  const tap = { attack: 0.005, decay: 6, noiseMs: 1.5, noiseGain: 0.5 };
  strike(b, 0, 440, len, { ...tap, seed: 5 });
  strike(b, Math.round(gap * SAMPLE_RATE), 660, len, { ...tap, seed: 9 });
  return finish(b);
}

// Mono 16-bit PCM with the plain 44-byte header.
function wav(samples) {
  const data = Buffer.alloc(samples.length * 2);
  for (let i = 0; i < samples.length; i++) {
    const v = Math.max(-1, Math.min(1, samples[i]));
    data.writeInt16LE(Math.round(v * 32767), i * 2);
  }
  const header = Buffer.alloc(44);
  header.write('RIFF', 0);
  header.writeUInt32LE(36 + data.length, 4);
  header.write('WAVE', 8);
  header.write('fmt ', 12);
  header.writeUInt32LE(16, 16);
  header.writeUInt16LE(1, 20);
  header.writeUInt16LE(1, 22);
  header.writeUInt32LE(SAMPLE_RATE, 24);
  header.writeUInt32LE(SAMPLE_RATE * 2, 28);
  header.writeUInt16LE(2, 32);
  header.writeUInt16LE(16, 34);
  header.write('data', 36);
  header.writeUInt32LE(data.length, 40);
  return Buffer.concat([header, data]);
}

export const CUES = { 'cue-tick.wav': bellTap, 'cue-done.wav': marimbaPair };

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  for (const [name, render] of Object.entries(CUES)) {
    const samples = render();
    const path = join(OUT_DIR, name);
    writeFileSync(path, wav(samples));
    console.log(`${name}: ${Math.round((samples.length / SAMPLE_RATE) * 1000)} ms`);
  }
}
