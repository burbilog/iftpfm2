
install: release
	cargo install --path .

debug:
	cargo build

release:
	cargo build --release
