"""A coarse loudness envelope of a clip, for the glow that moves with speech.

One byte per 50 ms window (20 Hz): the window's RMS relative to the loudest
window in the clip, raised to 0.6 so quiet speech still moves the glow,
scaled to 0..255. Pure numpy, no engine needed.
"""

from __future__ import annotations

import numpy as np

ENVELOPE_HZ = 20
CURVE = 0.6


def window_count(n_samples: int, sample_rate: int, hz: int = ENVELOPE_HZ) -> int:
    """How many windows a clip of `n_samples` gets: its length in windows,
    rounded half up, so a clip of `d` milliseconds has `round(d * hz / 1000)`
    bytes. Empty audio has none."""
    if n_samples <= 0:
        return 0
    return max(1, int(n_samples * hz / sample_rate + 0.5))


def envelope(samples: np.ndarray, sample_rate: int, hz: int = ENVELOPE_HZ) -> bytes:
    """The envelope of float PCM `samples` (any shape; flattened, mono).

    Windows cover the whole clip with none dropped: the boundaries are
    `window_count` even cuts, so the last window absorbs any remainder
    shorter than half a window and gives up any shortfall.
    """
    x = np.asarray(samples, dtype=np.float32).reshape(-1)
    n = window_count(x.size, sample_rate, hz)
    if n == 0:
        return b""
    edges = np.linspace(0, x.size, n + 1).round().astype(np.int64)
    rms = np.empty(n, dtype=np.float64)
    for i in range(n):
        chunk = x[edges[i] : edges[i + 1]]
        rms[i] = np.sqrt(np.mean(np.square(chunk, dtype=np.float64))) if chunk.size else 0.0
    peak = float(rms.max())
    if peak <= 0.0 or not np.isfinite(peak):
        return bytes(n)
    shaped = np.power(rms / peak, CURVE) * 255.0
    return np.clip(shaped.round(), 0, 255).astype(np.uint8).tobytes()
