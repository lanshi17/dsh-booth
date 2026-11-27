#pragma once

#include <string>
#include <string_view>
#include <vector>

namespace booth {

struct ChangelogEntry {
  int major = 0;
  int minor = 0;
  int patch = 0;
  std::string content;
};

/// Parse `## [major.minor.patch]` sections from a Markdown changelog.
std::vector<ChangelogEntry> parse_changelog(std::string_view markdown);

/// Render `/changelog` arguments against parsed entries. `count == 0` means all.
std::string format_changelog(std::string_view args, const std::vector<ChangelogEntry>& entries);

}  // namespace booth
