TAGS ?= webkit2_41

.PHONY: dev build build-windows build-macos clean test

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
