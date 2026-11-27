#include <catch2/catch_test_macros.hpp>

#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

#include <atomic>
#include <chrono>
#include <cstring>
#include <filesystem>
#include <string>
#include <thread>

#include <nlohmann/json.hpp>

#include "control/client.hpp"

using namespace std::chrono_literals;

namespace {

class FakeControlServer {
 public:
  explicit FakeControlServer(std::filesystem::path path) : path_(std::move(path)) {
    ::unlink(path_.c_str());
    listener_ = ::socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0);
    REQUIRE(listener_ >= 0);
    sockaddr_un address{};
    address.sun_family = AF_UNIX;
    REQUIRE(path_.string().size() < sizeof(address.sun_path));
    std::memcpy(address.sun_path, path_.c_str(), path_.string().size());
    REQUIRE(::bind(listener_, reinterpret_cast<sockaddr*>(&address), sizeof(address)) == 0);
    REQUIRE(::listen(listener_, 2) == 0);
    thread_ = std::thread([this] { serve(); });
  }

  ~FakeControlServer() {
    stopping_ = true;
    ::shutdown(listener_, SHUT_RDWR);
    ::close(listener_);
    if (thread_.joinable()) thread_.join();
    ::unlink(path_.c_str());
  }

 private:
  void serve() {
    const int client = ::accept(listener_, nullptr, nullptr);
    if (client < 0) return;
    std::string buffer;
    char chunk[1024];
    while (!stopping_) {
      const ssize_t size = ::recv(client, chunk, sizeof(chunk), 0);
      if (size <= 0) break;
      buffer.append(chunk, static_cast<std::size_t>(size));
      for (;;) {
        const auto newline = buffer.find('\n');
        if (newline == std::string::npos) break;
        const auto line = buffer.substr(0, newline);
        buffer.erase(0, newline + 1);
        if (line.empty()) continue;
        auto request = nlohmann::json::parse(line, nullptr, false);
        if (request.is_discarded()) continue;
        nlohmann::json response = {{"id", request["id"]}};
        if (request.value("method", "") == "/fail") {
          response["error"] = {{"code", "control/denied"}, {"message", "no"}};
        } else {
          response["result"] = {{"echo", request.value("method", "")},
                                {"params", request.value("params", nlohmann::json::object())}};
        }
        const std::string out = response.dump() + "\n";
        (void)::send(client, out.data(), out.size(), MSG_NOSIGNAL);
      }
    }
    ::close(client);
  }

  std::filesystem::path path_;
  int listener_{-1};
  std::atomic<bool> stopping_{false};
  std::thread thread_;
};

}  // namespace

TEST_CASE("control client round-trips and preserves remote errors") {
  const auto dir = std::filesystem::temp_directory_path() / "booth-control-test";
  std::filesystem::create_directories(dir);
  FakeControlServer server(dir / "control.sock");
  booth::ControlClient client;
  auto connected = client.connect(dir / "control.sock", 2s);
  REQUIRE(connected.has_value());
  REQUIRE(client.connected());

  auto result = client.request("session.info", {{"sessionId", "s-1"}}, 2s);
  REQUIRE(result.has_value());
  REQUIRE((*result)["echo"] == "session.info");
  REQUIRE((*result)["params"]["sessionId"] == "s-1");

  auto refused = client.request("/fail", nlohmann::json::object(), 2s);
  REQUIRE_FALSE(refused.has_value());
  REQUIRE(refused.error().layer == booth::Layer::Protocol);
  REQUIRE(refused.error().detail == "control/denied: no");
}

TEST_CASE("control client reports a missing socket") {
  booth::ControlClient client;
  auto connected = client.connect("/nonexistent/booth-control.sock", 100ms);
  REQUIRE_FALSE(connected.has_value());
  REQUIRE(connected.error().layer == booth::Layer::Transport);
}
