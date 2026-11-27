#pragma once

#include <filesystem>
#include <optional>
#include <string>

namespace booth {

struct RestartMarker {
  std::string session;
  std::string cwd;
  std::string title;
};

/// Atomically persist the session to resume after exec.
bool write_restart_marker(const std::filesystem::path& file, const RestartMarker& marker,
                          std::string& error);

/// Read and remove the marker; malformed markers are discarded.
std::optional<RestartMarker> take_restart_marker(const std::filesystem::path& file);

/// Replace this process with the same booth argv. Returns only on failure.
void exec_restart();

}  // namespace booth
