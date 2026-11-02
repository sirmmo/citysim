# Pinned tooling for baking 0 A.D. art from the original repository:
# git + git-lfs (sparse, shallow checkout) and Python with pycollada/NumPy/Pillow.
FROM python:3.12.7-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends git git-lfs ca-certificates \
 && git lfs install --system \
 && git config --system --add safe.directory '*' \
 && rm -rf /var/lib/apt/lists/*
RUN pip install --no-cache-dir --root-user-action=ignore --disable-pip-version-check \
      pycollada==0.8 numpy==1.26.4 pillow==11.0.0
WORKDIR /w
