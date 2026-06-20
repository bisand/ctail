# ctail — native macOS app build tooling
APP        := ctail
BUNDLE_ID  := net.biseth.ctail
VERSION    := $(shell cat ../VERSION 2>/dev/null || echo 0.0.0)
BUILD      := $(shell cat ../BUILD_NUMBER 2>/dev/null || echo 1)
CONFIG     := release
BUILD_DIR  := .build/$(CONFIG)
APP_DIR    := $(APP).app
CONTENTS   := $(APP_DIR)/Contents

.PHONY: all build run bundle icon clean test

all: bundle

## build: compile the binary
build:
	swift build -c $(CONFIG)

## run: build + run the unbundled binary (pass FILE=/path/to.log)
run:
	swift build
	./.build/debug/ctailmac $(FILE)

## icon: generate AppIcon.icns from Resources/appicon.png
icon:
	@bash scripts/make-icns.sh Resources/appicon.png .build/AppIcon.icns

## bundle: assemble a distributable ctail.app
bundle: build icon
	@rm -rf $(APP_DIR)
	@mkdir -p $(CONTENTS)/MacOS $(CONTENTS)/Resources
	@cp $(BUILD_DIR)/ctailmac $(CONTENTS)/MacOS/$(APP)
	@cp .build/AppIcon.icns $(CONTENTS)/Resources/AppIcon.icns
	@sed -e 's/__VERSION__/$(VERSION)/' -e 's/__BUILD__/$(BUILD)/' \
		Resources/Info.plist.in > $(CONTENTS)/Info.plist
	@codesign --force --deep \
		--entitlements Resources/ctail.entitlements \
		--sign - $(APP_DIR) 2>/dev/null \
		&& echo "ad-hoc signed" || echo "codesign skipped (ad-hoc unavailable)"
	@echo "built $(APP_DIR) (v$(VERSION) build $(BUILD))"

## test: run the Swift test suite
test:
	swift test

## clean: remove build artifacts
clean:
	swift package clean
	rm -rf $(APP_DIR) .build/AppIcon.icns .build/AppIcon.iconset
