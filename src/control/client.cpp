#include "control/client.hpp"

#include <poll.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

#include <cerrno>
#include <cstring>
#include <thread>

namespace booth {
namespace {

Error transport_error(std::string detail) {
  return Error{Layer::Transport, std::move(detail), std::nullopt};
}

std::string errno_text(std::string_view prefix, int number) {
  std::string out(prefix);
  out += ": ";
  out += std::strerror(number);
  return out;
}

}  // namespace

std::string control_error_text(const nlohmann::json& error) {
  if (!error.is_object()) return error.dump();
  const std::string code = error.value("code", std::string{});
  const std::string message = error.value("message", std::string{});
  if (code.empty()) return message.empty() ? error.dump() : message;
  return code + ": " + message;
}

ControlClient::~ControlClient() { close(); }

void ControlClient::set_path(std::filesystem::path path) {
  std::lock_guard<std::mutex> lock(mutex_);
  path_ = std::move(path);
}

Result<void> ControlClient::connect(const std::filesystem::path& path,
                                    std::chrono::milliseconds timeout) {
  std::lock_guard<std::mutex> lock(mutex_);
  path_ = path;
  if (fd_ >= 0) return {};
  const auto deadline = std::chrono::steady_clock::now() + timeout;
  for (;;) {
    auto connected = connect_once(std::chrono::milliseconds(500));
    if (connected) return {};
    if (std::chrono::steady_clock::now() >= deadline) return connected;
    std::this_thread::sleep_for(std::chrono::milliseconds(50));
  }
}

Result<void> ControlClient::connect_once(std::chrono::milliseconds timeout) {
  (void)timeout;
  if (path_.empty()) return std::unexpected(transport_error("no control socket configured"));
  const std::string path = path_.string();
  if (path.size() >= sizeof(sockaddr_un::sun_path)) {
    return std::unexpected(transport_error("control socket path is too long: " + path));
  }
  const int fd = ::socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0);
  if (fd < 0) return std::unexpected(transport_error(errno_text("cannot create control socket", errno)));

  sockaddr_un address{};
  address.sun_family = AF_UNIX;
  std::memcpy(address.sun_path, path.c_str(), path.size());
  if (::connect(fd, reinterpret_cast<sockaddr*>(&address), sizeof(address)) != 0) {
    const int saved = errno;
    ::close(fd);
    return std::unexpected(transport_error(errno_text("cannot connect to " + path, saved)));
  }
  fd_ = fd;
  buffer_.clear();
  return {};
}

void ControlClient::close() {
  std::lock_guard<std::mutex> lock(mutex_);
  if (fd_ >= 0) {
    ::close(fd_);
    fd_ = -1;
  }
  buffer_.clear();
}

bool ControlClient::connected() const {
  std::lock_guard<std::mutex> lock(mutex_);
  return fd_ >= 0;
}

Result<void> ControlClient::send_line(const std::string& line) {
  std::size_t sent = 0;
  while (sent < line.size()) {
    const ssize_t written = ::send(fd_, line.data() + sent, line.size() - sent, MSG_NOSIGNAL);
    if (written < 0) {
      if (errno == EINTR) continue;
      return std::unexpected(transport_error(errno_text("cannot write to the control socket", errno)));
    }
    sent += static_cast<std::size_t>(written);
  }
  return {};
}

Result<std::string> ControlClient::read_line(std::chrono::milliseconds timeout) {
  const auto deadline = std::chrono::steady_clock::now() + timeout;
  for (;;) {
    const auto newline = buffer_.find('\n');
    if (newline != std::string::npos) {
      std::string line = buffer_.substr(0, newline);
      buffer_.erase(0, newline + 1);
      return line;
    }
    const auto remaining = std::chrono::duration_cast<std::chrono::milliseconds>(
        deadline - std::chrono::steady_clock::now());
    if (remaining.count() <= 0) {
      return std::unexpected(transport_error("control request timed out"));
    }
    pollfd poll_fd{fd_, POLLIN, 0};
    const int ready = ::poll(&poll_fd, 1, static_cast<int>(remaining.count()));
    if (ready < 0) {
      if (errno == EINTR) continue;
      return std::unexpected(transport_error(errno_text("cannot poll the control socket", errno)));
    }
    if (ready == 0) {
      return std::unexpected(transport_error("control request timed out"));
    }
    char chunk[4096];
    const ssize_t read_bytes = ::recv(fd_, chunk, sizeof(chunk), 0);
    if (read_bytes < 0) {
      if (errno == EINTR) continue;
      return std::unexpected(transport_error(errno_text("cannot read the control socket", errno)));
    }
    if (read_bytes == 0) {
      return std::unexpected(transport_error("the control socket closed"));
    }
    buffer_.append(chunk, static_cast<std::size_t>(read_bytes));
  }
}

Result<nlohmann::json> ControlClient::request(std::string_view method,
                                              const nlohmann::json& params,
                                              std::chrono::milliseconds timeout) {
  std::lock_guard<std::mutex> lock(mutex_);
  if (fd_ < 0) {
    auto connected = connect_once(std::chrono::milliseconds(500));
    if (!connected) return std::unexpected(connected.error());
  }
  const auto drop = [this] {
    if (fd_ >= 0) {
      ::close(fd_);
      fd_ = -1;
    }
    buffer_.clear();
  };
  const std::uint64_t id = next_id_++;
  nlohmann::json message = {{"id", id},
                            {"method", std::string(method)},
                            {"params", params}};
  auto sent = send_line(message.dump() + "\n");
  if (!sent) {
    drop();
    return std::unexpected(sent.error());
  }

  const auto deadline = std::chrono::steady_clock::now() + timeout;
  for (;;) {
    const auto remaining = std::chrono::duration_cast<std::chrono::milliseconds>(
        deadline - std::chrono::steady_clock::now());
    if (remaining.count() <= 0) {
      drop();
      return std::unexpected(transport_error("control request timed out"));
    }
    auto line = read_line(remaining);
    if (!line) {
      drop();
      return std::unexpected(line.error());
    }
    auto parsed = nlohmann::json::parse(*line, nullptr, false);
    if (parsed.is_discarded() || !parsed.is_object()) continue;
    if (!parsed.contains("id") || !parsed["id"].is_number_unsigned() ||
        parsed["id"].get<std::uint64_t>() != id) {
      continue;
    }
    if (parsed.contains("error")) {
      return std::unexpected(Error{Layer::Protocol, control_error_text(parsed["error"]), std::nullopt});
    }
    if (!parsed.contains("result")) return nlohmann::json{};
    return parsed["result"];
  }
}

}  // namespace booth
