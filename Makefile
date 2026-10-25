SHELL := /bin/sh
CARGO ?= cargo
PYTHON ?= python3
PREFIX ?= $(HOME)/.softwares
ZSHRC ?= $(HOME)/.zshrc
CARGO_TARGET_DIR ?= target
BENCHMARK_ARGS ?= --entries 20000 --runs 31
DEMO_PYTHON ?= $(PYTHON)

.PHONY: all build install test check fmt clean parity benchmark demo
all: build

build:
	$(CARGO) build --release --locked --target-dir "$(CARGO_TARGET_DIR)"

install: build
	$(PYTHON) scripts/install.py --binary "$(CARGO_TARGET_DIR)/release/qs" --prefix "$(PREFIX)" --zshrc "$(ZSHRC)"

test:
	$(CARGO) test --all-targets --locked
	$(PYTHON) -m unittest discover -s tests -p 'test_install.py'

check:
	$(CARGO) fmt --all -- --check
	$(CARGO) clippy --all-targets --locked -- -D warnings

parity: build
	$(PYTHON) scripts/macos-parity.py --binary "$(CARGO_TARGET_DIR)/release/qs" --save "$(CARGO_TARGET_DIR)/macos-parity.json"

benchmark: build
	$(PYTHON) scripts/benchmark.py --binary "$(CARGO_TARGET_DIR)/release/qs" $(BENCHMARK_ARGS) --save "$(CARGO_TARGET_DIR)/benchmark.json"

demo: build
	$(DEMO_PYTHON) scripts/demo.py --binary "$(CARGO_TARGET_DIR)/release/qs"

fmt:
	$(CARGO) fmt --all

clean:
	$(CARGO) clean --target-dir "$(CARGO_TARGET_DIR)"
