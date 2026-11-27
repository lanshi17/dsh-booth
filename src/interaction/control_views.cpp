#include "interaction/control_views.hpp"

#include <cstddef>
#include <string>

namespace booth {
namespace {

using json = nlohmann::json;

std::string shorten(std::string text, std::size_t width) {
  if (text.size() <= width) return text;
  if (width <= 1) return "…";
  text.resize(width - 1);
  return text + "…";
}

}  // namespace

std::string tree_text(const json& result) {
  if (!result.is_array() || result.empty()) return "no session events";
  std::string out;
  for (const auto& event : result) {
    if (!event.is_object()) continue;
    const std::string type = event.value("type", std::string{});
    if (type != "user/message" && type != "assistant/message" && type != "turn/end" &&
        type != "turn/start") {
      continue;
    }
    if (!out.empty()) out += '\n';
    out += std::to_string(event.value("seq", 0LL)) + " · " + type;
    const std::string text = event.value("text", std::string{});
    if (!text.empty()) out += " · " + text;
  }
  return out.empty() ? "no conversational events" : out;
}

std::string jobs_text(const json& result, bool full) {
  if (!result.is_array() || result.empty()) return "no background jobs";
  std::string out;
  for (const auto& job : result) {
    if (!job.is_object()) continue;
    if (!out.empty()) out += '\n';
    out += job.value("id", std::string{"job"});
    out += " · " + job.value("kind", std::string{"job"});
    out += " · " + job.value("status", std::string{"unknown"});
    std::string label = job.value("label", std::string{});
    if (!label.empty()) out += " · " + (full ? label : shorten(std::move(label), 80));
    const std::string progress = job.value("progress", std::string{});
    if (!progress.empty()) out += " · " + progress;
    if (full) {
      const std::string detail = job.value("detail", std::string{});
      if (!detail.empty()) out += " · " + detail;
    }
  }
  return out;
}

std::string tools_text(const json& result) {
  if (!result.is_array() || result.empty()) return "no tools";
  std::string out;
  for (const auto& tool : result) {
    if (!tool.is_object()) continue;
    if (!out.empty()) out += '\n';
    out += tool.value("name", std::string{"tool"});
    const std::string description = tool.value("description", std::string{});
    if (!description.empty()) out += " · " + shorten(description, 96);
  }
  return out;
}

std::string dirs_text(const json& result) {
  std::string out = "workspace " + result.value("cwd", std::string{"(none)"});
  const auto roots = result.value("roots", json::array());
  if (!roots.is_array() || roots.empty()) {
    out += " · no added directories";
  } else {
    for (const auto& root : roots) {
      if (root.is_string()) out += " · " + root.get<std::string>();
    }
  }
  return out;
}

std::string agents_text(const json& result) {
  if (!result.is_array() || result.empty()) return "no agents";
  std::string out;
  for (const auto& agent : result) {
    if (!agent.is_object()) continue;
    if (!out.empty()) out += '\n';
    out += agent.value("sessionId", std::string{"session"});
    const std::string title = agent.value("title", std::string{});
    if (!title.empty()) out += " · " + title;
    out += " · " + agent.value("status", std::string{"unknown"});
    const std::string cwd = agent.value("cwd", std::string{});
    if (!cwd.empty()) out += " · " + cwd;
  }
  return out;
}

}  // namespace booth
