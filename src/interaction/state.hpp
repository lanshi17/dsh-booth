#pragma once

#include <chrono>
#include <cstddef>
#include <optional>
#include <string>
#include <vector>

#include "contract/event.hpp"

namespace booth {

struct Attachment {
  std::string name;
  std::string uri;
};

/// A transient status line; assignment clears any previous error styling.
struct Notice {
  std::string text;
  bool error{false};

  Notice() = default;
  Notice(std::string value) : text(std::move(value)) {}

  Notice& operator=(std::string value) {
    text = std::move(value);
    error = false;
    return *this;
  }

  operator const std::string&() const { return text; }
  operator std::string&() { return text; }
  [[nodiscard]] bool empty() const { return text.empty(); }
  void clear() {
    text.clear();
    error = false;
  }
  [[nodiscard]] std::size_t find(std::string_view needle) const { return text.find(needle); }
  [[nodiscard]] bool operator==(std::string_view value) const { return text == value; }
};

struct AppState {
  Snapshot snapshot;
  std::vector<TranscriptEntry> entries;
  std::vector<SessionRow> history;
  std::string streaming;  // assistant text not yet committed at a turn end
  std::string composer;
  std::string session;
  std::string title;
  std::string approvals_path;
  std::string model{"auto"};
  std::vector<ModelChoice> models;
  ReasoningOptions reasoning;
  bool busy{false};
  bool paused{false};
  std::optional<PermissionRequest> permission;
  bool permission_expanded{false};
  bool permission_armed{false};
  std::chrono::steady_clock::time_point permission_armed_at{};
  std::chrono::steady_clock::time_point cancel_armed_at{};
  std::vector<TodoItem> todos;
  bool todo_expanded{false};  // `/todo expand` shows every HUD task instead of three rows.
  Notice notice;
  std::string error;
  bool help{false};
  std::vector<Attachment> attachments;

  // OMP-compatible loop mode. The controller owns the transitions; the TUI
  // only feeds prompts and turn boundaries.
  bool loop_enabled{false};
  bool loop_paused{false};
  bool loop_ready{false};
  bool loop_duration{false};
  int loop_iterations{0};
  std::chrono::steady_clock::time_point loop_deadline{};
  std::string loop_prompt;
  std::string loop_condition;
  bool loop_until{false};
  bool loop_condition_running{false};
  std::string loop_pending_prompt;
  bool restart_requested{false};
  bool context_reset_requested{false};  // `/clear`: close the session and let the next prompt open a fresh context.
};

/// Apply one event. The only mutation point for the screen.
void apply(AppState& state, const Event& event);

/// The latest running tool title for the busy line, or "Working…" when none runs.
std::string active_activity(const AppState& state);

}  // namespace booth
