# Daedalus E2E sandbox image (issue #19): a real coding agent driven by a tiny local
# model, fully offline — proving working agent sessions under the deny-all egress policy.
#
#   opencode (agent) → ollama serving gemma4:e2b (2.3B effective params, ~2.9GB Q4)
#
# Everything network-dependent happens at BUILD time on the host (which has normal
# network); at RUN time the sandbox needs zero egress. Build:
#
#   docker build -t daedalus/e2e-openshell:latest tests/e2e/openshell/
#
# Then: openshell sandbox create --from daedalus/e2e-openshell:latest …
# The OpenShell supervisor is bind-mounted in by the runtime; extending the community
# base image keeps its conventions (user `sandbox`, home `/sandbox`).

FROM ghcr.io/nvidia/openshell-community/sandboxes/base:latest

USER root

# Build/runtime deps: zstd (ollama installer extraction), python3 (fixture verification).
RUN apt-get update && apt-get install -y --no-install-recommends zstd python3 \
    && rm -rf /var/lib/apt/lists/*

# zellij (not packaged in Ubuntu) — the in-sandbox multiplexer for FR-011 attach.
RUN set -eux; \
    arch="$(uname -m)"; \
    curl -fsSL -o /tmp/zellij.tar.gz \
      "https://github.com/zellij-org/zellij/releases/latest/download/zellij-${arch}-unknown-linux-musl.tar.gz"; \
    tar -xzf /tmp/zellij.tar.gz -C /usr/local/bin zellij; \
    rm /tmp/zellij.tar.gz; \
    zellij --version

# ollama, with the model store at a world-readable, non-$HOME path so the `sandbox`
# user can serve the baked weights.
ENV OLLAMA_MODELS=/opt/ollama-models
RUN curl -fsSL https://ollama.com/install.sh | sh && ollama --version

# Bake gemma4:e2b into the image, plus a 16k-context variant — ollama defaults to a
# 4096-token window and opencode's agentic tools need >=16k.
RUN set -eux; \
    mkdir -p "$OLLAMA_MODELS"; \
    (ollama serve &) ; \
    for i in $(seq 1 30); do ollama list >/dev/null 2>&1 && break; sleep 1; done; \
    ollama pull gemma4:e2b; \
    printf 'FROM gemma4:e2b\nPARAMETER num_ctx 16384\n' > /tmp/Modelfile; \
    ollama create gemma4:e2b-16k -f /tmp/Modelfile; \
    rm /tmp/Modelfile; \
    chmod -R a+rX "$OLLAMA_MODELS"

# The sandbox supervisor doesn't propagate Dockerfile ENV to exec'd shells — expose the
# baked model store at ollama's default per-user path instead.
RUN mkdir -p /sandbox/.ollama \
    && ln -s "$OLLAMA_MODELS" /sandbox/.ollama/models \
    && chown -h sandbox:sandbox /sandbox/.ollama /sandbox/.ollama/models

# opencode — the agent under test (non-interactive via `opencode run`).
RUN set -eux; \
    curl -fsSL https://opencode.ai/install | bash; \
    install -m 0755 "$(find /root -name opencode -type f | head -1)" /usr/local/bin/opencode; \
    opencode --version

# Agent + fixture land in the sandbox user's home.
COPY --chown=sandbox:sandbox opencode.json /sandbox/.config/opencode/opencode.json
COPY --chown=sandbox:sandbox fixture/ /sandbox/fixture/

USER sandbox
WORKDIR /sandbox
