#pragma once

#include <filesystem>
#include <string>

#include "interaction/state.hpp"

namespace booth {

/// Render the current transcript as a self-contained HTML trace report.
std::string render_trace_report(const AppState& state);

/// Write a report with owner-only permissions.
bool write_trace_report(const AppState& state, const std::filesystem::path& path, std::string& error);

}  // namespace booth
