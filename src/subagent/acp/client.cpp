#include "subagent/acp/client.hpp"

#include <fcntl.h>
#include <poll.h>
#include <unistd.h>

#include <array>
#include <cerrno>
#include <cstddef>
#include <string>
#include <utility>

#include "subagent/acp/messages.hpp"

namespace booth::acp {
namespace {

bool write_all(int fd, std::string_view bytes) {
  while (!bytes.empty()) {
    const ssize_t written = ::write(fd, bytes.data(), bytes.size());
    if (written < 0) {
      if (errno == EINTR) continue;
      return false;
    }
    bytes.remove_prefix(static_cast<std::size_t>(written));
  }
  return true;
}

std::string method_of(const Json& message) { return message.value("method", std::string{}); }

Error error_of(const Json& error) {
  Error out{Layer::Kernel, "kernel error", std::nullopt};
  if (error.is_object()) {
    out.detail = error.value("message", std::string{"kernel error"});
    if (error.contains("code") && error["code"].is_number_integer()) {
      out.status = error["code"].get<int>();
    }
  } else {
    out.detail = error.dump();
  }
  return out;
}

}  // namespace

Client::Client(int input_fd, int output_fd, EventSink sink)
    : input_fd_(input_fd), output_fd_(output_fd), sink_(std::move(sink)) {
  int fds[2] = {-1, -1};
  if (::pipe(fds) == 0) {
    wake_read_ = fds[0];
    wake_write_ = fds[1];
    for (const int fd : fds) {
      ::fcntl(fd, F_SETFD, FD_CLOEXEC);
    }
  }
}

Client::~Client() {
  stop();
  if (reader_.joinable()) reader_.join();
  if (wake_read_ >= 0) ::close(wake_read_);
  if (wake_write_ >= 0) ::close(wake_write_);
}

void Client::start() {
  if (started_) return;
  started_ = true;
  reader_ = std::thread([this] { reader_loop(); });
}

void Client::stop() {
  if (stopping_.exchange(true)) return;
  if (wake_write_ >= 0) {
    const char byte = 1;
    const ssize_t ignored = ::write(wake_write_, &byte, 1);
    (void)ignored;
  }
}

void Client::emit(Event event) {
  if (sink_) sink_(std::move(event));
}

bool Client::write_json(const Json& message) {
  const std::string bytes = encode(message);
  std::lock_guard<std::mutex> lock(write_mutex_);
  return write_all(output_fd_, bytes);
}

Result<Json> Client::request(std::string_view method, const Json& params,
                             std::chrono::milliseconds timeout) {
  const auto id = next_id_.fetch_add(1);
  auto pending = std::make_shared<Pending>();
  const std::string key = std::to_string(id);
  {
    std::lock_guard<std::mutex> lock(pending_mutex_);
    pending_[key] = pending;
  }
  Json message{{"jsonrpc", "2.0"},
               {"id", id},
               {"method", std::string(method)},
               {"params", params}};
  if (!write_json(message)) {
    std::lock_guard<std::mutex> lock(pending_mutex_);
    pending_.erase(key);
    return std::unexpected(Error{Layer::Transport, "cannot write to the kernel", std::nullopt});
  }

  std::unique_lock<std::mutex> lock(pending->mutex);
  if (!pending->ready.wait_for(lock, timeout, [&] { return pending->done; })) {
    lock.unlock();
    std::lock_guard<std::mutex> guard(pending_mutex_);
    pending_.erase(key);
    return std::unexpected(Error{Layer::Transport, "ACP request timed out", std::nullopt});
  }
  if (pending->error) return std::unexpected(*pending->error);
  return pending->result;
}

void Client::notify(std::string_view method, const Json& params) {
  Json message{{"jsonrpc", "2.0"}, {"method", std::string(method)}, {"params", params}};
  if (!write_json(message)) {
    emit(Event{Problem{Layer::Transport, "cannot write to the kernel"}});
  }
}

Result<void> Client::answer_permission(std::string_view request_id,
                                       const std::optional<std::string>& option_id) {
  Json rpc_id;
  {
    std::lock_guard<std::mutex> lock(permission_mutex_);
    const auto found = permission_ids_.find(std::string(request_id));
    if (found == permission_ids_.end()) {
      return std::unexpected(Error{Layer::Protocol, "unknown permission request", std::nullopt});
    }
    rpc_id = found->second;
    permission_ids_.erase(found);
  }
  Json message{{"jsonrpc", "2.0"}, {"id", rpc_id}, {"result", permission_result(option_id)}};
  if (!write_json(message)) {
    return std::unexpected(Error{Layer::Transport, "cannot write to the kernel", std::nullopt});
  }
  return {};
}

void Client::reader_loop() {
  std::string buffer;
  std::array<char, 65536> chunk{};
  bool clean = true;

  for (;;) {
    pollfd fds[2] = {{input_fd_, POLLIN, 0}, {wake_read_, POLLIN, 0}};
    const nfds_t count = wake_read_ >= 0 ? 2 : 1;
    const int ready = ::poll(fds, count, -1);
    if (ready < 0) {
      if (errno == EINTR) continue;
      clean = false;
      break;
    }
    if (count == 2 && (fds[1].revents & POLLIN) != 0) {
      clean = true;
      break;
    }
    if ((fds[0].revents & (POLLIN | POLLHUP | POLLERR)) == 0) continue;

    const ssize_t got = ::read(input_fd_, chunk.data(), chunk.size());
    if (got < 0) {
      if (errno == EINTR) continue;
      if (errno == EAGAIN || errno == EWOULDBLOCK) continue;
      clean = false;
      break;
    }
    if (got == 0) break;  // EOF
    buffer.append(chunk.data(), static_cast<std::size_t>(got));

    std::size_t newline = 0;
    while ((newline = buffer.find('\n')) != std::string::npos) {
      std::string line = buffer.substr(0, newline);
      buffer.erase(0, newline + 1);
      handle_line(line);
      if (stopping_) break;
    }
    if (buffer.size() > kMaxLine) {
      emit(Event{Problem{Layer::Protocol, "unterminated frame exceeds the limit"}});
      clean = false;
      break;
    }
    if (stopping_) break;
  }

  const bool stopped = stopping_.load();
  if (!clean && !stopped) {
    emit(Event{Problem{Layer::Kernel, "kernel connection closed"}});
  }
  fail_pending(Error{Layer::Kernel, "kernel connection closed", std::nullopt});
}

void Client::handle_line(std::string_view line) {
  auto decoded = decode(line);
  if (!decoded) {
    // dsh plugins can log to stdout before the ACP app takes over the stream. A log line is
    // not a protocol failure; drop it and keep reading frames.
    if (decoded.error().detail == "frame is not JSON" ||
        decoded.error().detail == "empty frame") {
      return;
    }
    emit(Event{Problem{decoded.error().layer, decoded.error().detail}});
    stop();
    return;
  }
  dispatch(*decoded);
}

void Client::set_raw_sink(std::function<void(const Json&)> sink) {
  std::lock_guard<std::mutex> lock(raw_mutex_);
  raw_sink_ = std::move(sink);
}

void Client::send_raw(const Json& message) {
  if (!write_json(message)) {
    emit(Event{Problem{Layer::Transport, "cannot write to the kernel"}});
  }
}

void Client::dispatch(const Json& message) {
  std::function<void(const Json&)> raw;
  {
    std::lock_guard<std::mutex> lock(raw_mutex_);
    raw = raw_sink_;
  }
  if (raw) {
    raw(message);
    return;
  }
  const bool has_method = message.contains("method");
  const bool has_id = message.contains("id");

  if (has_method && has_id) {
    handle_server_request(message);
    return;
  }
  if (has_method) {
    handle_notification(message);
    return;
  }
  if (has_id) {
    if (message.contains("error")) {
      resolve(message["id"], Json{}, error_of(message["error"]));
    } else {
      resolve(message["id"], message.value("result", Json::object()), std::nullopt);
    }
  }
}

void Client::handle_notification(const Json& message) {
  if (method_of(message) != "session/update") return;
  const auto params = message.value("params", Json::object());
  for (auto& event : session_updates(params)) {
    emit(std::move(event));
  }
}

void Client::handle_server_request(const Json& message) {
  const std::string method = method_of(message);
  if (method != "session/request_permission") {
    Json response{{"jsonrpc", "2.0"},
                  {"id", message["id"]},
                  {"error",
                   Json{{"code", -32601}, {"message", "method not found"}}}};
    write_json(response);
    return;
  }

  const std::string key = id_key(message["id"]);
  auto parsed = permission_request(message.value("params", Json::object()), key);
  if (!parsed) {
    Json response{{"jsonrpc", "2.0"},
                  {"id", message["id"]},
                  {"error",
                   Json{{"code", -32602}, {"message", "invalid permission request"}}}};
    write_json(response);
    return;
  }

  {
    std::lock_guard<std::mutex> lock(permission_mutex_);
    permission_ids_[key] = message["id"];
  }
  emit(Event{std::move(*parsed)});
}

void Client::resolve(const Json& id, const Json& result, const std::optional<Error>& error) {
  const std::string key = id_key(id);
  std::shared_ptr<Pending> pending;
  {
    std::lock_guard<std::mutex> lock(pending_mutex_);
    const auto found = pending_.find(key);
    if (found == pending_.end()) return;
    pending = found->second;
    pending_.erase(found);
  }
  {
    std::lock_guard<std::mutex> lock(pending->mutex);
    pending->result = result;
    pending->error = error;
    pending->done = true;
  }
  pending->ready.notify_all();
}

void Client::fail_pending(const Error& error) {
  std::unordered_map<std::string, std::shared_ptr<Pending>> pending;
  {
    std::lock_guard<std::mutex> lock(pending_mutex_);
    pending.swap(pending_);
  }
  for (auto& [key, item] : pending) {
    (void)key;
    {
      std::lock_guard<std::mutex> lock(item->mutex);
      item->error = error;
      item->done = true;
    }
    item->ready.notify_all();
  }
}

}  // namespace booth::acp
