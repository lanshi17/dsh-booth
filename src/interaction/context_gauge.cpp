#include "interaction/context_gauge.hpp"

#include <algorithm>
#include <string>

namespace booth {

std::string context_bar(int percent, int cells) {
  const int safe_percent = std::clamp(percent, 0, 100);
  const int safe_cells = std::max(1, cells);
  const int filled = (safe_percent * safe_cells + 50) / 100;
  std::string out;
  for (int index = 0; index < safe_cells; ++index) {
    out += index < filled ? "█" : "░";
  }
  return out;
}

ftxui::Color context_color(int percent) {
  if (percent >= 90) return ftxui::Color::RGB(255, 95, 95);
  if (percent >= 70) return ftxui::Color::RGB(255, 215, 0);
  return ftxui::Color::RGB(0, 255, 135);
}

}  // namespace booth
