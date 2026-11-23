#pragma once

#include <cstddef>
#include <vector>

#include <ftxui/dom/elements.hpp>

namespace booth {

/// Rendered rectangle of one transcript entry, in screen cells.
struct ClickRow {
  std::size_t index;
  int y_min;
  int y_max;
  int x_min;
  int x_max;
};

/// Wraps an entry so its final rendered box is recorded after layout.
ftxui::Element click_row(ftxui::Element child, std::size_t index, std::vector<ClickRow>* rows);

}  // namespace booth
