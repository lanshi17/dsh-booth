#include "subagent/acp/messages.hpp"

#include <algorithm>
#include <chrono>
#include <cstddef>
#include <string>

namespace booth::acp {
namespace {

std::string clip(std::string text, std::size_t limit) {
  if (text.size() > limit) {
    text.resize(limit);
    text += "…";
  }
  return text;
}

std::string text_of(const Json& content) {
  if (!content.is_object()) return {};
  if (content.value("type", std::string{}) != "text") return {};
  return content.value("text", std::string{});
}

std::string tool_id_of(const Json& update) {
  return update.value("toolCallId", std::string{});
}

std::string tool_input_of(const Json& update) {
  if (!update.contains("rawInput")) return {};
  const auto& raw = update["rawInput"];
  if (raw.is_string()) return raw.get<std::string>();
  if (raw.is_object()) {
    for (const char* key : {"command", "cmd", "description", "path", "file_path", "pattern", "query", "url"}) {
      if (raw.contains(key) && raw[key].is_string()) return raw[key].get<std::string>();
    }
  }
  return raw.dump();
}

std::size_t base64_bytes(const std::string& data) {
  if (data.empty()) return 0;
  std::size_t bytes = data.size() / 4 * 3;
  if (data.size() >= 2 && data.compare(data.size() - 2, 2, "==") == 0) bytes -= 2;
  else if (data.back() == '=') bytes -= 1;
  return bytes;
}

std::string block_text(const Json& block) {
  if (!block.is_object()) return {};
  const std::string type = block.value("type", std::string{});
  if (type == "text") return block.value("text", std::string{});
  if (type == "image") {
    std::string out = "[image: " + block.value("mimeType", std::string{"image"});
    if (block.contains("data") && block["data"].is_string()) {
      out += " · " + std::to_string(base64_bytes(block["data"].get<std::string>())) + "B";
    }
    return out + "]";
  }
  if (type == "resource_link") {
    const std::string uri = block.value("uri", std::string{});
    return uri.empty() ? "[link]" : "[link: " + uri + "]";
  }
  return {};
}

std::string tool_output_of(const Json& update) {
  if (!update.contains("content")) return {};
  std::string out;
  for (const auto& block : update["content"]) {
    std::string text;
    if (block.is_object()) {
      text = block.contains("content") ? block_text(block["content"]) : block_text(block);
    }
    if (text.empty()) continue;
    if (!out.empty() && out.back() != '\n') out += '\n';
    out += text;
  }
  return out;
}

std::string status_of(const Json& update) {
  return update.value("status", std::string{"pending"});
}

}  // namespace

Json initialize_params(std::string_view client_version) {
  return Json{{"protocolVersion", kProtocolVersion},
              {"clientInfo", Json{{"name", "booth"}, {"version", std::string(client_version)}}}};
}

Json new_session_params(std::string_view cwd, const Json& mcp_servers) {
  return Json{{"cwd", std::string(cwd)}, {"mcpServers", mcp_servers}};
}

Json prompt_params(std::string_view session, std::string_view text,
                   const std::vector<std::pair<std::string, std::string>>& resources,
                   const std::vector<ImageAttachment>& images) {
  Json prompt = Json::array({Json{{"type", "text"}, {"text", std::string(text)}}});
  for (const auto& [name, uri] : resources) {
    prompt.push_back(Json{{"type", "resource_link"}, {"name", name}, {"uri", uri}});
  }
  for (const auto& image : images) {
    Json block{{"type", "image"}, {"mimeType", image.mime_type}, {"data", image.data}};
    if (!image.name.empty()) block["name"] = image.name;
    prompt.push_back(std::move(block));
  }
  return Json{{"sessionId", std::string(session)}, {"prompt", std::move(prompt)}};
}

Json set_config_params(std::string_view session, std::string_view config_id,
                       std::string_view value) {
  return Json{{"sessionId", std::string(session)},
              {"configId", std::string(config_id)},
              {"value", std::string(value)}};
}

Json cancel_params(std::string_view session) {
  return Json{{"sessionId", std::string(session)}};
}

Json permission_result(const std::optional<std::string>& option_id) {
  if (!option_id) {
    return Json{{"outcome", Json{{"outcome", "cancelled"}}}};
  }
  return Json{{"outcome", Json{{"outcome", "selected"}, {"optionId", *option_id}}}};
}

std::string id_key(const Json& id) {
  if (id.is_string()) return id.get<std::string>();
  return id.dump();
}

std::optional<PermissionRequest> permission_request(const Json& params,
                                                    std::string_view request_id) {
  if (!params.is_object()) return std::nullopt;
  PermissionRequest request;
  request.id = RequestId{std::string(request_id)};
  request.session = SessionId{params.value("sessionId", std::string{})};
  const auto tool_call = params.value("toolCall", Json::object());
  request.tool_call_id = tool_call.value("toolCallId", std::string{});
  request.tool = tool_call.value("name", std::string{});
  request.title = tool_call.value("title", std::string{});
  if (request.tool.empty()) request.tool = request.title;
  if (request.tool.empty()) request.tool = "tool";
  if (tool_call.contains("rawInput")) {
    request.raw_input = tool_call["rawInput"].dump();
    request.detail = clip(request.raw_input, 512);
  } else {
    request.detail = request.title;
  }
  const auto options = params.value("options", Json::array());
  for (const auto& option : options) {
    if (!option.is_object()) continue;
    request.options.push_back(PermissionOption{option.value("optionId", std::string{}),
                                               option.value("name", std::string{}),
                                               option.value("kind", std::string{})});
  }
  if (request.session.value.empty()) return std::nullopt;
  return request;
}

std::vector<Event> session_updates(const Json& params) {
  std::vector<Event> events;
  if (!params.is_object()) return events;
  const SessionId session{params.value("sessionId", std::string{})};
  const auto update = params.value("update", Json::object());
  if (!update.is_object()) return events;
  const std::string kind = update.value("sessionUpdate", std::string{});

  if (kind == "agent_message_chunk" || kind == "agent_thought_chunk") {
    const auto content = update.value("content", Json::object());
    if (content.value("type", std::string{}) == "image") {
      TranscriptEntry entry;
      entry.kind = EntryKind::Image;
      entry.text = content.value("mimeType", std::string{"image"});
      if (content.contains("data") && content["data"].is_string()) {
        const auto bytes = base64_bytes(content["data"].get<std::string>());
        entry.tool = std::to_string(bytes) + "B";
      } else if (content.contains("name") && content["name"].is_string()) {
        entry.tool = content["name"].get<std::string>();
      }
      events.push_back(Event{std::move(entry)});
      return events;
    }
    const auto text = text_of(content);
    if (text.empty()) return events;
    if (kind == "agent_message_chunk") {
      events.push_back(Event{StreamDelta{text}});
    } else {
      TranscriptEntry entry;
      entry.kind = EntryKind::Thought;
      entry.text = text;
      events.push_back(Event{std::move(entry)});
    }
    return events;
  }

  if (kind == "tool_call") {
    TranscriptEntry entry;
    entry.kind = EntryKind::Tool;
    entry.id = tool_id_of(update);
    entry.tool = update.value("name", std::string{});
    if (entry.tool.empty()) entry.tool = update.value("title", std::string{"tool"});
    entry.text = update.value("title", std::string{});
    entry.status = status_of(update);
    entry.input = tool_input_of(update);
    if (update.contains("rawInput")) entry.raw_input = update["rawInput"].dump();
    entry.started = std::chrono::system_clock::now();
    events.push_back(Event{entry});

    return events;
  }

  if (kind == "tool_call_update") {
    TranscriptEntry entry;
    entry.kind = EntryKind::Tool;
    entry.id = tool_id_of(update);
    entry.tool = update.value("name", std::string{});
    entry.status = status_of(update);
    entry.output = tool_output_of(update);
    entry.finished = std::chrono::system_clock::now();
    events.push_back(Event{entry});

    return events;
  }

  if (kind == "config_option_update") {
    events.push_back(Event{ConfigOptionsChanged{update.value("configOptions", Json::array()).dump()}});
    return events;
  }

  if (kind == "usage_update") {
    UsageChanged usage;
    usage.session = session;
    usage.used = update.value("used", 0);
    usage.size = update.value("size", 0);
    events.push_back(Event{usage});
    return events;
  }

  return events;
}

}  // namespace booth::acp
