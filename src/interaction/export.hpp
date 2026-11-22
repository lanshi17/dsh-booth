#pragma once

#include <filesystem>
#include <string>

#include "interaction/state.hpp"

namespace booth {

/// Renders the visible transcript as Markdown.
std::string transcript_markdown(const AppState& state);

/// Handles `/export [path]`, writes the file and sets the notice.
void export_command(AppState& state, const std::string& line,
                    const std::filesystem::path& state_dir, const std::string& cwd);

}  // namespace booth
