#pragma once

#include <atomic>
#include <chrono>
#include <condition_variable>
#include <cstdint>
#include <functional>
#include <memory>
#include <mutex>
#include <optional>
#include <string>
#include <string_view>
#include <thread>
#include <unordered_map>

#include "contract/error.hpp"
#include "contract/event.hpp"
#include "subagent/acp/codec.hpp"

namespace booth::acp {

/// One ACP connection over a bidirectional pipe pair. The reader owns a thread; every
/// notification and permission request is delivered through `EventSink`.
class Client {
 public:
  using EventSink = std::function<void(Event)>;

  Client(int input_fd, int output_fd, EventSink sink);
  ~Client();

  Client(const Client&) = delete;
  Client& operator=(const Client&) = delete;

  void start();
  void stop();

  Result<Json> request(std::string_view method, const Json& params,
                       std::chrono::milliseconds timeout);
  void notify(std::string_view method, const Json& params);

  /// Answer one server-to-client permission request by JSON-RPC id. No option means cancelled.
  Result<void> answer_permission(std::string_view request_id,
                                 const std::optional<std::string>& option_id);

  /// Take every incoming frame instead of decoding it. Used by the daemon's proxy.
  void set_raw_sink(std::function<void(const Json&)> sink);
  /// Write one frame verbatim.
  void send_raw(const Json& message);

 private:
  struct Pending {
    std::mutex mutex;
    std::condition_variable ready;
    bool done{false};
    Json result;
    std::optional<Error> error;
  };

  void reader_loop();
  void handle_line(std::string_view line);
  void dispatch(const Json& message);
  void handle_notification(const Json& message);
  void handle_server_request(const Json& message);
  void resolve(const Json& id, const Json& result, const std::optional<Error>& error);
  void emit(Event event);
  bool write_json(const Json& message);
  void fail_pending(const Error& error);

  int input_fd_;
  int output_fd_;
  int wake_read_{-1};
  int wake_write_{-1};
  EventSink sink_;
  std::mutex raw_mutex_;
  std::function<void(const Json&)> raw_sink_;
  std::atomic<bool> stopping_{false};
  std::atomic<std::uint64_t> next_id_{1};
  std::mutex write_mutex_;
  std::mutex pending_mutex_;
  std::unordered_map<std::string, std::shared_ptr<Pending>> pending_;
  std::mutex permission_mutex_;
  std::unordered_map<std::string, Json> permission_ids_;
  std::thread reader_;
  bool started_{false};
};

}  // namespace booth::acp
