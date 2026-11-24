#pragma once

#include <cstddef>
#include <string>
#include <string_view>

namespace booth {

/// Truncate to at most `max_cells` terminal cells, keeping UTF-8 graphemes intact.
std::string ellipsize(std::string_view text, std::size_t max_cells);

}  // namespace booth
