#pragma once

#include "contract/event.hpp"

#include <ftxui/dom/elements.hpp>

namespace booth {

/// "Thinking… 3s" while live, "Thought for 12s" once finished, "Thought" when replay has no start.
std::string thought_label(const TranscriptEntry& entry);

/// The OMP-style thinking head plus its dim markdown body.
ftxui::Element thought_view(const TranscriptEntry& entry);

}  // namespace booth
