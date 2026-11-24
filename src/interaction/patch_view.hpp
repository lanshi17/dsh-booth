#pragma once

#include <cstddef>
#include <string>
#include <vector>

#include <ftxui/dom/elements.hpp>

#include "interaction/tool_diff.hpp"

namespace booth {

/// "Patch · N files · +A −R" for a Codex apply_patch card.
std::string patch_title(const std::vector<PatchFile>& patch);

/// Rows for a Codex apply_patch card: A/M/D file headers plus a bounded line preview.
ftxui::Elements patch_rows(const std::vector<PatchFile>& patch, bool expanded,
                           std::size_t content_width);

}  // namespace booth
