#include "interaction/tool_diff.hpp"

#include <nlohmann/json.hpp>

#include <algorithm>
#include <cctype>
#include <sstream>
#include <vector>

namespace booth {
namespace {

using Json = nlohmann::json;

void append_lines(std::vector<DiffLine>& out, std::string_view text, int change,
                  std::string_view prefix, int line_number) {
  std::size_t start = 0;
  while (start <= text.size()) {
    const auto end = text.find('\n', start);
    const std::string line(text.substr(start, end == std::string_view::npos
                                                  ? std::string_view::npos
                                                  : end - start));
    out.push_back({std::string(prefix) + line, change, 0, line_number++});
    if (end == std::string_view::npos) break;
    start = end + 1;
  }
}

std::vector<std::string> split_lines(const std::string& text) {
  std::vector<std::string> lines;
  if (text.empty()) return lines;
  std::istringstream input(text);
  std::string line;
  while (std::getline(input, line)) lines.push_back(line);
  return lines;
}

std::vector<DiffLine> compact_context(const std::vector<DiffLine>& raw) {
  constexpr std::size_t kContext = 2;
  std::vector<DiffLine> out;
  bool gap = false;
  for (std::size_t index = 0; index < raw.size(); ++index) {
    bool keep = raw[index].change != 0;
    if (!keep) {
      const std::size_t first = index > kContext ? index - kContext : 0;
      const std::size_t last = std::min(raw.size() - 1, index + kContext);
      for (std::size_t probe = first; probe <= last; ++probe) {
        if (raw[probe].change != 0) {
          keep = true;
          break;
        }
      }
    }
    if (keep) {
      if (gap) {
        out.push_back({"…", 0});
        gap = false;
      }
      out.push_back(raw[index]);
    } else {
      gap = true;
    }
  }
  if (gap && !out.empty()) out.push_back({"…", 0});
  return out;
}

std::vector<DiffLine> line_diff(const std::string& before, const std::string& after) {
  const auto old_lines = split_lines(before);
  const auto new_lines = split_lines(after);
  if (old_lines.size() > 400 || new_lines.size() > 400) {
    std::vector<DiffLine> fallback;
    int old_line = 1;
    int new_line = 1;
    for (const auto& line : old_lines) fallback.push_back({"- " + line, -1, old_line++, 0});
    for (const auto& line : new_lines) fallback.push_back({"+ " + line, 1, 0, new_line++});
    return compact_context(fallback);
  }

  const std::size_t n = old_lines.size();
  const std::size_t m = new_lines.size();
  std::vector<std::vector<int>> lcs(n + 1, std::vector<int>(m + 1, 0));
  for (std::size_t i = n; i-- > 0;) {
    for (std::size_t j = m; j-- > 0;) {
      lcs[i][j] = old_lines[i] == new_lines[j]
                      ? lcs[i + 1][j + 1] + 1
                      : std::max(lcs[i + 1][j], lcs[i][j + 1]);
    }
  }

  std::vector<DiffLine> raw;
  std::size_t i = 0;
  std::size_t j = 0;
  int old_line = 1;
  int new_line = 1;
  while (i < n && j < m) {
    if (old_lines[i] == new_lines[j]) {
      raw.push_back({"  " + old_lines[i], 0, old_line++, new_line++});
      ++i;
      ++j;
    } else if (lcs[i + 1][j] >= lcs[i][j + 1]) {
      raw.push_back({"- " + old_lines[i], -1, old_line++, 0});
      ++i;
    } else {
      raw.push_back({"+ " + new_lines[j], 1, 0, new_line++});
      ++j;
    }
  }
  while (i < n) raw.push_back({"- " + old_lines[i++], -1, old_line++, 0});
  while (j < m) raw.push_back({"+ " + new_lines[j++], 1, 0, new_line++});
  return compact_context(raw);
}

std::vector<DiffLine> from_edit(const Json& raw) {
  const std::string path = raw.value("file_path", raw.value("path", std::string{}));
  const std::string before = raw.value("old_string", raw.value("old_str", std::string{}));
  const std::string after = raw.value("new_string", raw.value("new_str", std::string{}));
  if (before.empty() && after.empty()) return {};
  std::vector<DiffLine> lines;
  if (!path.empty()) lines.push_back({path, 0});
  const auto diff = line_diff(before, after);
  if (diff.empty()) {
    lines.push_back({"  (no changes)", 0});
    return lines;
  }
  lines.insert(lines.end(), diff.begin(), diff.end());
  return lines;
}

bool hunk_header(const std::string& line, int& old_start, int& new_start) {
  if (line.rfind("@@ -", 0) != 0) return false;
  std::size_t pos = 4;
  const auto read_number = [&](int& value) -> bool {
    const std::size_t begin = pos;
    while (pos < line.size() && std::isdigit(static_cast<unsigned char>(line[pos]))) ++pos;
    if (pos == begin) return false;
    value = std::stoi(line.substr(begin, pos - begin));
    return true;
  };
  if (!read_number(old_start)) return false;
  const auto plus = line.find('+', pos);
  if (plus == std::string::npos) return false;
  pos = plus + 1;
  return read_number(new_start);
}

std::vector<DiffLine> from_patch(const std::string& patch) {
  std::vector<DiffLine> lines;
  std::size_t start = 0;
  int old_line = 0;
  int new_line = 0;
  while (start <= patch.size()) {
    const auto end = patch.find('\n', start);
    const std::string line = patch.substr(start, end == std::string::npos ? std::string::npos
                                                                            : end - start);
    if (!line.empty()) {
      int old_start = 0;
      int new_start = 0;
      if (hunk_header(line, old_start, new_start)) {
        old_line = old_start;
        new_line = new_start;
        lines.push_back({line, 0});
      } else {
        const int change = line[0] == '+' ? 1 : line[0] == '-' ? -1 : 0;
        lines.push_back({line, change, change <= 0 ? old_line : 0,
                         change >= 0 ? new_line : 0});
        if (change <= 0) ++old_line;
        if (change >= 0) ++new_line;
      }
    }
    if (end == std::string::npos) break;
    start = end + 1;
  }
  return lines;
}

}  // namespace

std::optional<std::vector<PatchFile>> codex_patch(const TranscriptEntry& entry) {
  if (entry.tool != "apply_patch") return std::nullopt;
  const Json raw = Json::parse(entry.raw_input, nullptr, false);
  if (raw.is_discarded() || !raw.is_object()) return std::nullopt;
  const std::string patch = raw.value("input", std::string{});
  if (patch.rfind("*** Begin Patch", 0) != 0) return std::nullopt;

  std::vector<PatchFile> files;
  PatchFile current;
  const auto flush = [&] {
    if (current.path.empty()) return;
    files.push_back(std::move(current));
    current = {};
  };
  const auto after = [](const std::string& line, std::string_view prefix) {
    return line.substr(prefix.size());
  };

  std::istringstream input(patch);
  std::string line;
  while (std::getline(input, line)) {
    if (!line.empty() && line.back() == '\r') line.pop_back();
    if (line.rfind("*** Add File: ", 0) == 0) {
      flush();
      current.op = PatchOp::Add;
      current.path = after(line, "*** Add File: ");
      continue;
    }
    if (line.rfind("*** Update File: ", 0) == 0) {
      flush();
      current.op = PatchOp::Update;
      current.path = after(line, "*** Update File: ");
      continue;
    }
    if (line.rfind("*** Delete File: ", 0) == 0) {
      flush();
      current.op = PatchOp::Delete;
      current.path = after(line, "*** Delete File: ");
      continue;
    }
    if (line.rfind("*** Move to: ", 0) == 0) {
      if (!current.path.empty()) {
        current.op = PatchOp::Move;
        current.destination = after(line, "*** Move to: ");
      }
      continue;
    }
    if (line == "*** End Patch") {
      flush();
      break;
    }
    if (line == "*** End of File" || line.rfind("*** Begin Patch", 0) == 0) continue;
    if (current.path.empty()) continue;
    if (line.rfind("@@", 0) == 0) {
      current.lines.push_back({line, 0});
      continue;
    }
    if (!line.empty() && line.front() == '+') {
      current.lines.push_back({line, 1});
      ++current.added;
      continue;
    }
    if (!line.empty() && line.front() == '-') {
      current.lines.push_back({line, -1});
      ++current.removed;
      continue;
    }
    current.lines.push_back({line, 0});
  }
  flush();
  if (files.empty()) return std::nullopt;
  return files;
}

std::vector<DiffLine> tool_diff(const TranscriptEntry& entry) {
  Json raw = Json::parse(entry.raw_input, nullptr, false);
  if (raw.is_discarded() || !raw.is_object()) return {};
  if (raw.contains("content") && raw.contains("file_path")) {
    std::vector<DiffLine> lines;
    lines.push_back({raw.value("file_path", std::string{}), 0});
    append_lines(lines, raw.value("content", std::string{}), 1, "+ ", 1);
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
