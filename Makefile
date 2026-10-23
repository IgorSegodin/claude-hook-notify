.PHONY: build test bundle install uninstall

build:
	cargo build

test:
	cargo test

bundle:
	sh scripts/bundle.sh

install: bundle
	sh scripts/install.sh

uninstall:
	sh scripts/uninstall.sh
