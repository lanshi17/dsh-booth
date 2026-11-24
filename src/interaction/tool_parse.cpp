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

std::optional<std::vector<TodoItem>> parse_todos(const TranscriptEntry& entry) {
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
  std::vector<TodoItem> rows;
  rows.reserve(list->size());
  for (const auto& item : *list) {
    if (!item.is_object() || !item.contains("content") || !item["content"].is_string()) continue;
    TodoItem row;
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

std::optional<WebResult> parse_web_search(const std::string& output) {
  std::istringstream input(output);
  WebResult result;
  std::vector<std::string> answer;
  bool in_sources = false;
  bool recognized = false;
  std::string line;
  while (std::getline(input, line)) {
    if (!line.empty() && line.back() == '\r') line.pop_back();
    if (line == "Sources:") {
      in_sources = true;
      recognized = true;
      continue;
    }
    if (!in_sources) {
      if (line.empty() || line.rfind("External web content follows.", 0) == 0) continue;
      if (line == "No results found.") {
        recognized = true;
        continue;
      }
      answer.push_back(line);
      recognized = true;
      continue;
    }
    if (line.empty()) continue;
    if (line.rfind("Cite the relevant URLs", 0) == 0) continue;
    if (line.front() == '(' && line.back() == ')') {
      result.footer = line;
      recognized = true;
      continue;
    }
    if (line.rfind("- [", 0) != 0) continue;
    const auto open = line.find("](", 3);
    if (open == std::string::npos) continue;
    std::size_t close = open + 2;
    int depth = 0;
    for (; close < line.size(); ++close) {
      if (line[close] == '(') ++depth;
      else if (line[close] == ')') {
        if (depth == 0) break;
        --depth;
      }
    }
    if (close >= line.size()) continue;
    WebSource source;
    source.title = line.substr(3, open - 3);
    source.url = line.substr(open + 2, close - open - 2);
    std::string suffix = line.substr(close + 1);
    const std::string separator = " — ";
    if (suffix.rfind(separator, 0) == 0) suffix.erase(0, separator.size());
    source.excerpt = suffix;
    result.sources.push_back(std::move(source));
    recognized = true;
  }
  if (!recognized) return std::nullopt;
  while (!answer.empty() && answer.back().empty()) answer.pop_back();
  for (const auto& row : answer) {
    if (!result.answer.empty() || !row.empty()) {
      if (!result.answer.empty()) result.answer += '\n';
      result.answer += row;
    }
  }
  return result;
}

std::string web_queries(const TranscriptEntry& entry) {
  const Json raw = Json::parse(entry.raw_input, nullptr, false);
  if (raw.is_discarded() || !raw.is_object()) return {};
  std::string out;
  for (const auto& query : raw.value("queries", Json::array())) {
    if (!query.is_string()) continue;
    if (!out.empty()) out += ", ";
    out += query.get<std::string>();
  }
  return out;
}

std::string tool_activity(const TranscriptEntry& entry) {
  const Json raw = Json::parse(entry.raw_input, nullptr, false);
  const auto field = [&](std::initializer_list<const char*> keys) -> std::string {
    if (raw.is_discarded() || !raw.is_object()) return {};
    for (const char* key : keys) {
      if (raw.contains(key) && raw[key].is_string()) return raw[key].get<std::string>();
    }
    return {};
  };
  const auto first_line = [](const std::string& text) {
    const auto end = text.find('\n');
    return end == std::string::npos ? text : text.substr(0, end);
  };
  if (!entry.text.empty() && entry.text != entry.tool) return entry.text;
  std::string label;
  if (is_shell_tool(entry.tool)) {
    const std::string command = first_line(field({"command", "cmd"}));
    if (!command.empty()) label = "$ " + command;
  } else if (entry.tool == "read" || entry.tool == "read_file") {
    label = field({"path", "file_path"});
  } else if (entry.tool == "grep") {
    const std::string pattern = field({"pattern"});
    if (!pattern.empty()) label = "grep \"" + pattern + "\"";
  } else if (entry.tool == "glob") {
    const std::string pattern = field({"pattern"});
    if (!pattern.empty()) label = "glob \"" + pattern + "\"";
  } else if (entry.tool == "web_search") {
    const std::string query = web_queries(entry);
    if (!query.empty()) label = "web \"" + query + "\"";
  } else if (entry.tool == "web_fetch") {
    const std::string url = field({"url"});
    if (!url.empty()) label = "fetch " + url;
  } else if (entry.tool == "edit" || entry.tool == "write") {
    label = field({"file_path", "path"});
  } else if (entry.tool == "todo_write") {
    label = "Updating todos";
  } else if (entry.tool == "skill") {
    label = field({"name"});
    if (!label.empty()) label = "skill " + label;
  } else if (entry.tool == "subagent" || entry.tool == "task") {
    label = field({"description", "agent"});
  }
  if (!label.empty()) return label;
  return entry.text.empty() ? entry.tool : entry.text;
}

std::optional<WebFetchResult> parse_web_fetch(const std::string& output) {
  std::istringstream input(output);
  std::string line;
  if (!std::getline(input, line)) return std::nullopt;
  if (!line.empty() && line.back() == '\r') line.pop_back();
  const std::string prefix = "Fetched ";
  if (line.rfind(prefix, 0) != 0) return std::nullopt;
  const auto marker = line.rfind(" (HTTP ");
  if (marker == std::string::npos || line.back() != ')') return std::nullopt;
  WebFetchResult result;
  result.url = line.substr(prefix.size(), marker - prefix.size());
  result.status = line.substr(marker + 7, line.size() - marker - 8);
  bool notice = false;
  while (std::getline(input, line)) {
    if (!line.empty() && line.back() == '\r') line.pop_back();
    if (!notice && line.rfind("External web content follows.", 0) == 0) {
      notice = true;
      continue;
    }
    if (line.rfind("(Content truncated.", 0) == 0) {
      result.footer = line;
      break;
    }
    result.body.push_back(line);
  }
  while (!result.body.empty() && result.body.front().empty()) result.body.erase(result.body.begin());
  while (!result.body.empty() && result.body.back().empty()) result.body.pop_back();
  return result;
}

std::optional<SkillResult> parse_skill(const TranscriptEntry& entry) {
  if (entry.tool != "skill") return std::nullopt;
  SkillResult result;
  bool in_instructions = false;
  std::istringstream input(entry.output);
  std::string line;
  while (std::getline(input, line)) {
    if (!line.empty() && line.back() == '\r') line.pop_back();
    const std::string prefix = "<skill_content name=\"";
    if (line.rfind(prefix, 0) == 0) {
      const auto end = line.find('"', prefix.size());
      if (end != std::string::npos) {
        result.name = line.substr(prefix.size(), end - prefix.size());
      }
      continue;
    }
    if (line == "<skill_instructions>") {
      in_instructions = true;
      continue;
    }
    if (line == "</skill_instructions>" || line == "</skill_content>") {
      in_instructions = false;
      continue;
    }
    if (line.rfind("Base directory for this skill:", 0) == 0) {
      result.base = line;
      continue;
    }
    if (in_instructions) result.instructions.push_back(line);
  }
  if (result.name.empty() && result.instructions.empty()) return std::nullopt;
  return result;
}

std::optional<std::string> pretty_json(const std::string& output) {
  const Json parsed = Json::parse(output, nullptr, false);
  if (parsed.is_discarded() || (!parsed.is_object() && !parsed.is_array())) return std::nullopt;
  return parsed.dump(2);
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
