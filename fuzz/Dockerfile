# Pinned by digest: the manifest list that `stable` and `latest` pointed at on
# 2026-09-02. A moving tag let this lane drift unnoticed; bump the digest on
# purpose, with a run of this workflow.
FROM aflplusplus/aflplusplus@sha256:07ae19760b62d785e71d42a56a267b6e1c7ca1effea6ed3694d0a469bebe5c3c AS builder

# The corpus generator links the sqlite backend, whose `-lsqlite3` the AFL++
# image does not provide.
RUN apt-get update \
    && apt-get install -y --no-install-recommends libsqlite3-dev \
    && rm -rf /var/lib/apt/lists/*

# The image ships cargo but not the `cargo afl` subcommand.
RUN cargo install cargo-afl --version 0.18.2 --locked

WORKDIR /mxd
COPY . .

# Build the seed corpus included in the repository
RUN make corpus

ENV CC=afl-clang-fast
ENV CXX=afl-clang-fast++
# The AFL runtime is built per toolchain, and this repository pins its own.
RUN cargo afl config --build
RUN cargo afl build --manifest-path fuzz/Cargo.toml

FROM aflplusplus/aflplusplus@sha256:07ae19760b62d785e71d42a56a267b6e1c7ca1effea6ed3694d0a469bebe5c3c
# The fuzz crate shares the repository's target directory.
COPY --from=builder /mxd/target/debug/fuzz /usr/local/bin/fuzz
# The seeds `make corpus` generated, so a run needs no corpus mount.
COPY --from=builder /mxd/fuzz/corpus /corpus
ENTRYPOINT ["afl-fuzz", "-M", "main", "-i", "/corpus", "-o", "/out", "--", "/usr/local/bin/fuzz", "@@"]
