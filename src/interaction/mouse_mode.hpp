#pragma once

#include <filesystem>
#include <string>
#include <string_view>

#include <ftxui/component/screen_interactive.hpp>

namespace booth {

class AppState;

/// True when this process runs behind SSH (SSH_CONNECTION/SSH_TTY/SSH_CLIENT).
bool ssh_session();

/// Default: mouse capture on for a local terminal, off over tmux/SSH so the
/// terminal owns scrolling and selection.
bool default_mouse_enabled();

/// Reads `<state>/mouse`; absent or malformed uses default_mouse_enabled().
bool load_mouse_enabled(const std::filesystem::path& state_dir);

/// Persists on/off to `<state>/mouse`; false on write failure.
bool save_mouse_enabled(const std::filesystem::path& state_dir, bool enabled);

/// DEC mouse-mode enable/disable sequences (1000/1002/1003/1006/1015).
std::string mouse_enable_sequence();
std::string mouse_disable_sequence();

/// Writes and flushes a terminal mouse-mode transition and updates FTXUI's flag.
void apply_mouse_capture(ftxui::ScreenInteractive& screen, bool enabled);

/// Handles `/mouse`, `/mouse on|off`; returns the resulting enabled state.
bool handle_mouse_command(std::string_view line, ftxui::ScreenInteractive& screen, AppState& state,
                          const std::filesystem::path& state_dir);

}  // namespace booth
