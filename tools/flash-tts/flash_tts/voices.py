"""The voice table.

A voice id is `<lang><gender>_<name>`: the first letter is the language
code the engine's pipeline is keyed on, the second is `f` or `m`. The
server keeps the same table in Rust and warns at boot when the two drift,
so a voice added here must be added there too.

The engine also ships Spanish, French, Hindi, Italian and Portuguese
voices, and none is offered: those languages have no dictionary of their
own in the engine (espeak-ng phonemizes them) and were trained on next to
nothing, so they come out garbled or with an English accent. The three
languages here read as themselves.
"""

from __future__ import annotations

# Language code -> label, in the order the settings picker groups them.
LANGUAGES: dict[str, str] = {
    "a": "American English",
    "b": "British English",
    "j": "Japanese",
    "z": "Mandarin",
}

VOICES: tuple[str, ...] = (
    # American English
    "af_alloy", "af_aoede", "af_bella", "af_heart", "af_jessica", "af_kore",
    "af_nicole", "af_nova", "af_river", "af_sarah", "af_sky",
    "am_adam", "am_echo", "am_eric", "am_fenrir", "am_liam", "am_michael",
    "am_onyx", "am_puck", "am_santa",
    # British English
    "bf_alice", "bf_emma", "bf_isabella", "bf_lily",
    "bm_daniel", "bm_fable", "bm_george", "bm_lewis",
    # Japanese
    "jf_alpha", "jf_gongitsune", "jf_nezumi", "jf_tebukuro", "jm_kumo",
    # Mandarin
    "zf_xiaobei", "zf_xiaoni", "zf_xiaoxiao", "zf_xiaoyi",
    "zm_yunjian", "zm_yunxi", "zm_yunxia", "zm_yunyang",
)

DEFAULT_VOICE = "af_heart"


def lang_of(voice: str) -> str:
    """The pipeline language code of a voice id (its first letter)."""
    return voice[:1]


def is_voice(voice: str) -> bool:
    return voice in VOICES


def display_name(voice: str) -> str:
    """`bm_fable` -> `Fable`."""
    return voice.split("_", 1)[1].capitalize()


def listing() -> list[dict[str, str]]:
    """The `/voices` payload: every voice with its language code."""
    return [{"id": v, "lang": lang_of(v)} for v in VOICES]
