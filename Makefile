TAGS ?= webkit2_41
PREFIX ?= /usr/local
NFPM ?= $(shell go env GOPATH)/bin/nfpm

# Read version from tracked file
VERSION := $(shell cat VERSION 2>/dev/null || echo 0.0.0-dev)

# Build number — read from file, targets that produce binaries bump it
BUILD_NUMBER := $(shell cat BUILD_NUMBER 2>/dev/null || echo 0)

.PHONY: dev build build-windows build-macos build-gio clean test install uninstall package-deb package-rpm

dev:
	wails dev -tags $(TAGS) -ldflags "-X main.version=$(VERSION) -X main.buildNumber=$(BUILD_NUMBER)"

build:
	$(eval BUILD_NUMBER := $(shell echo $$(( $(BUILD_NUMBER) + 1 ))))
	@echo $(BUILD_NUMBER) > BUILD_NUMBER
	wails build -tags $(TAGS) -ldflags "-X main.version=$(VERSION) -X main.buildNumber=$(BUILD_NUMBER)"

build-windows:
	$(eval BUILD_NUMBER := $(shell echo $$(( $(BUILD_NUMBER) + 1 ))))
	@echo $(BUILD_NUMBER) > BUILD_NUMBER
	wails build -platform windows/amd64 -ldflags "-X main.version=$(VERSION) -X main.buildNumber=$(BUILD_NUMBER)"

build-macos:
	$(eval BUILD_NUMBER := $(shell echo $$(( $(BUILD_NUMBER) + 1 ))))
	@echo $(BUILD_NUMBER) > BUILD_NUMBER
	wails build -platform darwin/universal -ldflags "-X main.version=$(VERSION) -X main.buildNumber=$(BUILD_NUMBER)"

GIO_TAGS ?= novulkan,nox11
build-gio:
	go build -tags "$(GIO_TAGS)" -ldflags "-X main.version=$(VERSION) -X main.buildNumber=$(BUILD_NUMBER)" -o build/bin/ctail-gio ./cmd/ctail-gio/

run-gio: build-gio
	./build/bin/ctail-gio

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
	sed -i '0,/<release version="[^"]*"/{s/<release version="[^"]*"/<release version="$(VERSION).$(BUILD_NUMBER)"/}' build/linux/io.github.bisand.ctail.metainfo.xml
	export VERSION=$(VERSION).$(BUILD_NUMBER) && $(NFPM) package --packager deb --target build/
	git checkout build/linux/io.github.bisand.ctail.metainfo.xml

package-rpm: build
	sed -i '0,/<release version="[^"]*"/{s/<release version="[^"]*"/<release version="$(VERSION).$(BUILD_NUMBER)"/}' build/linux/io.github.bisand.ctail.metainfo.xml
	export VERSION=$(VERSION).$(BUILD_NUMBER) && $(NFPM) package --packager rpm --target build/
	git checkout build/linux/io.github.bisand.ctail.metainfo.xml
