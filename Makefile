
all:
	@echo usage: make debug or make release or make install or make test or make test-sftp or make test-temp or make test-pid or make cloc

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
	./test_sftp_timeout.sh
	./test_ftps.sh
	./test_temp_dir.sh
	./test_pid.sh
	./test_ram_threshold.sh

# run SFTP tests with Docker (separate target, not included in main test target)
test-sftp:
	cargo build
	./test_sftp_docker.sh

# run temp directory test (separate target, not included in main test target)
test-temp:
	cargo build
	./test_temp_dir.sh

# run PID handling test (included in main test target)
test-pid:
	cargo build
	./test_pid.sh

# generate and install rust documentation (always rebuilds)
.PHONY: doc
doc:
	@echo "Generating fresh documentation..."
	@rm -rf doc target/doc
	cargo doc --no-deps
	@mkdir -p doc
	@cp -r target/doc/* doc/
	@echo "Documentation regenerated in doc/"

# Count lines of code (excludes build directories and other temporary files)
cloc:
	@echo "Counting lines of code..."
	@which cloc >/dev/null 2>&1 || { echo "cloc not found. Install with: sudo apt install cloc"; exit 1; }
	@cloc --exclude-dir=.git,.claude,target \
		--exclude-list-file=.gitignore \
		.
