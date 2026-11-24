#pragma once

#include <ftxui/dom/elements.hpp>

#include "interaction/tool_parse.hpp"

namespace booth {

/// A dim row naming omitted bytes and the recoverable spill path.
ftxui::Element spill_notice_row(const SpillNotice& notice);

}  // namespace booth
