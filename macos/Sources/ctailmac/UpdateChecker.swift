import Foundation

/// Checks GitHub releases for a newer version, mirroring app.go's
/// fetchLatestRelease. App Store builds should leave this disabled (the Store
/// handles updates) via the disableUpdateCheck setting.
enum UpdateChecker {
    struct Result {
        let current: String
        let latest: String
        let updateAvailable: Bool
        let notes: String
        let url: String
        let error: String?
    }

    static let releasesAPI = "https://api.github.com/repos/bisand/ctail/releases/latest"

    static func check(current: String, completion: @escaping (Result) -> Void) {
        guard let url = URL(string: releasesAPI) else {
            completion(Result(current: current, latest: "", updateAvailable: false, notes: "", url: "",
                              error: "bad URL")); return
        }
        var req = URLRequest(url: url)
        req.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        req.timeoutInterval = 15
        URLSession.shared.dataTask(with: req) { data, resp, err in
            let done: (Result) -> Void = { r in DispatchQueue.main.async { completion(r) } }
            if let err { done(fail(current, "Failed to check for updates: \(err.localizedDescription)")); return }
            guard let http = resp as? HTTPURLResponse, http.statusCode == 200 else {
                let code = (resp as? HTTPURLResponse)?.statusCode ?? 0
                done(fail(current, "Failed to check for updates (HTTP \(code))")); return
            }
            guard let data,
                  let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let tag = obj["tag_name"] as? String else {
                done(fail(current, "Failed to parse update info")); return
            }
            let latest = tag.hasPrefix("v") ? String(tag.dropFirst()) : tag
            let notes = (obj["body"] as? String) ?? ""
            let htmlURL = (obj["html_url"] as? String) ?? "https://github.com/bisand/ctail/releases"
            done(Result(current: current, latest: latest,
                        updateAvailable: compareVersions(latest, current) > 0,
                        notes: notes, url: htmlURL, error: nil))
        }.resume()
    }

    private static func fail(_ current: String, _ msg: String) -> Result {
        Result(current: current, latest: "", updateAvailable: false, notes: "", url: "", error: msg)
    }

    /// Returns >0 if a is newer than b, <0 if older, 0 if equal. Compares dotted
    /// numeric components (ignoring any build suffix after '+').
    static func compareVersions(_ a: String, _ b: String) -> Int {
        func parts(_ s: String) -> [Int] {
            s.split(separator: "+").first.map(String.init)?
                .split(separator: ".").map { Int($0.prefix(while: \.isNumber)) ?? 0 } ?? []
        }
        let pa = parts(a), pb = parts(b)
        for i in 0..<max(pa.count, pb.count) {
            let x = i < pa.count ? pa[i] : 0
            let y = i < pb.count ? pb[i] : 0
            if x != y { return x < y ? -1 : 1 }
        }
        return 0
    }
}
