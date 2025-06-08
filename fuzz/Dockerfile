FROM aflplusplus/aflplusplus:latest AS builder

WORKDIR /mxd
COPY . .

ENV CC=afl-clang-fast
ENV CXX=afl-clang-fast++
RUN cargo afl build --manifest-path fuzz/Cargo.toml

COPY --from=builder /mxd/fuzz/target/debug/fuzz /usr/local/bin/fuzz

FROM aflplusplus/aflplusplus:latest
COPY --from=builder /mxd/fuzz/target/release/fuzz /usr/local/bin/fuzz
ENTRYPOINT ["afl-fuzz", "-M", "main", "-i", "/corpus", "-o", "/out", "--", "/usr/local/bin/fuzz", "@@"]
