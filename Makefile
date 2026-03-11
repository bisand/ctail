TAGS ?= webkit2_41

.PHONY: dev build clean

dev:
	wails dev -tags $(TAGS)

build:
	wails build -tags $(TAGS)

clean:
	rm -rf build/bin

test:
	go test ./internal/... -v
