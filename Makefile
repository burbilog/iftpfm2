
all:
	@echo usage: make debug or make release or make install or make test or make test-sftp or make test-sftp-keys or make test-temp or make test-pid or make test-pid-no-xdg or make cloc

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
	./test_pid_no_xdg.sh
	./test_ram_threshold.sh
	@echo ""
	@command -v docker >/dev/null 2>&1 && { \
		echo "Running Docker integration tests..."; \
		./test_sftp_docker.sh; \
		./test_sftp_keys_docker.sh; \
	} || { \
		echo "=================================================="; \
		echo "WARNING: Docker not found - skipping SFTP tests"; \
		echo "Install Docker to run full integration test suite:"; \
		echo "  - test_sftp_docker.sh (SFTP password auth)"; \
		echo "  - test_sftp_keys_docker.sh (SFTP SSH key auth)"; \
		echo "=================================================="; \
	}

# run SFTP password authentication tests with Docker (also included in main test target if Docker is available)
test-sftp:
	cargo build
	./test_sftp_docker.sh

# run SFTP SSH key authentication tests with Docker (also included in main test target if Docker is available)
test-sftp-keys:
	cargo build
	./test_sftp_keys_docker.sh

# run temp directory test (separate target, not included in main test target)
test-temp:
	cargo build
	./test_temp_dir.sh

# run PID handling test (included in main test target)
test-pid:
	cargo build
	./test_pid.sh

# run PID handling test WITHOUT XDG_RUNTIME_DIR (included in main test target)
test-pid-no-xdg:
	cargo build
	./test_pid_no_xdg.sh

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
