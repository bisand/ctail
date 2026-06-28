# Releasing ctail to the Mac App Store (with IAP + TestFlight)

The SwiftPM package (`Package.swift`) is for dev/CLI/self-tests. App Store
distribution uses an Xcode app target generated from `project.yml` via
[XcodeGen](https://github.com/yonaskolb/XcodeGen) — `ctail.xcodeproj` is
git-ignored and regenerated on demand.

## One-time setup
1. **Apple Developer Program** membership.
2. In **App Store Connect**:
   - Create the app, bundle id **`net.biseth.ctail`**.
   - Create the in-app purchase: **Non-Consumable**, product id **`net.biseth.ctail.pro`**,
     set price tier, name, description. (Enroll in the **Small Business Program** → 15% fee.)
3. Set your team in `project.yml` (`DEVELOPMENT_TEAM:`), or pick it in Xcode after generating.

## Build & upload
```sh
cd macos
make xcodeproj          # xcodegen generate
open ctail.xcodeproj    # Signing & Capabilities → select your Team (Automatic signing)
```
Then in Xcode: **Product ▸ Archive → Distribute App → App Store Connect → Upload**.
(macOS App Store apps ship as a signed `.pkg`; Xcode handles this.)

The build is **Release + sandboxed**, so the DEBUG dev-unlock is compiled out and the
CLI AI providers are hidden — testers get the real App Store experience.

## TestFlight (macOS)
After the build finishes processing in App Store Connect → **TestFlight**:
- **Internal testers** (your team, ≤100): no review, available immediately.
- **External testers** (≤10,000): a quick Beta App Review, then a public/invite link.
- Testers install via the **TestFlight Mac app**.

### Testing the paywall
In-app purchases are **free in TestFlight** (App Store *sandbox*) — testers can run the
real Pro purchase + Restore without being charged. The local `ctail.storekit` file is
only for Xcode runs (Scheme ▸ Run ▸ Options ▸ StoreKit Configuration); TestFlight uses
the real sandbox, so the IAP must exist in App Store Connect.

## Already in place (App Store compatible)
- App Sandbox + security-scoped bookmarks (`Resources/ctail.entitlements`).
- IAP via StoreKit 2 (`StoreManager` / `net.biseth.ctail.pro`); IAP needs no entitlement.
- App icon as an asset catalog (`Assets.xcassets/AppIcon`), version, `LSApplicationCategoryType`,
  document types — all set in `project.yml`.
- `appIcon()` guards `Bundle.module` with `#if SWIFT_PACKAGE` so the Xcode target compiles.
