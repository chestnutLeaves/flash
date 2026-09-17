import re

from flash_tts.voices import (
    DEFAULT_VOICE,
    LANGUAGES,
    VOICES,
    display_name,
    is_voice,
    lang_of,
    listing,
)

EXPECTED_PER_LANGUAGE = {
    "a": 20,
    "b": 8,
    "j": 5,
    "z": 8,
}


def test_forty_one_unique_voices():
    assert len(VOICES) == 41
    assert len(set(VOICES)) == 41


def test_every_id_has_the_expected_shape():
    for v in VOICES:
        assert re.fullmatch(r"[abjz][fm]_[a-z]+", v), v


def test_the_espeak_languages_are_not_offered():
    for v in ("ef_dora", "em_santa", "ff_siwis", "hf_alpha", "if_sara", "pm_alex"):
        assert not is_voice(v), v


def test_every_language_is_known_and_counts_match():
    counts: dict[str, int] = {}
    for v in VOICES:
        counts[lang_of(v)] = counts.get(lang_of(v), 0) + 1
    assert counts == EXPECTED_PER_LANGUAGE
    assert set(counts) == set(LANGUAGES)


def test_default_voice_is_in_the_table():
    assert DEFAULT_VOICE == "af_heart"
    assert is_voice(DEFAULT_VOICE)
    assert not is_voice("af_nobody")
    assert not is_voice("")


def test_display_name_is_the_capitalised_second_part():
    assert display_name("bm_fable") == "Fable"
    assert display_name("zf_xiaoxiao") == "Xiaoxiao"
    assert display_name("af_heart") == "Heart"


def test_listing_shape():
    rows = listing()
    assert len(rows) == 41
    assert rows[0] == {"id": "af_alloy", "lang": "a"}
    assert {"id": "zm_yunyang", "lang": "z"} in rows
    assert all(set(r) == {"id", "lang"} for r in rows)
