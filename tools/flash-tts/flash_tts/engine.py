"""The synthesis engine: Kokoro-82M on the CPU, one clip at a time.

One `KModel` is shared by every language; a `KPipeline` (the language's
grapheme-to-phoneme front end plus the voice packs it has loaded) is built
lazily per language code and kept. Synthesis holds a single lock: the
model is not thread-safe and a second concurrent call on a one-core box
would only make both slower.
"""

from __future__ import annotations

import logging
import os
import subprocess
import tempfile
import threading
from importlib import metadata

import numpy as np

log = logging.getLogger("flash_tts.engine")

REPO_ID = "hexgrad/Kokoro-82M"
SAMPLE_RATE = 24_000
SPEED = 1.0
LOCK_WAIT_SECONDS = 30.0
WARM_LANG = "a"

# One line per language for `prefetch`, enough to make each front end
# load (and, at image build time, download) what it needs.
PREFETCH_TEXT = {
    "a": "Hello.",
    "b": "Hello.",
    "j": "こんにちは。",
    "z": "你好。",
}

# Encoding: MP4/AAC mono 24 kHz 48 kbps with the moov atom first, the one
# container that iOS, Android and every browser decode. `ffmpeg` is a
# system package; the PCM goes in through stdin and the file comes back
# through a temporary path because faststart needs a seekable output.
FFMPEG = os.environ.get("FLASH_TTS_FFMPEG", "ffmpeg")
AAC_BITRATE = "48k"
MIME = "audio/mp4"


def model_tag() -> str:
    """`kokoro-82m-v<package version>`: the server keys clips on it, so a
    new engine release re-synthesizes lazily instead of serving old audio
    under a new voice."""
    try:
        return "kokoro-82m-v" + metadata.version("kokoro")
    except metadata.PackageNotFoundError:
        return "kokoro-82m"


class SynthesisError(RuntimeError):
    """The engine produced no audio, or the encoder failed."""


class Busy(RuntimeError):
    """The synthesis lock was not free within `LOCK_WAIT_SECONDS`."""


class Engine:
    def __init__(self, repo_id: str = REPO_ID) -> None:
        import torch
        from kokoro import KModel

        torch.set_num_threads(1)
        self.repo_id = repo_id
        self.model = KModel(repo_id=repo_id).eval()
        self.sample_rate = SAMPLE_RATE
        self.model_tag = model_tag()
        self._pipelines: dict[str, object] = {}
        self._pipelines_lock = threading.Lock()
        self._lock = threading.Lock()

    # -- pipelines ---------------------------------------------------------

    def pipeline(self, lang: str):
        with self._pipelines_lock:
            p = self._pipelines.get(lang)
            if p is None:
                from kokoro import KPipeline

                log.info("building pipeline for language %r", lang)
                p = KPipeline(lang_code=lang, model=self.model, repo_id=self.repo_id)
                self._pipelines[lang] = p
            return p

    def warm(self, lang: str = WARM_LANG, voice: str | None = None) -> None:
        """Build a pipeline (and load a voice pack) before the first request
        so the first clip does not pay for it."""
        p = self.pipeline(lang)
        if voice:
            p.load_voice(voice)

    def prefetch(self, voices: tuple[str, ...]) -> None:
        """Download everything an offline runtime (`HF_HUB_OFFLINE=1`, a
        read-only filesystem) will need: every voice pack into the cache,
        and one short clip per language, since some front ends fetch their
        dictionaries the first time they see text (the Japanese one into
        its own package directory)."""
        p = self.pipeline(WARM_LANG)
        for v in voices:
            p.load_voice(v)
        first_voice = {}
        for v in voices:
            first_voice.setdefault(v[:1], v)
        for lang, voice in sorted(first_voice.items()):
            pcm = self._synthesize(PREFETCH_TEXT.get(lang, "Hello."), voice)
            log.info("prefetch %s (%s): %d samples", lang, voice, pcm.size)

    # -- synthesis ---------------------------------------------------------

    @property
    def busy(self) -> bool:
        return self._lock.locked()

    def synthesize(self, text: str, voice: str) -> np.ndarray:
        """Float32 mono PCM at `sample_rate` for `text` in `voice`, or raise
        `Busy` / `SynthesisError`."""
        if not self._lock.acquire(timeout=LOCK_WAIT_SECONDS):
            raise Busy()
        try:
            return self._synthesize(text, voice)
        finally:
            self._lock.release()

    def _synthesize(self, text: str, voice: str) -> np.ndarray:
        lang = voice[:1]
        try:
            pipeline = self.pipeline(lang)
        except Exception as exc:  # noqa: BLE001 - a missing front end is a 500, not a crash
            raise SynthesisError(f"no pipeline for language {lang!r}: {exc}") from exc
        chunks: list[np.ndarray] = []
        try:
            for result in pipeline(text, voice=voice, speed=SPEED):
                audio = result.audio
                if audio is not None and len(audio):
                    chunks.append(audio.detach().cpu().numpy().astype(np.float32, copy=False))
        except Exception as exc:  # noqa: BLE001 - reported as a 500 with the cause
            if lang in "jz" and not chunks:
                # Japanese and Mandarin go through the pipeline's own misaki
                # G2P above. If that path fails, phonemize explicitly and
                # hand the phoneme string to the model directly (the route
                # `generate_from_tokens` exposes, the equivalent of calling
                # the pipeline with `is_phonemes=True` on engines that have
                # that flag).
                log.warning("%s pipeline failed for %s (%s); trying explicit phonemes", lang, voice, exc)
                chunks = self._synthesize_from_phonemes(pipeline, text, voice)
            else:
                raise SynthesisError(f"{voice}: {exc}") from exc
        if not chunks:
            raise SynthesisError(f"{voice}: the engine produced no audio")
        return np.concatenate(chunks) if len(chunks) > 1 else chunks[0]

    @staticmethod
    def _synthesize_from_phonemes(pipeline, text: str, voice: str) -> list[np.ndarray]:
        phonemes, _ = pipeline.g2p(text)
        if not phonemes:
            raise SynthesisError(f"{voice}: no phonemes for the text")
        chunks: list[np.ndarray] = []
        for result in pipeline.generate_from_tokens(tokens=phonemes, voice=voice, speed=SPEED):
            audio = result.audio
            if audio is not None and len(audio):
                chunks.append(audio.detach().cpu().numpy().astype(np.float32, copy=False))
        return chunks


# -- encoding ---------------------------------------------------------------


def encode_m4a(samples: np.ndarray, sample_rate: int = SAMPLE_RATE) -> bytes:
    """AAC in an MP4 container, moov atom first, from float32 PCM."""
    pcm = np.ascontiguousarray(np.asarray(samples, dtype=np.float32).reshape(-1)).tobytes()
    with tempfile.TemporaryDirectory(prefix="flash-tts-") as tmp:
        out = os.path.join(tmp, "clip.m4a")
        cmd = [
            FFMPEG, "-hide_banner", "-loglevel", "error", "-nostdin", "-y",
            "-f", "f32le", "-ar", str(sample_rate), "-ac", "1", "-i", "pipe:0",
            "-c:a", "aac", "-b:a", AAC_BITRATE, "-ar", str(sample_rate), "-ac", "1",
            "-movflags", "+faststart", "-f", "mp4", out,
        ]
        try:
            proc = subprocess.run(cmd, input=pcm, capture_output=True, timeout=120, check=False)
        except FileNotFoundError as exc:
            raise SynthesisError(f"ffmpeg not found ({FFMPEG})") from exc
        except subprocess.TimeoutExpired as exc:
            raise SynthesisError("ffmpeg timed out") from exc
        if proc.returncode != 0:
            err = proc.stderr.decode("utf-8", "replace").strip()
            raise SynthesisError(f"ffmpeg failed ({proc.returncode}): {err[:400]}")
        with open(out, "rb") as f:
            data = f.read()
    if len(data) < 8 or data[4:8] != b"ftyp":
        raise SynthesisError("ffmpeg output is not an MP4 (no ftyp box)")
    return data
