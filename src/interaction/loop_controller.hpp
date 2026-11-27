#pragma once

#include <optional>
#include <string>
#include <string_view>

#include "interaction/state.hpp"

namespace booth {

struct LoopLimitConfig {
  bool duration{false};
  int iterations{0};
  long long duration_ms{0};
};

struct LoopConditionConfig {
  std::string command;
  bool until{false};
};

struct ParsedLoopArgs {
  std::optional<LoopLimitConfig> limit;
  std::optional<LoopConditionConfig> condition;
  std::string prompt;
  std::string error;
};

/// OMP `/loop` grammar: a leading count/duration, one --while/--until gate,
/// then the prompt text.
ParsedLoopArgs parse_loop_args(std::string_view args);

/// One decision made by the loop controller after a turn or a condition run.
struct LoopStep {
  std::optional<std::string> prompt;
  std::optional<std::string> condition;
  std::string notice;
};

/// Handle `/loop ...`; an inline prompt is staged in `loop_pending_prompt`.
void loop_command(AppState& state, std::string_view line, std::string& notice);
void loop_note_prompt(AppState& state, std::string_view text);
void loop_turn_finished(AppState& state);
void loop_pause(AppState& state, std::string& notice);
void loop_reset(AppState& state);
LoopStep loop_next(AppState& state);
LoopStep loop_condition_result(AppState& state, int exit_code);

/// Run one condition command with stdout/stderr discarded and a hard timeout.
int run_shell_status(const std::string& command, const std::string& cwd,
                     int timeout_ms = 60000);

}  // namespace booth
