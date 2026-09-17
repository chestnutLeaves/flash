"""flash-tts: the natural-voice sidecar for Flash.

A small HTTP service the Flash server talks to over `FLASH_TTS_URL`. It
turns a card's spoken text into an MP4/AAC clip plus a loudness envelope;
the server stores the clip and the apps play it.
"""

__version__ = "0.2.0"
