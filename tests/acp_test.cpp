#include <catch2/catch_test_macros.hpp>

#include <sys/socket.h>
#include <unistd.h>

#include <chrono>
#include <condition_variable>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

#include "subagent/acp/client.hpp"
#include "subagent/acp/codec.hpp"
#include "subagent/acp/messages.hpp"

using namespace booth;
using namespace booth::acp;

namespace {

std::string read_line(int fd) {
  std::string out;
  char byte = 0;
  while (true) {
    const ssize_t got = ::read(fd, &byte, 1);
    if (got <= 0) return {};
    if (byte == '\n') return out;
    out.push_back(byte);
  }
}

bool send_all(int fd, const std::string& bytes) {
  std::size_t sent = 0;
  while (sent < bytes.size()) {
    const ssize_t wrote = ::write(fd, bytes.data() + sent, bytes.size() - sent);
    if (wrote <= 0) return false;
    sent += static_cast<std::size_t>(wrote);
  }
  return true;
}

}  // namespace

TEST_CASE("the codec round-trips one frame and refuses junk") {
  const Json message{{"jsonrpc", "2.0"}, {"id", 7}, {"result", Json::object()}};
  auto decoded = decode(encode(message));
  REQUIRE(decoded.has_value());
  REQUIRE((*decoded)["id"] == 7);

  REQUIRE_FALSE(decode("").has_value());
  REQUIRE_FALSE(decode("not json").has_value());
  REQUIRE_FALSE(decode("[1,2,3]").has_value());
  REQUIRE(decode("{\"a\":1}\r\n").has_value());
}

TEST_CASE("message builders match the ACP shapes") {
  const auto init = initialize_params("1.2.3");
  REQUIRE(init["protocolVersion"] == 1);
  REQUIRE(init["clientInfo"]["name"] == "booth");

  const auto session = new_session_params("/tmp/work");
  REQUIRE(session["cwd"] == "/tmp/work");
  REQUIRE(session["mcpServers"].is_array());

  const auto prompt = prompt_params("s-1", "hello");
  REQUIRE(prompt["sessionId"] == "s-1");
  REQUIRE(prompt["prompt"][0]["type"] == "text");
  REQUIRE(prompt["prompt"][0]["text"] == "hello");

  const auto with_image = prompt_params(
      "s-1", "look", {}, {ImageAttachment{"image/png", "aGVsbG8=", "clipboard.png"}});
  REQUIRE(with_image["prompt"][1]["type"] == "image");
  REQUIRE(with_image["prompt"][1]["mimeType"] == "image/png");
  REQUIRE(with_image["prompt"][1]["data"] == "aGVsbG8=");
  REQUIRE(with_image["prompt"][1]["name"] == "clipboard.png");

  REQUIRE(set_config_params("s-1", "model", "m-1")["value"] == "m-1");
  REQUIRE(cancel_params("s-1")["sessionId"] == "s-1");

  REQUIRE(permission_result(std::nullopt)["outcome"]["outcome"] == "cancelled");
  REQUIRE(permission_result(std::string{"allow"})["outcome"]["optionId"] == "allow");
}

TEST_CASE("session updates become contract events") {
  const Json params{
      {"sessionId", "s-1"},
      {"update",
       Json{{"sessionUpdate", "agent_message_chunk"},
            {"content", Json{{"type", "text"}, {"text", "hello"}}}}}};
  auto events = session_updates(params);
  REQUIRE(events.size() == 1);
  REQUIRE(std::holds_alternative<StreamDelta>(events[0].value));
  REQUIRE(std::get<StreamDelta>(events[0].value).text == "hello");

  const Json tool{
      {"sessionId", "s-1"},
      {"update", Json{{"sessionUpdate", "tool_call"},
                      {"toolCallId", "t-1"},
                      {"title", "subagent: fix the build"},
                      {"name", "subagent"},
                      {"status", "pending"}}}};
  auto tool_events = session_updates(tool);
  REQUIRE(tool_events.size() == 1);
  REQUIRE(std::holds_alternative<TranscriptEntry>(tool_events[0].value));
  REQUIRE(std::get<TranscriptEntry>(tool_events[0].value).id == "t-1");

  const Json config{
      {"sessionId", "s-1"},
      {"update", Json{{"sessionUpdate", "config_option_update"},
                      {"configOptions",
                       Json::array({Json{{"id", "model"},
                                         {"name", "Model"},
                                         {"category", "model"},
                                         {"currentValue", "deepseek-v4-pro"}}})}}}};
  auto config_events = session_updates(config);
  REQUIRE(config_events.size() == 1);
  REQUIRE(std::get<SessionConfigChanged>(config_events[0].value).value == "deepseek-v4-pro");

  const Json usage{
      {"sessionId", "s-1"},
      {"update", Json{{"sessionUpdate", "usage_update"}, {"used", 12}, {"size", 128}}}};
  auto usage_events = session_updates(usage);
  REQUIRE(std::get<UsageChanged>(usage_events[0].value).size == 128);
}

TEST_CASE("the client correlates responses, notifications and permissions") {
  int fds[2] = {-1, -1};
  REQUIRE(::socketpair(AF_UNIX, SOCK_STREAM, 0, fds) == 0);

  std::mutex mutex;
  std::condition_variable ready;
  std::vector<Event> events;
  bool answered = false;
  std::string outcome;

  Client client(fds[0], fds[0], [&](Event event) {
    std::lock_guard<std::mutex> lock(mutex);
    events.push_back(std::move(event));
    ready.notify_all();
  });
  client.start();

  bool server_ok = true;
  std::thread server([&] {
    const auto request = decode(read_line(fds[1]));
    if (!request) { server_ok = false; return; }
    const Json response{{"jsonrpc", "2.0"},
                        {"id", (*request)["id"]},
                        {"result", Json{{"protocolVersion", 1}}}};
    server_ok = send_all(fds[1], encode(response));

    const Json notification{
        {"jsonrpc", "2.0"},
        {"method", "session/update"},
        {"params", Json{{"sessionId", "s-1"},
                        {"update", Json{{"sessionUpdate", "agent_message_chunk"},
                                        {"content", Json{{"type", "text"}, {"text", "hi"}}}}}}}};
    server_ok = server_ok && send_all(fds[1], encode(notification));

    const Json permission{
        {"jsonrpc", "2.0"},
        {"id", "perm-1"},
        {"method", "session/request_permission"},
        {"params",
         Json{{"sessionId", "s-1"},
              {"toolCall", Json{{"toolCallId", "t-1"}, {"title", "bash"},
                                {"rawInput", Json{{"command", "ls"}}}}},
              {"options", Json::array({Json{{"optionId", "allow"},
                                            {"name", "Allow once"},
                                            {"kind", "allow_once"}}})}}}};
    server_ok = server_ok && send_all(fds[1], encode(permission));

    const auto answer = decode(read_line(fds[1]));
    {
      std::lock_guard<std::mutex> lock(mutex);
      answered = answer.has_value();
      if (answer) outcome = (*answer)["result"]["outcome"]["outcome"];
      ready.notify_all();
    }
  });

  auto init = client.request("initialize", initialize_params("test"), std::chrono::seconds(2));
  REQUIRE(init.has_value());
  REQUIRE((*init)["protocolVersion"] == 1);

  {
    std::unique_lock<std::mutex> lock(mutex);
    REQUIRE(ready.wait_for(lock, std::chrono::seconds(2),
                           [&] { return events.size() >= 2; }));
  }
  REQUIRE(std::holds_alternative<StreamDelta>(events[0].value));
  REQUIRE(std::holds_alternative<PermissionRequest>(events[1].value));
  const auto& permission = std::get<PermissionRequest>(events[1].value);
  REQUIRE(permission.options.front().id == "allow");

  REQUIRE(client.answer_permission(permission.id.value, std::string{"allow"}).has_value());
  {
    std::unique_lock<std::mutex> lock(mutex);
    REQUIRE(ready.wait_for(lock, std::chrono::seconds(2), [&] { return answered; }));
  }
  REQUIRE(outcome == "selected");

  client.stop();
  ::close(fds[1]);
  server.join();
  ::close(fds[0]);
}

TEST_CASE("a request that never answers times out") {
  int fds[2] = {-1, -1};
  REQUIRE(::socketpair(AF_UNIX, SOCK_STREAM, 0, fds) == 0);
  Client client(fds[0], fds[0], [](Event) {});
  client.start();

  auto result = client.request("initialize", Json::object(), std::chrono::milliseconds(50));
  REQUIRE_FALSE(result.has_value());
  REQUIRE(result.error().layer == Layer::Transport);

  client.stop();
  ::close(fds[1]);
  ::close(fds[0]);
}
