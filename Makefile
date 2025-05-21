
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

# generate and install rust documentation
doc:
	cargo doc --no-deps
	@rm -rf doc
	@mkdir -p doc
	@cp -r target/doc/* doc/
	@echo "Documentation generated in doc/"
