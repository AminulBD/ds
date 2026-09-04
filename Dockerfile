# syntax=docker/dockerfile:1

# Build on Alpine so the toolchain is already musl: the release profile links
# musl statically, so the binary that comes out needs nothing at runtime but a
# kernel. build-base is for the vendored C in `ring`, which rustls pulls in.
FROM rust:alpine AS build

RUN apk add --no-cache build-base

WORKDIR /src
COPY . .

# The data files are embedded with include_str!, so this one build step is the
# whole binary — nothing has to be copied alongside it. `--locked` keeps the
# image on the versions in Cargo.lock.
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/src/target,sharing=locked \
    cargo build --release --locked && \
    cp target/release/ds /usr/local/bin/ds

FROM alpine:3.22

# source is what makes ghcr link the package back to this repository and show
# the README on its page; the rest is what `docker inspect` reports.
LABEL org.opencontainers.image.source="https://github.com/AminulBD/ds" \
      org.opencontainers.image.description="Check domain availability over RDAP with a WHOIS fallback" \
      org.opencontainers.image.licenses="MIT"

# ds talks TLS with webpki's bundled roots and WHOIS in the clear, so it needs
# no ca-certificates; DNS comes from the resolv.conf Docker writes.
RUN adduser -D -h /home/ds ds
COPY --from=build /usr/local/bin/ds /usr/local/bin/ds

USER ds
# The IANA RDAP bootstrap is cached here for a week. Mount a volume on it and a
# restarted container starts from a warm list instead of refetching.
ENV HOME=/home/ds \
    XDG_CACHE_HOME=/home/ds/.cache
WORKDIR /home/ds

EXPOSE 8080

# `--serve` binds loopback by default, deliberately, because a reachable ds
# server queries registries for whoever asks it to. In a container loopback
# means nobody at all, so the default here is 0.0.0.0 and the container's own
# network is what keeps it private: publish the port only where you would have
# been willing to pass --host 0.0.0.0 on the command line. Everything else —
# --max-lookups, --rate-limit, --cache-ttl, --cors — keeps its usual default
# and can be appended:
#     docker run --rm -p 8080:8080 ds --serve --host 0.0.0.0 --rate-limit 0
# The API is then GET /v1/check?name=apple&tld=com,net, with GET /healthz for
# a liveness probe. Overriding the whole command runs the CLI instead:
#     docker run --rm ds apple --tld com
ENTRYPOINT ["ds"]
CMD ["--serve", "--host", "0.0.0.0", "--port", "8080"]
