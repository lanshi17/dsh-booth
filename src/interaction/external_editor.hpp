#pragma once

#include <optional>
#include <string>

#include <ftxui/component/screen_interactive.hpp>

namespace booth {

/// Opens `$VISUAL`/`$EDITOR` on a temp file seeded with `initial` and returns the edited text.
std::optional<std::string> edit_externally(const std::string& initial);

/// Runs the editor with the TUI screen uninstalled, then updates `content`.
bool edit_in_screen(ftxui::ScreenInteractive& screen, std::string& content, std::string& notice);

}  // namespace booth
