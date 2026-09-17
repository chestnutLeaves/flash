"""`python -m flash_tts --bind 127.0.0.1:8880`

Environment the libraries read: `HF_HOME` (where the weights are cached)
and `HF_HUB_OFFLINE=1` (never touch the network; everything must already
be in the cache). `FLASH_TTS_FFMPEG` names the encoder binary when it is
not on PATH.
"""

from __future__ import annotations

import argparse
import logging
import sys


def parse_bind(value: str) -> tuple[str, int]:
    host, sep, port = value.rpartition(":")
    if not sep or not host or not port.isdigit():
        raise argparse.ArgumentTypeError(f"expected host:port, got {value!r}")
    return host.strip("[]"), int(port)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="flash_tts", description="The natural-voice sidecar for Flash.")
    parser.add_argument("--bind", type=parse_bind, default=("127.0.0.1", 8880), help="host:port to listen on (default 127.0.0.1:8880)")
    parser.add_argument(
        "--prefetch",
        action="store_true",
        help="download the model and every voice pack into the cache, then exit (an image build step)",
    )
    parser.add_argument("--log-level", default="info")
    args = parser.parse_args(argv)

    logging.basicConfig(level=args.log_level.upper(), format="%(asctime)s %(levelname)s %(name)s: %(message)s")
    # The hub client logs every request at INFO; one line per clip is
    # the journal we want.
    for noisy in ("httpx", "httpcore", "huggingface_hub", "filelock"):
        logging.getLogger(noisy).setLevel(logging.WARNING)

    if args.prefetch:
        from .engine import Engine
        from .voices import VOICES

        engine = Engine()
        engine.prefetch(VOICES)
        print(f"prefetched {engine.model_tag} and {len(VOICES)} voices")
        return 0

    import uvicorn

    from .app import create_app

    host, port = args.bind
    uvicorn.run(create_app(), host=host, port=port, log_level=args.log_level, workers=1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
