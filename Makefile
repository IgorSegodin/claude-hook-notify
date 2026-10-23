.PHONY: build test icon bundle install uninstall

build:
	cargo build

test:
	cargo test

icon:
	sh scripts/icon.sh

bundle:
	sh scripts/bundle.sh

install: bundle
	sh scripts/install.sh

uninstall:
	sh scripts/uninstall.sh
