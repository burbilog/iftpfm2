
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
	cargo build # Ensure binary is built before testing
	cargo test
	./test.sh
	./test_age.sh
	./test_conn_timeout.sh
	./test_ftps.sh

# generate and install rust documentation (always rebuilds)
.PHONY: doc
doc:
	@echo "Generating fresh documentation..."
	@rm -rf doc target/doc
	cargo doc --no-deps
	@mkdir -p doc
	@cp -r target/doc/* doc/
	@echo "Documentation regenerated in doc/"
