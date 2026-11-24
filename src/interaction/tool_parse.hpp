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

struct TodoRow {
  std::string content;
  std::string status;
};

std::optional<std::vector<TodoRow>> parse_todos(const TranscriptEntry& entry);

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

}  // namespace booth
