#pragma once

#include <string>

#include "interaction/state.hpp"

namespace booth {

/// The last committed assistant answer, or the live stream when no answer is committed yet.
std::string last_answer(const AppState& state);

/// The OSC 52 sequence for a terminal clipboard, wrapped for tmux passthrough when `$TMUX` is set.
std::string clipboard_sequence(const std::string& text);

/// Writes the text to the terminal clipboard with OSC 52. Returns false for empty text.
bool copy_to_clipboard(const std::string& text);

/// Copies the last answer and updates the notice.
void copy_answer(AppState& state);

}  // namespace booth
