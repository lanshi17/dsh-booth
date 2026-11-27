#pragma once

#include <filesystem>
#include <string>

#include "interaction/state.hpp"

namespace booth {

/// Renders the visible transcript as Markdown.
std::string transcript_markdown(const AppState& state);

/// Renders the visible transcript as one self-contained HTML document.
std::string transcript_html(const AppState& state);

/// Handles `/export [--themes] [path]`; `.md` keeps Markdown, everything else writes HTML.
void export_command(AppState& state, const std::string& line,
                    const std::filesystem::path& state_dir, const std::string& cwd);

}  // namespace booth
