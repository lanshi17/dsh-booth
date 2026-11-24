#pragma once

#include <cstddef>
#include <string>

#include <ftxui/dom/elements.hpp>

namespace booth {

/// A bounded, indented preview of pretty-printed JSON output.
ftxui::Elements json_rows(const std::string& json, bool expanded, std::size_t content_width);

}  // namespace booth
