#pragma once

#include <string>
#include <string_view>

namespace booth {

/// One `/compact` request translated for dsh's argument-free compaction command.
struct CompactRequest {
  std::string line;    // always `/compact`
  std::string prefix;  // result label for the OMP mode or ignored focus
  std::string error;   // non-empty when the mode is unavailable
};

/// Parse OMP's `/compact [soft|remote|snapcompact|handoff] [focus]` grammar.
CompactRequest parse_compact_request(std::string_view args);

/// Translate OMP's `/goal` verbs onto dsh's `[objective|clear|edit|pause|resume]`.
struct GoalRequest {
  std::string line;
  std::string error;
};

GoalRequest parse_goal_request(std::string_view args);

}  // namespace booth
