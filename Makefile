rt:
	@RUST_LOG=trace cargo run -- shell --session test

build:
	cargo build --release

dev:
	docker build -f eval/Dockerfile -t redtrail-dev .
	docker run --rm -it redtrail-dev /bin/zsh
