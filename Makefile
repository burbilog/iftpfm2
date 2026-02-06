
all:
	@echo "iftpfm2 - Interactive File Transfer for Professional Media 2"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@echo "Build targets:"
	@echo "  debug      - Build debug version (cargo build)"
	@echo "  release    - Build release version (cargo build --release)"
	@echo "  install    - Install release to ~/.cargo/bin"
	@echo ""
	@echo "Test targets:"
	@echo "  test       - Run all tests (unit + integration, including Docker if available)"
	@echo "  test-sftp              - SFTP password authentication tests (Docker)"
	@echo "  test-sftp-keys         - SFTP SSH key authentication tests (Docker)"
	@echo "  test-temp              - Temp directory and debug logging test"
	@echo "  test-pid               - PID file handling test"
	@echo "  test-pid-no-xdg        - PID handling test WITHOUT XDG_RUNTIME_DIR"
	@echo ""
	@echo "Other targets:"
	@echo "  cloc       - Count lines of code (requires cloc utility)"

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

# Count lines of code (excludes build directories and other temporary files)
cloc:
	@echo "Counting lines of code..."
	@which cloc >/dev/null 2>&1 || { echo "cloc not found. Install with: sudo apt install cloc"; exit 1; }
	@cloc --exclude-dir=.git,.claude,target \
		--exclude-list-file=.gitignore \
		.
