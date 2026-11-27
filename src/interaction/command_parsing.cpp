#include "interaction/command_parsing.hpp"

#include <cctype>
#include <string>

namespace booth {
namespace {

std::string trim(std::string_view text) {
  const auto first = text.find_first_not_of(" \t\r\n");
  if (first == std::string_view::npos) return {};
  const auto last = text.find_last_not_of(" \t\r\n");
  return std::string(text.substr(first, last - first + 1));
}

std::string lower(std::string_view text) {
  std::string out(text);
  for (auto& ch : out) ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
  return out;
}

}  // namespace

CompactRequest parse_compact_request(std::string_view args) {
  const std::string input = trim(args);
  if (input.empty()) return {"/compact", "", ""};
  const auto space = input.find_first_of(" \t");
  const std::string mode = lower(input.substr(0, space));
  const std::string focus =
      space == std::string::npos ? std::string{} : trim(std::string_view(input).substr(space + 1));
  if (mode == "snapcompact") {
    return {"", "",
            "/compact snapcompact is unavailable: dsh compaction cannot archive history onto images"};
  }
  const std::string ignored =
      focus.empty() ? std::string{} : "focus instructions ignored; ";
  if (mode == "soft") {
    return {"/compact", "compaction (soft): " + (focus.empty() ? "" : ignored), ""};
  }
  if (mode == "remote") {
    return {"/compact", "remote compaction unavailable; local compaction: " +
                             (focus.empty() ? "" : ignored),
            ""};
  }
  if (mode == "handoff") {
    return {"/compact", "handoff checkpoint: " + (focus.empty() ? "" : ignored), ""};
  }
  return {"/compact", focus.empty() ? "" : "compaction (focus instructions ignored): ", ""};
}

GoalRequest parse_goal_request(std::string_view args) {
  const std::string input = trim(args);
  if (input.empty()) return {"/goal", ""};
  const std::string control = lower(input);
  if (control == "show") return {"/goal", ""};
  if (control == "drop" || control == "clear") return {"/goal clear", ""};
  if (control == "set") return {"", "usage: /goal set <objective>"};
  if (control.rfind("set ", 0) == 0) {
    const std::string objective = trim(std::string_view(input).substr(4));
    if (objective.empty()) return {"", "usage: /goal set <objective>"};
    return {"/goal " + objective, ""};
  }
  if (control == "budget" || control.rfind("budget ", 0) == 0) {
    return {"", "goal budget is unavailable: dsh goals carry no token budget"};
  }
  return {"/goal " + input, ""};
}

}  // namespace booth
