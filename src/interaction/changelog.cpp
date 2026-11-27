#include "interaction/changelog.hpp"

#include <algorithm>
#include <cctype>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>

namespace booth {
namespace {

std::string_view trim(std::string_view text) {
  while (!text.empty() && std::isspace(static_cast<unsigned char>(text.front())) != 0) {
    text.remove_prefix(1);
  }
  while (!text.empty() && std::isspace(static_cast<unsigned char>(text.back())) != 0) {
    text.remove_suffix(1);
  }
  return text;
}

bool parse_version(std::string_view heading, ChangelogEntry& entry) {
  if (heading.rfind("## ", 0) != 0) return false;
  std::string rest(heading.substr(3));
  const auto first = rest.find_first_not_of(" \t");
  if (first == std::string::npos) return false;
  rest.erase(0, first);
  if (!rest.empty() && rest.front() == '[') rest.erase(0, 1);
  int major = 0;
  int minor = 0;
  int patch = 0;
  char tail = 0;
  if (std::sscanf(rest.c_str(), "%d.%d.%d%c", &major, &minor, &patch, &tail) != 4) return false;
  if (tail != ']' && !std::isspace(static_cast<unsigned char>(tail)) && tail != '-' && tail != '\xE2') {
    return false;
  }
  entry.major = major;
  entry.minor = minor;
  entry.patch = patch;
  return true;
}

std::string usage() {
  return "Usage: /changelog [full|last [N]]";
}

}  // namespace

std::vector<ChangelogEntry> parse_changelog(std::string_view markdown) {
  std::vector<ChangelogEntry> entries;
  ChangelogEntry current;
  bool open = false;
  std::string body;
  std::size_t start = 0;
  while (start <= markdown.size()) {
    const std::size_t end = markdown.find('\n', start);
    const std::string_view line =
        markdown.substr(start, end == std::string_view::npos ? std::string_view::npos : end - start);
    ChangelogEntry next;
    if (parse_version(line, next)) {
      if (open) {
        current.content = std::string(trim(body));
        if (!current.content.empty()) entries.push_back(current);
      }
      current = next;
      open = true;
      body = std::string(line);
    } else if (line.rfind("## ", 0) == 0) {
      if (open) {
        current.content = std::string(trim(body));
        if (!current.content.empty()) entries.push_back(current);
      }
      open = false;
      body.clear();
    } else if (open) {
      body += '\n';
      body += std::string(line);
    }
    if (end == std::string_view::npos) break;
    start = end + 1;
  }
  if (open) {
    current.content = std::string(trim(body));
    if (!current.content.empty()) entries.push_back(current);
  }
  return entries;
}

std::string format_changelog(std::string_view args, const std::vector<ChangelogEntry>& entries) {
  const std::string_view text = trim(args);
  std::size_t count = 3;
  if (!text.empty()) {
    std::string lowered(text);
    std::transform(lowered.begin(), lowered.end(), lowered.begin(),
                   [](unsigned char ch) { return static_cast<char>(std::tolower(ch)); });
    if (lowered == "full") {
      count = entries.size();
    } else if (lowered == "last") {
      count = 1;
    } else if (lowered.rfind("last ", 0) == 0) {
      const std::string_view digits = trim(std::string_view(lowered).substr(5));
      if (digits.empty() || !std::all_of(digits.begin(), digits.end(),
                                         [](unsigned char ch) { return std::isdigit(ch) != 0; })) {
        return usage();
      }
      count = static_cast<std::size_t>(std::strtoull(std::string(digits).c_str(), nullptr, 10));
      if (count < 1) return usage() + " (N must be a positive integer)";
    } else {
      return usage();
    }
  }
  if (entries.empty()) return "No changelog entries found.";
  count = std::min(count, entries.size());
  std::string out;
  for (std::size_t i = 0; i < count; ++i) {
    if (!out.empty()) out += "\n\n";
    out += entries[i].content;
  }
  return out;
}

}  // namespace booth
