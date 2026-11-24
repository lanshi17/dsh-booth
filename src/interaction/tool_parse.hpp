#pragma once

#include "contract/event.hpp"

#include <optional>
#include <string>
#include <vector>

namespace booth {

struct ReadEnvelope {
  std::string path;
  std::vector<std::string> rows;
  std::string footer;
};

std::vector<ReadEnvelope> parse_read_envelopes(const std::string& output);

std::optional<std::vector<TodoItem>> parse_todos(const TranscriptEntry& entry);

bool is_shell_tool(const std::string& tool);

struct ShellStatus {
  std::string note;
  bool failed{false};
};

/// Consume DSH's trailing exit/signal marker and return the footer note.
ShellStatus take_shell_status(std::string& text);

struct GrepLine {
  std::size_t number{0};
  std::string text;
};

struct GrepGroup {
  std::string path;
  std::vector<GrepLine> lines;
};

struct GrepResult {
  std::vector<GrepGroup> groups;
  std::string footer;
  std::size_t total{0};
};

std::optional<GrepResult> parse_grep(const std::string& output);
std::string grep_pattern(const TranscriptEntry& entry);

struct GlobResult {
  std::vector<std::string> paths;
  std::string footer;
};

std::optional<GlobResult> parse_glob(const std::string& output);
std::string glob_pattern(const TranscriptEntry& entry);

struct WebSource {
  std::string title;
  std::string url;
  std::string excerpt;
};

struct WebResult {
  std::string answer;
  std::vector<WebSource> sources;
  std::string footer;
};

std::optional<WebResult> parse_web_search(const std::string& output);
std::string web_queries(const TranscriptEntry& entry);

struct WebFetchResult {
  std::string url;
  std::string status;
  std::vector<std::string> body;
  std::string footer;
};

std::optional<WebFetchResult> parse_web_fetch(const std::string& output);

struct SkillResult {
  std::string name;
  std::string base;
  std::vector<std::string> instructions;
};

std::optional<SkillResult> parse_skill(const TranscriptEntry& entry);

/// Pretty-print a complete JSON object or array; empty for primitives or invalid JSON.
std::optional<std::string> pretty_json(const std::string& output);

/// A short activity label derived from the tool arguments.
std::string tool_activity(const TranscriptEntry& entry);

}  // namespace booth
