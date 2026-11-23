#include "interaction/tool_diff.hpp"

#include <nlohmann/json.hpp>

namespace booth {
namespace {

using Json = nlohmann::json;

void append_lines(std::vector<DiffLine>& out, std::string_view text, int change,
                  std::string_view prefix) {
  std::size_t start = 0;
  while (start <= text.size()) {
    const auto end = text.find('\n', start);
    const std::string line(text.substr(start, end == std::string_view::npos
                                                  ? std::string_view::npos
                                                  : end - start));
    out.push_back({std::string(prefix) + line, change});
    if (end == std::string_view::npos) break;
    start = end + 1;
  }
}

std::vector<DiffLine> from_edit(const Json& raw) {
  const std::string path = raw.value("file_path", raw.value("path", std::string{}));
  const std::string before = raw.value("old_string", raw.value("old_str", std::string{}));
  const std::string after = raw.value("new_string", raw.value("new_str", std::string{}));
  if (before.empty() && after.empty()) return {};
  std::vector<DiffLine> lines;
  if (!path.empty()) lines.push_back({path, 0});
  append_lines(lines, before, -1, "- ");
  append_lines(lines, after, 1, "+ ");
  return lines;
}

std::vector<DiffLine> from_patch(const std::string& patch) {
  std::vector<DiffLine> lines;
  std::size_t start = 0;
  while (start <= patch.size()) {
    const auto end = patch.find('\n', start);
    const std::string line = patch.substr(start, end == std::string::npos ? std::string::npos
                                                                            : end - start);
    if (!line.empty()) {
      const int change = line[0] == '+' ? 1 : line[0] == '-' ? -1 : 0;
      lines.push_back({line, change});
    }
    if (end == std::string::npos) break;
    start = end + 1;
  }
  return lines;
}

}  // namespace

std::vector<DiffLine> tool_diff(const TranscriptEntry& entry) {
  Json raw = Json::parse(entry.raw_input, nullptr, false);
  if (raw.is_discarded() || !raw.is_object()) return {};
  if (raw.contains("content") && raw.contains("file_path")) {
    std::vector<DiffLine> lines;
    lines.push_back({raw.value("file_path", std::string{}), 0});
    append_lines(lines, raw.value("content", std::string{}), 1, "+ ");
    return lines;
  }
  if (raw.contains("input") && raw["input"].is_string()) {
    return from_patch(raw["input"].get<std::string>());
  }
  if (raw.contains("patch") && raw["patch"].is_string()) {
    return from_patch(raw["patch"].get<std::string>());
  }
  return from_edit(raw);
}

}  // namespace booth
