.PHONY: all clean test test-postgres test-sqlite corpus sqlite postgres sqlite-release postgres-release

corpus:
	cargo run --bin gen_corpus

sqlite: target/debug/mxd
postgres: target/postgres/debug/mxd
sqlite-release: target/release/mxd
postgres-release: target/postgres/release/mxd

all: sqlite-release

clean:
	cargo clean
	rm -rf target/postgres

test: test-postgres test-sqlite

test-postgres:
	RUSTFLAGS="-D warnings" cargo test --no-default-features --features postgres

test-sqlite:
	RUSTFLAGS="-D warnings" cargo test --features sqlite

target/debug/mxd:
	cargo build --bin mxd --features sqlite

target/postgres/debug/mxd:
	cargo build --bin mxd --no-default-features --features postgres --target-dir target/postgres

target/release/mxd:
	cargo build --release --bin mxd --features sqlite

target/postgres/release/mxd:
	cargo build --release --bin mxd --no-default-features --features postgres --target-dir target/postgres
