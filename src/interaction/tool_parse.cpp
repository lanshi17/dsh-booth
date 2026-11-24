#include "interaction/tool_parse.hpp"

#include <nlohmann/json.hpp>

#include <cstddef>
#include <optional>
#include <sstream>
#include <string>
#include <string_view>
#include <vector>

#include "interaction/view.hpp"

namespace booth {

using Json = nlohmann::json;

ReadEnvelope parse_read_envelope(const std::string& output) {
  ReadEnvelope envelope;
  const auto path_start = output.find("<path>");
  if (path_start != std::string::npos) {
    const auto path_end = output.find("</path>", path_start + 6);
    if (path_end != std::string::npos) {
      envelope.path = output.substr(path_start + 6, path_end - path_start - 6);
    }
  }
  const auto content_start = output.find("<content>");
  const auto content_end = content_start == std::string::npos
                               ? std::string::npos
                               : output.find("</content>", content_start + 9);
  if (content_start == std::string::npos) return envelope;
  const std::string body =
      output.substr(content_start + 9, content_end == std::string::npos
                                             ? std::string::npos
                                             : content_end - content_start - 9);
  std::istringstream input(body);
  std::string line;
  while (std::getline(input, line)) {
    if (!line.empty() && line.back() == '\r') line.pop_back();
    envelope.rows.push_back(line);
  }
  while (!envelope.rows.empty() && envelope.rows.front().empty()) envelope.rows.erase(envelope.rows.begin());
  while (!envelope.rows.empty() && envelope.rows.back().empty()) envelope.rows.pop_back();
  if (!envelope.rows.empty() && envelope.rows.back().size() >= 2 &&
      envelope.rows.back().front() == '(' && envelope.rows.back().back() == ')') {
    envelope.footer = envelope.rows.back();
    envelope.rows.pop_back();
    while (!envelope.rows.empty() && envelope.rows.back().empty()) envelope.rows.pop_back();
  }
  return envelope;
}

std::optional<std::vector<TodoRow>> parse_todos(const TranscriptEntry& entry) {
  if (entry.tool != "todo_write") return std::nullopt;
  const std::string& source = entry.raw_input.empty() ? entry.input : entry.raw_input;
  if (source.empty()) return std::nullopt;
  const Json parsed = Json::parse(source, nullptr, false);
  if (parsed.is_discarded()) return std::nullopt;
  const Json* list = nullptr;
  if (parsed.is_object() && parsed.contains("todos") && parsed["todos"].is_array()) {
    list = &parsed["todos"];
  } else if (parsed.is_array()) {
    list = &parsed;
  } else {
    return std::nullopt;
  }
  std::vector<TodoRow> rows;
  rows.reserve(list->size());
  for (const auto& item : *list) {
    if (!item.is_object() || !item.contains("content") || !item["content"].is_string()) continue;
    TodoRow row;
    row.content = item["content"].get<std::string>();
    row.status = item.value("status", std::string{"pending"});
    rows.push_back(std::move(row));
  }
  return rows;
}

bool is_shell_tool(const std::string& tool) {
  return tool == "bash" || tool == "sh" || tool == "shell" || tool == "pwsh" ||
         tool == "powershell";
}

ShellStatus take_shell_status(std::string& text) {
  const auto end = text.find_last_not_of("\r\n");
  if (end == std::string::npos) return {};
  const auto newline = text.find_last_of('\n', end);
  const std::size_t start = newline == std::string::npos ? 0 : newline + 1;
  const std::string line = text.substr(start, end - start + 1);
  const auto between = [&](std::string_view prefix) -> std::string {
    if (line.rfind(prefix, 0) != 0 || line.back() != ']') return {};
    return line.substr(prefix.size(), line.size() - prefix.size() - 1);
  };
  ShellStatus status;
  if (const std::string exit_value = between("[exit code: "); !exit_value.empty()) {
    if (exit_value == "null") {
      status.note = "exit ?";
      status.failed = true;
    } else {
      status.note = "exit " + exit_value;
      status.failed = exit_value != "0";
    }
  } else if (const std::string signal_value = between("[killed by signal: ");
             !signal_value.empty()) {
    status.note = "signal " + signal_value;
    status.failed = true;
  } else if (const std::string command_value = between("[Command finished with exit code ");
             !command_value.empty()) {
    status.note = "exit " + command_value;
    status.failed = command_value != "0";
  } else if (const std::string shell_signal = between("[shell killed by signal: ");
             !shell_signal.empty()) {
    status.note = "signal " + shell_signal;
    status.failed = true;
  } else {
    return {};
  }
  text.erase(start);
  while (!text.empty() && (text.back() == '\n' || text.back() == '\r')) text.pop_back();
  return status;
}

std::optional<GrepResult> parse_grep(const std::string& output) {
  std::istringstream input(output);
  std::vector<std::string> lines;
  std::string line;
  while (std::getline(input, line)) {
    if (!line.empty() && line.back() == '\r') line.pop_back();
    lines.push_back(line);
  }
  std::size_t index = 0;
  while (index < lines.size() && lines[index].empty()) ++index;
  if (index >= lines.size()) return std::nullopt;
  bool recognized = false;
  if (lines[index].rfind("Found ", 0) == 0 || lines[index] == "No matches found") {
    recognized = true;
    ++index;
  }
  GrepResult result;
  while (index < lines.size()) {
    const std::string& row = lines[index++];
    if (row.empty()) continue;
    if (row.rfind("Line ", 0) == 0) {
      const auto colon = row.find(": ", 5);
      if (colon == std::string::npos || result.groups.empty()) continue;
      std::size_t number = 0;
      try {
        number = static_cast<std::size_t>(std::stoul(row.substr(5, colon - 5)));
      } catch (...) {
        continue;
      }
      result.groups.back().lines.push_back({number, row.substr(colon + 2)});
      ++result.total;
      recognized = true;
    } else if (row.front() == '(' && row.back() == ')') {
      result.footer = row;
      recognized = true;
    } else {
      result.groups.push_back({row, {}});
    }
  }
  if (!recognized) return std::nullopt;
  return result;
}

std::string grep_pattern(const TranscriptEntry& entry) {
  const Json raw = Json::parse(entry.raw_input, nullptr, false);
  if (!raw.is_discarded() && raw.is_object()) return raw.value("pattern", std::string{});
  return entry.input;
}

std::optional<GlobResult> parse_glob(const std::string& output) {
  std::istringstream input(output);
  GlobResult result;
  bool recognized = false;
  std::string line;
  while (std::getline(input, line)) {
    if (!line.empty() && line.back() == '\r') line.pop_back();
    if (line.empty()) continue;
    if (line == "No files found") {
      recognized = true;
    } else if (line.front() == '(' && line.back() == ')') {
      result.footer = line;
      recognized = true;
    } else {
      result.paths.push_back(line);
      recognized = true;
    }
  }
  if (!recognized) return std::nullopt;
  return result;
}

std::string glob_pattern(const TranscriptEntry& entry) {
  const Json raw = Json::parse(entry.raw_input, nullptr, false);
  if (!raw.is_discarded() && raw.is_object()) return raw.value("pattern", std::string{});
  return {};
}

std::vector<ReadEnvelope> parse_read_envelopes(const std::string& output) {
  std::vector<ReadEnvelope> envelopes;
  std::size_t start = output.find("<path>");
  while (start != std::string::npos) {
    const std::size_t next = output.find("<path>", start + 6);
    ReadEnvelope envelope =
        parse_read_envelope(output.substr(start, next == std::string::npos ? std::string::npos
                                                                          : next - start));
    if (!envelope.path.empty() || !envelope.rows.empty() || !envelope.footer.empty()) {
      envelopes.push_back(std::move(envelope));
    }
    start = next;
  }
  return envelopes;
}

std::string read_output_text(const std::string& output) {
  std::string out;
  for (const auto& read : parse_read_envelopes(output)) {
    if (!out.empty()) out += '\n';
    if (!read.path.empty()) out += read.path + '\n';
    for (const auto& row : read.rows) out += row + '\n';
    if (!read.footer.empty()) out += read.footer + '\n';
  }
  return out;
}

}  // namespace booth
