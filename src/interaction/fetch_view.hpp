#pragma once

#include <cstddef>
#include <string>

#include <ftxui/dom/elements.hpp>

#include "interaction/tool_parse.hpp"

namespace booth {

/// "Web fetch · host · HTTP status" for a web_fetch card.
std::string web_fetch_title(const WebFetchResult& result);

/// URL, body preview and truncation footer rows for a web_fetch card.
ftxui::Elements web_fetch_rows(const WebFetchResult& result, bool expanded,
                               std::size_t content_width);

}  // namespace booth
