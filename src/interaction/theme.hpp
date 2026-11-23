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

/// Reads `key=#rrggbb` overrides from a theme file.
std::map<std::string, std::string> load_theme(const std::filesystem::path& path);

/// Publishes overrides as `BOOTH_THEME_<KEY>` for the renderer.
void apply_theme(const std::map<std::string, std::string>& theme);

/// Reads a live theme color from the environment; invalid/missing values use the fallback.
ftxui::Color theme_color(std::string_view key, ftxui::Color fallback);

}  // namespace booth
