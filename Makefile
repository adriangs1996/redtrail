build:
	cargo build --release

dev:
	@docker build -f eval/Dockerfile -t redtrail-dev \
		--build-arg USER_UID=$$(id -u) \
		--build-arg USER_GID=$$(id -g) .
	@docker run --rm -it \
		--env-file .env.development \
		redtrail-dev /bin/zsh

test-live:
	@./scripts/live-test.sh
