#pragma once

#include <filesystem>
#include <optional>
#include <string>
#include <string_view>

namespace booth {

/// One Markdown prompt template discovered from a commands directory.
struct FileCommand {
  std::string name;
  std::string description;
  std::string body;
};

/// Reads `<dir>/<name>.md`; nullopt for unreadable, empty or invalid files.
std::optional<FileCommand> read_file_command(const std::filesystem::path& path);

/// Expands `$ARGUMENTS`, `$@` and `$1`–`$9`; appends unused arguments.
std::string expand_file_command(const FileCommand& command, std::string_view arguments);

}  // namespace booth
