.PHONY: all build core companion test test-core test-companion test-tools install knowledge clean

all: build

build: core companion

core:
	cargo build --release --manifest-path core/Cargo.toml

companion:
	dotnet build companion/AlliedID.Companion.slnx -c Release --nologo

test: test-core test-companion test-tools

test-core:
	cargo test --manifest-path core/Cargo.toml

test-companion:
	dotnet build companion/AlliedID.Companion.slnx -c Release --nologo -warnaserror

test-tools:
	python3 -m unittest discover -s tools/tests

knowledge:
	python3 tools/knowledge_builder.py knowledge/sources/basics --name allied-id-basics

install:
	scripts/install.sh

clean:
	cargo clean --manifest-path core/Cargo.toml
	dotnet clean companion/AlliedID.Companion.slnx --nologo -v quiet
