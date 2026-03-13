TAGS ?= webkit2_41
PREFIX ?= /usr/local

.PHONY: dev build build-windows build-macos clean test install uninstall

dev:
	wails dev -tags $(TAGS)

build:
	wails build -tags $(TAGS)

build-windows:
	wails build -platform windows/amd64

build-macos:
	wails build -platform darwin/universal

clean:
	rm -rf build/bin

test:
	go test ./internal/... -v

install:
	install -Dm755 build/bin/ctail $(DESTDIR)$(PREFIX)/bin/ctail
	install -Dm644 build/appicon.png $(DESTDIR)$(PREFIX)/share/icons/hicolor/1024x1024/apps/ctail.png
	install -Dm644 build/linux/ctail.desktop $(DESTDIR)$(PREFIX)/share/applications/ctail.desktop
	install -Dm644 build/linux/ctail-x11.desktop $(DESTDIR)$(PREFIX)/share/applications/ctail-x11.desktop

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/ctail
	rm -f $(DESTDIR)$(PREFIX)/share/icons/hicolor/1024x1024/apps/ctail.png
	rm -f $(DESTDIR)$(PREFIX)/share/applications/ctail.desktop
	rm -f $(DESTDIR)$(PREFIX)/share/applications/ctail-x11.desktop
