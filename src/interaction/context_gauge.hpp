#pragma once

#include <string>

#include <ftxui/screen/color.hpp>

namespace booth {

/// `cells` blocks, filled from the left for `percent` (0–100).
std::string context_bar(int percent, int cells = 8);

/// Green below 70%, yellow below 90%, red above.
ftxui::Color context_color(int percent);

}  // namespace booth
