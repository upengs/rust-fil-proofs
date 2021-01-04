#	cargo run --release --bin benchy -- merkleproofs --size 2Kib
#    > cargo run --release --bin benchy -- winning-post --size 2Kib
#    > cargo run --release --bin benchy -- window-post --size 2Kib
#    > cargo run --release --bin benchy -- prodbench --size 2Kib


env:
 export RUST_BACKTRACE=1

.PHONY: wp
wp: env
	 RUST_BACKTRACE=full RUST_LOG=filecoin_proofs=info RUST_LOG=trace cargo run --release --bin benchy -- window-post --size 2Kib

.PHONY: test
test:
	cargo test --all

.PHONY: build
build:
	cargo build --release --all
