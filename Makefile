TAGS ?= webkit2_41
PREFIX ?= /usr/local
NFPM ?= $(shell go env GOPATH)/bin/nfpm
VERSION ?= 0.5.0
BUILD_NUMBER := $(shell git rev-list --count HEAD 2>/dev/null || echo 0)
LDFLAGS := -ldflags "-X main.buildNumber=$(BUILD_NUMBER)"

.PHONY: dev build build-windows build-macos clean test install uninstall package-deb package-rpm

dev:
	wails dev -tags $(TAGS)

build:
	wails build -tags $(TAGS) $(LDFLAGS)

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
	install -Dm644 build/linux/ctail.desktop $(DESTDIR)$(PREFIX)/share/applications/ctail.desktop
	install -Dm644 build/appicon.png $(DESTDIR)$(PREFIX)/share/icons/hicolor/1024x1024/apps/ctail.png
	install -Dm644 build/linux/ctail-512.png $(DESTDIR)$(PREFIX)/share/icons/hicolor/512x512/apps/ctail.png
	install -Dm644 build/linux/ctail-256.png $(DESTDIR)$(PREFIX)/share/icons/hicolor/256x256/apps/ctail.png
	install -Dm644 build/linux/ctail-128.png $(DESTDIR)$(PREFIX)/share/icons/hicolor/128x128/apps/ctail.png
	install -Dm644 build/linux/ctail-64.png $(DESTDIR)$(PREFIX)/share/icons/hicolor/64x64/apps/ctail.png
	install -Dm644 build/linux/ctail-48.png $(DESTDIR)$(PREFIX)/share/icons/hicolor/48x48/apps/ctail.png
	install -Dm644 build/linux/ctail-32.png $(DESTDIR)$(PREFIX)/share/icons/hicolor/32x32/apps/ctail.png
	install -Dm644 build/linux/ctail-16.png $(DESTDIR)$(PREFIX)/share/icons/hicolor/16x16/apps/ctail.png
	-gtk-update-icon-cache -f -t $(DESTDIR)$(PREFIX)/share/icons/hicolor 2>/dev/null || true

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/ctail
	rm -f $(DESTDIR)$(PREFIX)/share/applications/ctail.desktop
	rm -f $(DESTDIR)$(PREFIX)/share/icons/hicolor/1024x1024/apps/ctail.png
	rm -f $(DESTDIR)$(PREFIX)/share/icons/hicolor/512x512/apps/ctail.png
	rm -f $(DESTDIR)$(PREFIX)/share/icons/hicolor/256x256/apps/ctail.png
	rm -f $(DESTDIR)$(PREFIX)/share/icons/hicolor/128x128/apps/ctail.png
	rm -f $(DESTDIR)$(PREFIX)/share/icons/hicolor/64x64/apps/ctail.png
	rm -f $(DESTDIR)$(PREFIX)/share/icons/hicolor/48x48/apps/ctail.png
	rm -f $(DESTDIR)$(PREFIX)/share/icons/hicolor/32x32/apps/ctail.png
	rm -f $(DESTDIR)$(PREFIX)/share/icons/hicolor/16x16/apps/ctail.png
	-gtk-update-icon-cache -f -t $(DESTDIR)$(PREFIX)/share/icons/hicolor 2>/dev/null || true

package-deb: build
	VERSION=$(VERSION).$(BUILD_NUMBER) $(NFPM) package --packager deb --target build/

package-rpm: build
	VERSION=$(VERSION).$(BUILD_NUMBER) $(NFPM) package --packager rpm --target build/
