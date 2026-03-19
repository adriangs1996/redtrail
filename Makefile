rt:
	@RUST_LOG=trace cargo run -- shell --session test

build:
	cargo build --release
