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
    for (const char* key : {"command", "cmd", "path", "file_path", "pattern", "query", "url"}) {
      if (raw.contains(key) && raw[key].is_string()) return raw[key].get<std::string>();
    }
  }
  return raw.dump();
}

std::string tool_output_of(const Json& update) {
  if (!update.contains("content")) return {};
  std::string out;
  for (const auto& block : update["content"]) {
    std::string text;
    if (block.is_object()) {
      if (block.contains("content")) {
        text = text_of(block["content"]);
      } else {
        text = text_of(block);
      }
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

Json new_session_params(std::string_view cwd) {
  return Json{{"cwd", std::string(cwd)}, {"mcpServers", Json::array()}};
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
  request.tool = tool_call.value("name", std::string{});
  if (request.tool.empty()) request.tool = tool_call.value("title", std::string{"tool"});
  if (tool_call.contains("rawInput")) {
    request.detail = clip(tool_call["rawInput"].dump(), 512);
  } else {
    request.detail = tool_call.value("title", std::string{});
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
    const auto text = text_of(update.value("content", Json::object()));
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
    for (const auto& option : update.value("configOptions", Json::array())) {
      if (!option.is_object()) continue;
      const std::string category = option.value("category", std::string{});
      if (category != "model" && category != "thought_level" && category != "model_config") {
        continue;
      }
      const std::string id = option.value("id", std::string{});
      std::string value;
      if (option.contains("currentValue")) {
        value = option["currentValue"].is_string() ? option["currentValue"].get<std::string>()
                                                   : option["currentValue"].dump();
      }
      events.push_back(Event{SessionConfigChanged{session, id, value}});
    }
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
