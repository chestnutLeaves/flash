# flash-tts

The natural-voice sidecar. The Flash server points `FLASH_TTS_URL` at it
and, a few cards ahead of the study queue, asks it to turn each card's
spoken text into a short clip; the server stores the clip and the web
app and the phone apps play it. Without the sidecar study falls back to
the device's own voice, so it is optional, and nothing in a request ever
waits on it.

The engine is [Kokoro-82M](https://huggingface.co/hexgrad/Kokoro-82M)
(Apache-2.0), run on the CPU through the `kokoro` package: 54 voices over
nine languages, about 1.4 times realtime on one modern core, 700 to 900 MB
of memory once the weights are loaded. The engine's name stays in here
and in `THIRD_PARTY.md`; to users it is "natural voice".

## Run it

With Docker, from the repository root:

```sh
docker compose --profile voice up -d
```

That builds the image (torch, the weights and every voice pack are baked
in, about 2 GB) and starts it beside `flash` with no published port. Set
`FLASH_TTS_URL=http://flash-tts:8880` in `.env`.

Without Docker, in a virtual environment:

```sh
python -m venv .venv && . .venv/bin/activate     # Windows: .venv\Scripts\activate
pip install torch --index-url https://download.pytorch.org/whl/cpu
pip install tools/flash-tts
python -m flash_tts --bind 127.0.0.1:8880
```

`ffmpeg` must be on PATH (or named in `FLASH_TTS_FFMPEG`), and `espeak-ng`
for the English words the engine's dictionary has never seen. The voices
offered speak American and British English, Japanese and Mandarin; the
engine's Spanish, French, Hindi, Italian and Portuguese voices are left
out, since it phonemizes those languages with espeak-ng and was trained on
too little of them to read them well. The first start downloads
the model and the voices it uses into the Hugging Face cache (`HF_HOME`
to move it); `python -m flash_tts --prefetch` fetches everything at once,
after which `HF_HUB_OFFLINE=1` keeps the service off the network.

## The contract

```
GET  /health      -> {ok, model_tag, sample_rate, voices, busy}
GET  /voices      -> {voices: [{id, lang}]}
POST /synthesize  {text, voice}
                  -> {mime: "audio/mp4", audio_b64, duration_ms, sample_rate,
                      envelope_b64, envelope_hz: 20, model_tag}
```

- `text` is 1 to 2000 characters after control characters are stripped;
  newlines split the text into chunks the engine reads in turn.
- `voice` is one of the ids in `flash_tts/voices.py`. The first letter
  is the language, which picks the pipeline; there is no language
  detection.
- The clip is MP4/AAC, mono, 24 kHz, 48 kbps, moov atom first: about
  24 KB for four seconds, and the one container every target plays.
- The envelope is one byte per 50 ms window: the window's RMS relative
  to the loudest window, raised to 0.6, scaled to 0..255. It has
  `(duration_ms + 25) / 50` bytes in integer arithmetic (the length in
  windows rounded half up, never zero for a non-empty clip), and byte
  `i` covers `i * 50` to `(i + 1) * 50` ms. The glow on the study
  screen moves with it.
- `model_tag` is `kokoro-82m-v<package version>`. The server keys stored
  clips on it, so upgrading the engine re-synthesizes lazily.
- Errors: 400 for an unknown voice, empty or over-long text; 503 with
  `Retry-After: 5` when the single synthesis slot has been busy for
  30 seconds; 500 when the engine or the encoder fails. One clip is made
  at a time; a second request waits for the slot.

The service has no authentication. Bind it to loopback or a private
network and let only the Flash server reach it.

## Tests

```sh
pip install pytest
python -m pytest tools/flash-tts/tests
```

The tests cover the envelope and the voice table and need neither the
model nor `ffmpeg`.
