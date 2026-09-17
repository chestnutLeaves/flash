"""The HTTP surface.

    GET  /health      {ok, model_tag, sample_rate, voices, busy}
    GET  /voices      {voices: [{id, lang}]}
    POST /synthesize  {text, voice} -> {mime, audio_b64, duration_ms,
                      sample_rate, envelope_b64, envelope_hz, model_tag}

400 for an unknown voice, empty text, or text over `MAX_CHARS` once
control characters are stripped; 503 with `Retry-After: 5` when the one
synthesis slot has been held for longer than the lock wait; 500 when the
engine or the encoder fails. Nothing here is authenticated: the sidecar
binds to loopback or a private network, and the Flash server is its only
client.
"""

from __future__ import annotations

import base64
import contextlib
import logging
import time
import unicodedata

from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse
from pydantic import BaseModel

from . import __version__
from .engine import Busy, Engine, MIME, SynthesisError, encode_m4a
from .envelope import ENVELOPE_HZ, envelope
from .voices import VOICES, is_voice, listing

log = logging.getLogger("flash_tts")

MAX_CHARS = 2000
RETRY_AFTER_SECONDS = 5


class SynthesizeRequest(BaseModel):
    text: str
    voice: str


def clean_text(text: str) -> str:
    """Drop control characters (newlines stay: the engine splits on them,
    tabs become spaces) and surrounding whitespace."""
    out: list[str] = []
    for ch in text:
        if ch == "\n":
            out.append(ch)
        elif ch == "\t":
            out.append(" ")
        elif unicodedata.category(ch) == "Cc":
            continue
        else:
            out.append(ch)
    return "".join(out).strip()


def error(status: int, message: str, **headers: str) -> JSONResponse:
    return JSONResponse({"error": message}, status_code=status, headers=headers or None)


def create_app(engine_factory=Engine) -> FastAPI:
    @contextlib.asynccontextmanager
    async def lifespan(app: FastAPI):
        t0 = time.monotonic()
        engine = engine_factory()
        engine.warm()
        app.state.engine = engine
        log.info("engine ready in %.1fs (%s, %d voices)", time.monotonic() - t0, engine.model_tag, len(VOICES))
        yield

    app = FastAPI(title="flash-tts", version=__version__, lifespan=lifespan, docs_url=None, redoc_url=None)

    @app.get("/health")
    def health(request: Request):
        engine: Engine = request.app.state.engine
        return {
            "ok": True,
            "model_tag": engine.model_tag,
            "sample_rate": engine.sample_rate,
            "voices": len(VOICES),
            "busy": engine.busy,
        }

    @app.get("/voices")
    def voices():
        return {"voices": listing()}

    @app.post("/synthesize")
    def synthesize(req: SynthesizeRequest, request: Request):
        if not is_voice(req.voice):
            return error(400, f"unknown voice {req.voice!r}")
        text = clean_text(req.text)
        if not text:
            return error(400, "empty text")
        if len(text) > MAX_CHARS:
            return error(400, f"text is {len(text)} characters; the limit is {MAX_CHARS}")

        engine: Engine = request.app.state.engine
        t0 = time.monotonic()
        try:
            pcm = engine.synthesize(text, req.voice)
            audio = encode_m4a(pcm, engine.sample_rate)
        except Busy:
            return error(503, "synthesis is busy", **{"Retry-After": str(RETRY_AFTER_SECONDS)})
        except SynthesisError as exc:
            log.error("synthesis failed for %s: %s", req.voice, exc)
            return error(500, "synthesis failed")
        duration_ms = int(pcm.size * 1000 / engine.sample_rate + 0.5)
        env = envelope(pcm, engine.sample_rate, ENVELOPE_HZ)
        log.info(
            "%s: %d chars -> %d ms, %d bytes, %.2fs",
            req.voice, len(text), duration_ms, len(audio), time.monotonic() - t0,
        )
        return {
            "mime": MIME,
            "audio_b64": base64.b64encode(audio).decode("ascii"),
            "duration_ms": duration_ms,
            "sample_rate": engine.sample_rate,
            "envelope_b64": base64.b64encode(env).decode("ascii"),
            "envelope_hz": ENVELOPE_HZ,
            "model_tag": engine.model_tag,
        }

    return app
