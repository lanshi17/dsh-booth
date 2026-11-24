#pragma once

#include <cstddef>
#include <string>
#include <string_view>
#include <vector>

namespace booth {

/// Truncate to at most `max_cells` terminal cells, keeping UTF-8 graphemes intact.
std::string ellipsize(std::string_view text, std::size_t max_cells);

/// Wrap text into rows no wider than `width` terminal cells, preserving graphemes.
std::vector<std::string> wrap_cells(const std::string& text, std::size_t width);

}  // namespace booth
