#pragma once

#include <cstddef>
#include <deque>
#include <filesystem>
#include <functional>
#include <optional>
#include <string>

#include <ftxui/component/event.hpp>

#include "contract/options.hpp"
#include "interaction/scroll.hpp"
#include "interaction/keymap.hpp"
#include "interaction/transcript_search.hpp"
#include "interaction/state.hpp"

namespace booth {

class Kernel;

/// True for commands that stay usable while a turn is running (aliases included).
bool local_command(const std::string& line);

/// The permission choice for a key press, or nullptr when the key does nothing.
const char* permission_kind(const ftxui::Event& event, const PermissionRequest& request);

/// Ctrl+O/Ctrl+T/Alt+O display toggles. Returns true when the event was consumed.
bool view_toggle(const ftxui::Event& event, const KeyBindings& keys, bool& expand_tools,
                 bool& hide_tools, bool& show_thoughts, AppState& state);

/// Answers the pending permission with the requested option kind.
void answer_permission_choice(Kernel& kernel, AppState& state, const char* kind);

/// Handles a key while a permission is pending, including Ctrl+O expansion and armed Enter.
void handle_permission_key(const ftxui::Event& event, AppState& state, Kernel& kernel,
                           const KeyBindings& keys);

/// The next reasoning choice after the current one, or nullopt when none are offered.
std::optional<ModelChoice> next_reasoning(const ReasoningOptions& options);

/// Help, status, copy, clear, context, stats, session info, attachments and ACP info.
bool handle_local_command(AppState& state, TranscriptScroll& scroll, TranscriptSearch& search,
                          std::size_t queued, const std::string& cwd,
                          const std::filesystem::path& state_dir, const std::string& line);

/// Moves the newest queued prompt back into the composer; false when the queue is empty.
bool dequeue_prompt(AppState& state, std::deque<std::string>& queued);

/// `/queue <prompt>`: queue text for the end of the active turn.
bool handle_queue_command(AppState& state, std::deque<std::string>& queued,
                          const std::string& line);

/// `/effort [level|next]`: read or change the reasoning effort.
bool handle_effort_command(AppState& state, const std::string& line,
                           const std::function<void(const std::string&, const ModelChoice&)>& apply);

/// `/new`: clear locally when no session exists; otherwise ask the caller to close the session.
bool start_new_session(AppState& state, TranscriptScroll& scroll, bool& close_after);

/// One line describing the active ACP session.
std::string session_text(const AppState& state, std::size_t queued);

/// One line describing the ACP connection.
std::string acp_text(const AppState& state, bool connected, const std::string& socket);

/// One line describing dsh's reported context window usage.
std::string context_text(const AppState& state);

/// One line of local transcript counters.
std::string stats_text(const AppState& state, std::size_t queued);

}  // namespace booth
