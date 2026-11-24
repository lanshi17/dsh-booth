#pragma once

#include <optional>
#include <string>
#include <vector>

#include "contract/event.hpp"

namespace booth {

/// One rendered line of an edit/write/patch tool call.
struct DiffLine {
  std::string text;
  int change{0};  // -1 removed, +1 added, 0 header/context
  int old_line{0};
  int new_line{0};
};

enum class PatchOp { Add, Update, Delete, Move };

struct PatchFile {
  PatchOp op{PatchOp::Update};
  std::string path;
  std::string destination;
  std::vector<DiffLine> lines;
  std::size_t added{0};
  std::size_t removed{0};
};

/// Parse the Codex `*** Begin Patch` grammar used by dsh's apply_patch tool.
std::optional<std::vector<PatchFile>> codex_patch(const TranscriptEntry& entry);

/// Extracts a reviewable diff from a tool call's raw arguments; empty when not an edit.
std::vector<DiffLine> tool_diff(const TranscriptEntry& entry);

}  // namespace booth
