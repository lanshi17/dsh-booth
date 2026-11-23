#pragma once

#include <string>
#include <vector>

#include "contract/event.hpp"

namespace booth {

/// One rendered line of an edit/write/patch tool call.
struct DiffLine {
  std::string text;
  int change{0};  // -1 removed, +1 added, 0 header/context
};

/// Extracts a reviewable diff from a tool call's raw arguments; empty when not an edit.
std::vector<DiffLine> tool_diff(const TranscriptEntry& entry);

}  // namespace booth
