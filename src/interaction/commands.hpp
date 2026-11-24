#pragma once

#include <optional>
#include <span>
#include <string>
#include <string_view>
#include <vector>

namespace booth {

struct CommandHint {
  const char* name;
  const char* description;
  const char* keywords;
  const char* aliases;  // space-separated, without the leading slash
};

/// Every command the slash menu and the help card must document.
std::span<const CommandHint> command_hints();

/// Rewrites a command alias to its canonical name, preserving any arguments.
std::string normalize_command(std::string line);

/// Lowercased text after the leading slash, or nullopt when this is not a slash token.
std::optional<std::string> slash_query(const std::string& composer);

/// Whether the query matches the command name or alias, not only its description.
bool command_name_matches(const CommandHint& hint, std::string_view query);

/// Commands matching the slash token, best match first.
std::vector<const CommandHint*> matching_commands(const std::string& composer);

}  // namespace booth
