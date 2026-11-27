#pragma once

#include <chrono>
#include <cstdint>
#include <filesystem>
#include <mutex>
#include <string>
#include <string_view>

#include <nlohmann/json.hpp>

#include "contract/error.hpp"

namespace booth {

/// One synchronous newline-delimited JSON client for the booth-control socket.
/// Requests are serialized; the UI thread must not call it.
class ControlClient {
 public:
  ControlClient() = default;
  ~ControlClient();

  ControlClient(const ControlClient&) = delete;
  ControlClient& operator=(const ControlClient&) = delete;

  /// Record the socket path without blocking; the next request connects.
  void set_path(std::filesystem::path path);
  /// Connect, retrying until the deadline because the plugin binds after boot.
  Result<void> connect(const std::filesystem::path& path, std::chrono::milliseconds timeout);
  void close();
  [[nodiscard]] bool connected() const;
  [[nodiscard]] const std::filesystem::path& path() const { return path_; }

  /// Send one request and return its result object, or an Error naming the
  /// layer that failed. A dropped connection closes the client for a retry.
  Result<nlohmann::json> request(std::string_view method, const nlohmann::json& params,
                                 std::chrono::milliseconds timeout);

 private:
  Result<void> connect_once(std::chrono::milliseconds timeout);
  Result<void> send_line(const std::string& line);
  Result<std::string> read_line(std::chrono::milliseconds timeout);

  int fd_{-1};
  std::filesystem::path path_;
  std::uint64_t next_id_{1};
  std::string buffer_;
  mutable std::mutex mutex_;
};

/// The control API's error text, preserving the remote code when present.
std::string control_error_text(const nlohmann::json& error);

}  // namespace booth
