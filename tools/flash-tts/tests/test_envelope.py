import numpy as np

from flash_tts.envelope import ENVELOPE_HZ, envelope, window_count

SR = 24_000
WINDOW = SR // ENVELOPE_HZ  # 1200 samples = 50 ms


def test_empty_audio_has_no_envelope():
    assert envelope(np.zeros(0, dtype=np.float32), SR) == b""
    assert window_count(0, SR) == 0


def test_one_byte_per_window():
    for seconds in (0.05, 0.5, 1.0, 2.87, 4.0):
        n = int(seconds * SR)
        env = envelope(np.random.default_rng(1).standard_normal(n).astype(np.float32), SR)
        assert len(env) == window_count(n, SR) == round(n * 1000 / SR / 50)


def test_count_matches_duration_ms_at_20_hz():
    # duration_ms / 50 rounded half up, and never zero for a non-empty clip.
    assert window_count(1, SR) == 1
    assert window_count(WINDOW // 2 - 1, SR) == 1
    assert window_count(WINDOW * 3, SR) == 3
    assert window_count(WINDOW * 3 + WINDOW // 2, SR) == 4
    assert window_count(SR * 4, SR) == 80


def test_loudest_window_is_255_and_silence_is_0():
    x = np.zeros(WINDOW * 4, dtype=np.float32)
    x[WINDOW : 2 * WINDOW] = 0.5
    x[3 * WINDOW :] = 0.25
    env = envelope(x, SR)
    assert len(env) == 4
    assert env[0] == 0
    assert env[1] == 255
    assert env[2] == 0
    # (0.25 / 0.5) ** 0.6 * 255 = 168.2
    assert env[3] == 168


def test_all_silence_is_all_zero_bytes():
    assert envelope(np.zeros(WINDOW * 5, dtype=np.float32), SR) == bytes(5)


def test_curve_lifts_quiet_speech():
    x = np.zeros(WINDOW * 2, dtype=np.float32)
    x[:WINDOW] = 1.0
    x[WINDOW:] = 0.1
    env = envelope(x, SR)
    # Linear would give 25; the 0.6 curve gives 0.1 ** 0.6 * 255 = 64.
    assert env[1] == 64


def test_accepts_any_shape_and_dtype():
    x = np.full((WINDOW, 1), 0.3, dtype=np.float64)
    assert envelope(x, SR) == b"\xff"


def test_tail_shorter_than_half_a_window_joins_the_last_window():
    x = np.zeros(WINDOW * 2 + 100, dtype=np.float32)
    x[-100:] = 1.0
    env = envelope(x, SR)
    assert len(env) == 2
    assert env[1] == 255 and env[0] == 0
