#pragma once

#include <filesystem>
#include <map>
#include <optional>
#include <string>
#include <string_view>

#include <ftxui/screen/color.hpp>

namespace booth {

/// Parses `#rrggbb`.
std::optional<ftxui::Color> parse_color(std::string_view text);

/// Result of validating and persisting one theme override.
struct ThemeUpdateResult {
  bool success = false;
  std::string key;
  std::string color;
  std::string error;
};

/// Reads valid, supported `key=#rrggbb` overrides from a theme file.
std::map<std::string, std::string> load_theme(const std::filesystem::path& path);

/// Validates `key=#rrggbb` and updates the file without disturbing other lines.
ThemeUpdateResult update_theme_override(const std::filesystem::path& path,
                                        std::string_view assignment);

/// Publishes overrides as `BOOTH_THEME_<KEY>` for the renderer.
void apply_theme(const std::map<std::string, std::string>& theme);

/// Reads a live theme color from the environment; invalid/missing values use the fallback.
ftxui::Color theme_color(std::string_view key, ftxui::Color fallback);

}  // namespace booth
