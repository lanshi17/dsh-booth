#pragma once

#include <string>
#include <string_view>

#include <ftxui/component/screen_interactive.hpp>

namespace booth {

/// True for `/providers` with optional whitespace-separated arguments.
bool is_provider_command(std::string_view line);

/// The argument after `/providers`, trimmed.
std::string provider_command_argument(std::string_view line);

/// Run the standalone provider editor with the terminal restored. Returns an
/// empty string on success, or a notice for the caller.
std::string launch_provider_editor(ftxui::ScreenInteractive& screen, const std::string& argument);

/// Handle `/providers [name|new]`; writes the resulting notice and returns false
/// for every other line.
bool handle_provider_command(std::string_view line, ftxui::ScreenInteractive& screen,
                             std::string& notice);

}  // namespace booth
