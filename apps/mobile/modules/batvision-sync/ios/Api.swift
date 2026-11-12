import Foundation

enum ApiError: Error, LocalizedError {
  /// The server can't be reached (network down, server stopped).
  case unreachable(String)
  /// The server answered with an error status.
  case http(Int, String)

  var errorDescription: String? {
    switch self {
    case let .unreachable(message): return message
    case let .http(status, message): return "HTTP \(status): \(message)"
    }
  }
}

/// Calls to the BatVision server with the device token.
final class Api {
  let server: String
  let token: String

  private static let session: URLSession = {
    let config = URLSessionConfiguration.default
    config.timeoutIntervalForRequest = 30
    config.waitsForConnectivity = false
    return URLSession(configuration: config)
  }()

  init(server: String, token: String) {
    self.server = server
    self.token = token
  }

  func url(_ path: String) -> URL {
    let base = server.hasSuffix("/") ? String(server.dropLast()) : server
    return URL(string: path.hasPrefix("http") ? path : base + path)!
  }

  func request(_ path: String) -> URLRequest {
    var request = URLRequest(url: url(path))
    request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
    return request
  }

  private func send(_ method: String, _ path: String, _ body: Any? = nil) async throws -> Data {
    var req = request(path)
    req.httpMethod = method
    if let body {
      req.setValue("application/json", forHTTPHeaderField: "Content-Type")
      req.httpBody = try JSONSerialization.data(withJSONObject: body)
    }
    let (data, response): (Data, URLResponse)
    do {
      (data, response) = try await Api.session.data(for: req)
    } catch {
      throw ApiError.unreachable(error.localizedDescription)
    }
    let status = (response as? HTTPURLResponse)?.statusCode ?? 0
    // Behind a reverse proxy, a stopped server answers 502/503/504.
    if (502...504).contains(status) { throw ApiError.unreachable("server answered \(status)") }
    if !(200..<300).contains(status) {
      throw ApiError.http(status, String(decoding: data.prefix(200), as: UTF8.self))
    }
    return data
  }

  private func object(_ data: Data) throws -> [String: Any] {
    (try JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
  }

  func plan(av1: Bool) async throws -> (settings: [String: Any], items: [PlanEntry]) {
    let o = try object(try await send("POST", "/api/downloads/plan", ["av1": av1]))
    let items = (o["items"] as? [[String: Any]] ?? []).compactMap(PlanParser.entry)
    return (o["settings"] as? [String: Any] ?? [:], items)
  }

  func request(kind: String, id: Int64) async throws {
    _ = try await send("POST", "/api/downloads/request", ["kind": kind, "id": id])
  }

  /// This device drops an item: the server removes what asks for it (D74).
  func cancel(kind: String, id: Int64) async throws {
    _ = try await send("POST", "/api/downloads/cancel", ["kind": kind, "id": id])
  }

  func confirm(variantId: Int64) async throws {
    _ = try await send("POST", "/api/downloads/confirm", ["variant_id": variantId])
  }

  func progress(_ batch: [PendingProgress]) async throws {
    _ = try await send("POST", "/api/progress/batch", batch.map(\.json))
  }

  /// `/api/movies/{id}` or `/api/episodes/{id}`.
  func detail(kind: String, id: Int64) async throws -> [String: Any] {
    try object(try await send("GET", "/api/\(kind)s/\(id)"))
  }

  /// A small file (subtitle, picture); `nil` when the server doesn't have it.
  func bytes(_ path: String) async throws -> Data? {
    do {
      return try await send("GET", path)
    } catch ApiError.http {
      return nil
    }
  }
}
