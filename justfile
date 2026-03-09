default: build_release

clean:
        @echo "Cleaning..."
        @cargo clean

check:
        @echo "Checking..."
        @cargo check

clippy:
        @echo "Running Clippy..."
        @cargo clippy -- -W clippy::pedantic

clippy_fix:
        @echo "Running Clippy fixes..."
        @cargo clippy --fix -- -W clippy::pedantic

build_dev:
        @echo "Building debug..."
        @cargo build

build_release:
        @echo "Building release..."
        @cargo build --release
