#pragma once

#include <filesystem>
#include <optional>
#include <span>
#include <string>
#include <string_view>
#include <vector>

namespace booth {

struct CommandHint {
  std::string name;
  std::string description;
  std::string keywords;
  std::string aliases;  // space-separated, without the leading slash
  bool skill{false};
};

/// Every command the slash menu and the help card must document.
std::span<const CommandHint> command_hints();

/// Loads `.booth/commands/*.md` and `<state>/commands/*.md`; project wins on name collisions.
void load_file_commands(const std::filesystem::path& project, const std::filesystem::path& state);

/// Expands a registered `/name args` prompt template, or nullopt for other input.
std::optional<std::string> file_command_prompt(const std::string& line);

/// Loads user-invocable dsh skills into the slash menu for this project.
void load_skill_commands(const std::filesystem::path& cwd);

/// Whether the first token names a user-invocable skill (a prompt gesture, not a local command).
bool is_skill_prompt(const std::string& line);

/// Rewrites a command alias to its canonical name, preserving any arguments.
std::string normalize_command(std::string line);

/// Lowercased text after the leading slash, or nullopt when this is not a slash token.
std::optional<std::string> slash_query(const std::string& composer);

/// Whether the query matches the command name or alias, not only its description.
bool command_name_matches(const CommandHint& hint, std::string_view query);

/// Commands matching the slash token, best match first.
std::vector<const CommandHint*> matching_commands(const std::string& composer);

}  // namespace booth
