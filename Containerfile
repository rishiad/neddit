FROM docker.io/library/debian:bookworm-slim AS video-tool

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates curl \
    && curl --fail --location --silent --show-error https://github.com/yt-dlp/yt-dlp/releases/download/2026.08.19/yt-dlp_linux -o /yt-dlp \
    && echo "58162f9bfdc27458ea47bfcb311cf47028f17d8154a8bf7d689861d46399230a  /yt-dlp" | sha256sum --check -

FROM docker.io/library/rust:1.88.0-bookworm AS build

RUN apt-get update \
    && apt-get install --yes --no-install-recommends clang cmake ninja-build \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --locked --release --package neddit

FROM docker.io/library/debian:bookworm-slim

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 neddit \
    && useradd --no-create-home --uid 10001 --gid 10001 --shell /usr/sbin/nologin neddit

COPY --from=build /src/target/release/neddit /usr/local/bin/neddit
COPY --from=video-tool /yt-dlp /usr/local/bin/yt-dlp
RUN chmod 755 /usr/local/bin/yt-dlp

USER neddit:neddit
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/neddit"]
CMD ["--config", "/etc/neddit/config.toml"]
