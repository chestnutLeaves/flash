# The natural-voice sidecar. Build from the repository root:
#   docker build -t flash-tts tools/flash-tts
# or let docker compose do it: docker compose --profile voice up -d
#
# The weights and every voice pack are downloaded at build time into
# /models, so the running container never needs the network
# (HF_HUB_OFFLINE=1). Expect the image to weigh a little over 2 GB, most
# of it the CPU build of torch.
FROM python:3.12-slim

# espeak-ng phonemizes the English words the engine's dictionary lacks;
# ffmpeg encodes the clips. Both are called as separate processes.
RUN apt-get update \
    && apt-get install -y --no-install-recommends espeak-ng ffmpeg \
    && rm -rf /var/lib/apt/lists/*

ENV PIP_NO_CACHE_DIR=1 \
    PIP_DISABLE_PIP_VERSION_CHECK=1 \
    PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    HF_HOME=/models

WORKDIR /app
COPY pyproject.toml README.md ./
COPY flash_tts ./flash_tts
RUN pip install torch --index-url https://download.pytorch.org/whl/cpu \
    && pip install .

# Model, voices and the English tokenizer's data, fetched once here. The
# service user only reads them.
RUN useradd --system --create-home --uid 10001 flash-tts \
    && mkdir -p /models \
    && python -m flash_tts --prefetch \
    && chown -R flash-tts:flash-tts /models

USER flash-tts
ENV HF_HUB_OFFLINE=1
EXPOSE 8880
HEALTHCHECK --interval=30s --timeout=5s --start-period=90s --retries=3 \
    CMD python -c "import sys, urllib.request; sys.exit(0 if urllib.request.urlopen('http://127.0.0.1:8880/health', timeout=4).status == 200 else 1)"
CMD ["python", "-m", "flash_tts", "--bind", "0.0.0.0:8880"]
