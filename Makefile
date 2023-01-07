
all:
	@echo usage: make debug or make release or make install or make test

# install into ~/.cargo/bin
install: release
	cargo install --path .

# build debug version
debug:
	cargo build

# build release version
release:
	cargo build --release

# run unit tests
test:
	cargo test
