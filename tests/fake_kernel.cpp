// A minimal ACP peer for tests: answers the calls booth makes and streams one message.
#include <nlohmann/json.hpp>

#include <iostream>
#include <optional>
#include <string>

namespace {

using json = nlohmann::json;

void send(const json& message) { std::cout << message.dump() << '\n' << std::flush; }

void reply(const json& id, const json& result) {
  send({{"jsonrpc", "2.0"}, {"id", id}, {"result", result}});
}

void stream_text(const std::string& text) {
  send({{"jsonrpc", "2.0"},
        {"method", "session/update"},
        {"params",
         {{"sessionId", "s-1"},
          {"update",
           {{"sessionUpdate", "agent_message_chunk"},
            {"content", {{"type", "text"}, {"text", text}}}}}}}});
}

json prompt_text(const json& message) {
  const auto prompt = message["params"].value("prompt", json::array());
  if (prompt.empty()) return json{};
  return prompt[0].value("text", "");
}

std::string prompt_image(const json& message) {
  for (const auto& block : message["params"].value("prompt", json::array())) {
    if (block.value("type", std::string{}) == "image") {
      return block.value("mimeType", std::string{});
    }
  }
  return {};
}

std::string prompt_resource(const json& message) {
  for (const auto& block : message["params"].value("prompt", json::array())) {
    if (block.value("type", std::string{}) == "resource_link") {
      return block.value("name", std::string{});
    }
  }
  return {};
}

}  // namespace

int main() {
  std::optional<json> waiting_for_cancel;
  std::optional<json> waiting_for_permission;

  // A plugin may log to stdout before the ACP app owns it; the client must skip such lines.
  std::cout << "[fake] starting\n" << std::flush;

  std::string line;
  while (std::getline(std::cin, line)) {
    auto message = json::parse(line, nullptr, false);
    if (message.is_discarded() || !message.is_object()) continue;

    if (message.contains("method") && !message.contains("id")) {
      if (message.value("method", "") == "session/cancel" && waiting_for_cancel) {
        reply(*waiting_for_cancel, {{"stopReason", "cancelled"}});
        waiting_for_cancel.reset();
      }
      continue;
    }

    if (!message.contains("method") && message.contains("id")) {
      if (waiting_for_permission && message["id"] == "perm-1") {
        const std::string option =
            message.value("result", json::object())
                .value("outcome", json::object())
                .value("optionId", std::string{});
        stream_text(option == "allow-always" ? "always-ok" : "hello");
        reply(*waiting_for_permission, {{"stopReason", "end_turn"}});
        waiting_for_permission.reset();
      }
      continue;
    }

    if (!message.contains("method") || !message.contains("id")) continue;
    const std::string method = message.value("method", "");
    const json id = message["id"];

    if (method == "initialize") {
      reply(id, {{"protocolVersion", 1}});
    } else if (method == "session/new") {
      const json config_options = json::parse(R"([
        {"id":"model","name":"Model","category":"model",
         "currentValue":"[\"provider\",\"model-one\"]",
         "options":[{"group":"provider","name":"Provider","options":[
           {"value":"[\"provider\",\"model-one\"]","name":"model-one"},
           {"value":"[\"provider\",\"model-two\"]","name":"model-two"}]},
          {"group":"other","name":"Other","options":[
           {"value":"[\"other\",\"model-three\"]","name":"model-three"}]}]},
        {"id":"reasoning_effort","name":"Reasoning effort","category":"thought_level",
         "currentValue":"high",
         "options":[{"value":"low","name":"Low"},{"value":"high","name":"High"}]}
      ])");
      reply(id, {{"sessionId", "s-1"}, {"configOptions", config_options}});
    } else if (method == "session/set_config_option") {
      reply(id, {{"configOptions", json::array()}});
    } else if (method == "session/list") {
      const std::string cursor =
          message.value("params", json::object()).value("cursor", std::string{});
      if (cursor.empty()) {
        reply(id, {{"sessions", json::array({json{{"sessionId", "s-old"}, {"cwd", "/tmp"}},
                                             json{{"sessionId", "s-older"}, {"cwd", "/tmp"}}})},
                   {"nextCursor", "page-2"}});
      } else {
        reply(id, {{"sessions", json::array({json{{"sessionId", "s-oldest"}, {"cwd", "/tmp"}}})}});
      }
    } else if (method == "session/close") {
      reply(id, json::object());
    } else if (method == "session/resume") {
      reply(id, {{"configOptions", json::array({json{
                      {"id", "model"},
                      {"name", "Model"},
                      {"category", "model"},
                      {"currentValue", "[\"provider\",\"model-two\"]"},
                      {"options", json::array({json{
                          {"group", "provider"},
                          {"name", "Provider"},
                          {"options", json::array({
                              json{{"value", "[\"provider\",\"model-two\"]"},
                                   {"name", "model-two"}},
                          })},
                      }})},
                  }})}});
    } else if (method == "session/prompt") {
      const std::string text = prompt_text(message);
      const std::string resource = prompt_resource(message);
      const std::string image = prompt_image(message);
      if (!image.empty()) stream_text("image:" + image);
      if (text.find("<model agent=") != std::string::npos) stream_text("mention-tag");
      if (!resource.empty()) stream_text("resource:" + resource);
      if (text.find("usage") != std::string::npos) {
        send({{"jsonrpc", "2.0"}, {"method", "session/update"},
              {"params", {{"sessionId", "s-1"},
                          {"update", {{"sessionUpdate", "usage_update"},
                                      {"used", 1200}, {"size", 4800}}}}}});
      }
      if (text.find("slow") != std::string::npos) {
        waiting_for_cancel = id;
        continue;
      }
      if (text.find("edit") != std::string::npos) {
        send({{"jsonrpc", "2.0"}, {"method", "session/update"},
              {"params", {{"sessionId", "s-1"},
                          {"update", {{"sessionUpdate", "tool_call"},
                                      {"toolCallId", "t-edit"},
                                      {"title", "Edit file"},
                                      {"name", "edit"},
                                      {"status", "pending"},
                                      {"rawInput", {{"file_path", "README.md"},
                                                    {"old_string", "old line"},
                                                    {"new_string", "new line"}}}}}}}});
        send({{"jsonrpc", "2.0"}, {"method", "session/update"},
              {"params", {{"sessionId", "s-1"},
                          {"update", {{"sessionUpdate", "tool_call_update"},
                                      {"toolCallId", "t-edit"},
                                      {"status", "completed"}}}}}});
        stream_text("Done.");
        reply(id, {{"stopReason", "end_turn"}});
        continue;
      }
      if (text.find("longtool") != std::string::npos) {
        std::string lines;
        for (int index = 1; index <= 20; ++index) {
          if (!lines.empty()) lines += '\n';
          lines += "line " + (index < 10 ? std::string("0") : std::string()) + std::to_string(index);
        }
        send({{"jsonrpc", "2.0"}, {"method", "session/update"},
              {"params", {{"sessionId", "s-1"},
                          {"update", {{"sessionUpdate", "tool_call"},
                                      {"toolCallId", "t-long"},
                                      {"title", "Long output"},
                                      {"name", "bash"},
                                      {"status", "pending"},
                                      {"rawInput", {{"command", "seq 20"}}}}}}}});
        send({{"jsonrpc", "2.0"}, {"method", "session/update"},
              {"params", {{"sessionId", "s-1"},
                          {"update", {{"sessionUpdate", "tool_call_update"},
                                      {"toolCallId", "t-long"},
                                      {"status", "completed"},
                                      {"content", json::array({json{{"type", "content"},
                                                                    {"content", {{"type", "text"}, {"text", lines}}}}})}}}}}});
        stream_text("Done.");
        reply(id, {{"stopReason", "end_turn"}});
        continue;
      }
      if (text.find("think") != std::string::npos) {
        send({{"jsonrpc", "2.0"}, {"method", "session/update"},
              {"params", {{"sessionId", "s-1"},
                          {"update", {{"sessionUpdate", "agent_thought_chunk"},
                                      {"content", {{"type", "text"}, {"text", "hidden-thought"}}}}}}}});
        stream_text("hello");
        reply(id, {{"stopReason", "end_turn"}});
        continue;
      }
      if (text.find("tool") != std::string::npos) {
        send({{"jsonrpc", "2.0"},
              {"method", "session/update"},
              {"params",
               {{"sessionId", "s-1"},
                {"update",
                 {{"sessionUpdate", "tool_call"},
                  {"toolCallId", "t-1"},
                  {"title", "Printing two lines with printf"},
                  {"name", "bash"},
                  {"status", "pending"},
                  {"rawInput", {{"command", "printf '%s\\n' 'line one' 'line two' --with-a-long-tail that-wraps-across-the-tool-frame-and-keeps-going"}}}}}}}});
        send({{"jsonrpc", "2.0"},
              {"method", "session/update"},
              {"params",
               {{"sessionId", "s-1"},
                {"update",
                 {{"sessionUpdate", "tool_call_update"},
                  {"toolCallId", "t-1"},
                  {"status", "completed"},
                  {"content",
                   json::array({json{{"type", "content"},
                                     {"content", {{"type", "text"}, {"text", "line one\nline two"}}}}})}}}}}});
        stream_text("Done.");
        reply(id, {{"stopReason", "end_turn"}});
        continue;
      }
      if (text.find("markdown") != std::string::npos) {
        stream_text("Here is the result:\n- first `printf` item\n- second item\n\nSee [the docs](https://example.com).\n\n| name | value |\n|------|-------|\n| alpha | 1 |\n| beta | 2 |\n\n| 名称 | value |\n|------|-------|\n| 中文 | 1 |\n\n### Details\n1. first ordered\n2. second ordered\n> quoted line\n  - nested bullet\n\n```cpp\n// sum two numbers\nint add(int a, int b) { return a + b; }\n```\n");
        reply(id, {{"stopReason", "end_turn"}});
        continue;
      }
      if (text.find("perm") != std::string::npos) {
        waiting_for_permission = id;
        send({{"jsonrpc", "2.0"},
              {"id", "perm-1"},
              {"method", "session/request_permission"},
              {"params",
               {{"sessionId", "s-1"},
                {"toolCall", {{"toolCallId", "t-1"}, {"title", "bash"},
                              {"rawInput", {{"command", "ls"}}}}},
                {"options", json::array({json{{"optionId", "allow"}, {"name", "Allow once"},
                                              {"kind", "allow_once"}},
                                         json{{"optionId", "allow-always"}, {"name", "Allow always"},
                                              {"kind", "allow_always"}},
                                         json{{"optionId", "deny"}, {"name", "Deny"},
                                              {"kind", "reject_once"}},
                                         json{{"optionId", "deny-always"}, {"name", "Deny always"},
                                              {"kind", "reject_always"}}})}}}});
        continue;
      }
      stream_text("hello");
      reply(id, {{"stopReason", "end_turn"}});
    } else {
      reply(id, json::object());
    }
  }
  return 0;
}
