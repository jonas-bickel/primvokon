PREFIX ?= /usr/local
APP_ID = io.github.jonas_bickel.Primvokon

.PHONY: build release test lint install uninstall

build:
	cargo build

release:
	cargo build --release

test:
	cargo test --workspace

lint:
	cargo clippy --workspace --all-targets -- -D warnings
	cargo fmt --all -- --check

install: release
	install -Dm755 target/release/primvokon $(DESTDIR)$(PREFIX)/bin/primvokon
	install -Dm644 crates/primvokon/data/$(APP_ID).desktop $(DESTDIR)$(PREFIX)/share/applications/$(APP_ID).desktop
	install -Dm644 crates/primvokon/data/icons/$(APP_ID).svg $(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/$(APP_ID).svg
	install -Dm644 crates/primvokon/data/icons/$(APP_ID)-symbolic.svg $(DESTDIR)$(PREFIX)/share/icons/hicolor/symbolic/apps/$(APP_ID)-symbolic.svg
	-update-desktop-database $(DESTDIR)$(PREFIX)/share/applications 2>/dev/null
	-gtk-update-icon-cache -q $(DESTDIR)$(PREFIX)/share/icons/hicolor 2>/dev/null

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/primvokon
	rm -f $(DESTDIR)$(PREFIX)/share/applications/$(APP_ID).desktop
	rm -f $(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/$(APP_ID).svg
	rm -f $(DESTDIR)$(PREFIX)/share/icons/hicolor/symbolic/apps/$(APP_ID)-symbolic.svg
