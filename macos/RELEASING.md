# Releasing ctail to the Mac App Store (with IAP + TestFlight)

The SwiftPM package (`Package.swift`) is for dev/CLI/self-tests. App Store
distribution uses an Xcode app target generated from `project.yml` via
[XcodeGen](https://github.com/yonaskolb/XcodeGen) — `ctail.xcodeproj` is
git-ignored and regenerated on demand.

## One-time setup
1. **Apple Developer Program** membership.
2. In **App Store Connect**:
   - Create the app, bundle id **`no.bogentech.ctail`**.
   - Create the in-app purchase: **Non-Consumable**, product id **`no.bogentech.ctail.pro`**,
     set price tier, name, description. (Enroll in the **Small Business Program** → 15% fee.)
3. Set your team in `project.yml` (`DEVELOPMENT_TEAM:`), or pick it in Xcode after generating.
4. **Agreements, tax and banking** — see the next section. TestFlight and a free app run on the
   Free Apps Agreement alone; the in-app purchase does not ship until the paid one is in place.

## Agreements, tax and banking (Account Holder only)
All of it lives under **App Store Connect ▸ Business ▸ _legal entity_** (`appstoreconnect.apple.com/business`),
not under the app. A new legal entity (including one created by converting a personal account to a
company) starts with the **Free Apps Agreement only**; the Paid Apps Agreement, banking and tax forms
have to be done for it separately. Until they are, the entity page shows *Actions Pending* and the
Paid Apps row reads **New**. In order:

1. **Edit Legal Entity** (the link in the banner above the agreements table): confirm the company's
   legal name, address and contact details.
2. **Paid Apps Agreement ▸ View** → request and accept it. This is a separate contract from the free
   one, and the one in-app purchases are sold under.
3. **Banking**: the account payouts go to (appears once the paid agreement is accepted).
4. **Tax**: the U.S. tax form for a non-U.S. company (W-8BEN-E) plus Apple's own tax questionnaire; a
   Norwegian AS also gets the Norway VAT questions. Apple's substitute W-8BEN-E, for a Norwegian
   company (a form is valid to the end of the third calendar year after signing):
   - Part I: Corporation (the FATCA/chapter 4 section disappears once that is chosen); no U.S. TIN;
     foreign TIN = the organisasjonsnummer.
   - Part III, line 14: resident of Norway ✓, derives the income ✓, LOB type **"No LOB article in
     treaty"** — the U.S.–Norway treaty is from 1971 and predates LOB articles; the dividends box stays
     unticked.
   - Line 15: **Article 5, paragraph 1**, rate **0**, "Income from the sale of applications". The 1971
     treaty's numbering is old: Business Profits is Article 5 and Royalties is Article 10 (modern
     treaties put them at 7 and 12, which is where "Article 7 paragraph 1" advice online comes from).
     Both give 0% — Article 5(1) exempts a Norwegian resident's business profits absent a U.S.
     permanent establishment, Article 10(1) exempts royalties — so the explanation names both.
   - Part XXX is two checkboxes; the signed-in Account Holder is the signature and there are no
     name/capacity/date fields.
5. **Digital Services Act**: declare whether the entity is a **trader**. A company selling an app or an
   in-app purchase to users in the EU is one — the DSA turns on commercial activity, not on where the
   developer is (Norway is EEA, the EU storefronts are still EU). Declaring trader means an address,
   phone number and email are shown on the EU product page; any of the company's contact details will
   do. Declaring non-trader restricts distribution to outside the EU.

The Paid Apps Agreement shows **Active** with an effective date when it is done; the IAP's
*Add for Review* is available from then on.

## Submission checklist (per version, on the app's pages)
- **Pricing and Availability**: a price (Free for ctail; Pro is the IAP) and the regions. Both are
  empty on a new app record and both block submission.
- **App Information**: Content Rights (no third-party content), Age Ratings questionnaire (4+),
  categories (Developer Tools / Utilities).
- **App Privacy**: policy URL and the "Data Not Collected" label, published.
- **Version page**: version number matches the build's `CFBundleShortVersionString`, the build is
  attached (*Add Build* lists processed uploads), 10 screenshots at most, **Sign-in required**
  unticked (no account exists), App Review contact filled, notes explaining how to reach the purchase
  sheet and Restore Purchases.
- **In-App Purchase**: availability, en-US localization, price, a **review screenshot** of the paywall
  (take it from the TestFlight build, where the sandbox price shows), review notes. The first IAP must be
  submitted **together with** the first app version — *Add for Review* on the IAP, then on the version.

## Build & upload — automated (GitHub Actions → TestFlight)
The **macOS TestFlight** workflow (`.github/workflows/macos-testflight.yml`) builds,
signs, and uploads a beta to TestFlight. Trigger it from **Actions ▸ macOS TestFlight
▸ Run workflow** (manual). It bumps the build number to the workflow run number, so
every run produces a unique TestFlight build.

### Required repo secrets (one-time)
Add these under **Settings ▸ Secrets and variables ▸ Actions**:

| Secret | What it is / how to get it |
|---|---|
| `APPLE_TEAM_ID` | Your 10-char Team ID (App Store Connect ▸ Membership). |
| `BUILD_CERTIFICATE_BASE64` | An **Apple Distribution** certificate exported from Keychain as `.p12`, then `base64 -i cert.p12 \| pbcopy`. |
| `P12_PASSWORD` | The password you set when exporting the `.p12`. |
| `KEYCHAIN_PASSWORD` | Any throwaway string (temp keychain password). |
| `APP_STORE_CONNECT_KEY_ID` | App Store Connect ▸ **Users and Access ▸ Integrations ▸ App Store Connect API** ▸ key ID. |
| `APP_STORE_CONNECT_ISSUER_ID` | The Issuer ID on that same page. |
| `APP_STORE_CONNECT_API_KEY_BASE64` | The downloaded `AuthKey_XXXX.p8`, `base64 -i AuthKey_*.p8 \| pbcopy`. Give the key **App Manager** role. |

The API key drives automatic provisioning (`-allowProvisioningUpdates`) and the upload,
so no provisioning profile needs to be managed by hand.

> First run note: this pipeline couldn't be executed end-to-end without your Apple
> credentials, so expect to fine-tune on the first run — most likely the export
> `method` string (`app-store` vs `app-store-connect` on newer Xcode) or the signing
> style. The app record + IAP (below) must already exist in App Store Connect.

## Build & upload — manual (Xcode, fallback)
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
- IAP via StoreKit 2 (`StoreManager` / `no.bogentech.ctail.pro`); IAP needs no entitlement.
- App icon as an asset catalog (`Assets.xcassets/AppIcon`), version, `LSApplicationCategoryType`,
  document types — all set in `project.yml`.
- `appIcon()` guards `Bundle.module` with `#if SWIFT_PACKAGE` so the Xcode target compiles.
- A privacy manifest (`Resources/PrivacyInfo.xcprivacy`): collects nothing, tracks nobody, and
  names the required-reason APIs the app touches (UserDefaults; file timestamps on the logs the
  user opened and on the container's own files).
- The sandboxed (store) build has no **Check for Updates** and no launch-time check: the check
  asks GitHub's releases, which is the direct download, and App Review does not allow a store
  app to point there. The store delivers its own updates.
