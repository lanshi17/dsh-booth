#pragma once

#include <cstddef>
#include <deque>
#include <functional>
#include <string>

#include "contract/options.hpp"
#include "interaction/scroll.hpp"
#include "interaction/state.hpp"

namespace booth {

/// True for commands that stay usable while a turn is running (aliases included).
bool local_command(const std::string& line);

/// Help, status, copy, clear, context and stats; returns true when handled.
bool handle_local_command(AppState& state, TranscriptScroll& scroll, std::size_t queued,
                          const std::string& line);

/// `/queue <prompt>`: queue text for the end of the active turn.
bool handle_queue_command(AppState& state, std::deque<std::string>& queued,
                          const std::string& line);

/// `/effort [level|next]`: read or change the reasoning effort.
bool handle_effort_command(AppState& state, const std::string& line,
                           const std::function<void(const std::string&, const ModelChoice&)>& apply);

/// `/new`: clear locally when no session exists; otherwise ask the caller to close the session.
bool start_new_session(AppState& state, TranscriptScroll& scroll, bool& close_after);

/// One line describing dsh's reported context window usage.
std::string context_text(const AppState& state);

/// One line of local transcript counters.
std::string stats_text(const AppState& state, std::size_t queued);

}  // namespace booth
