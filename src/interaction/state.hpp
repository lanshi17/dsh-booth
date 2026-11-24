#pragma once

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
  std::string model{"auto"};
  std::vector<ModelChoice> models;
  ReasoningOptions reasoning;
  bool busy{false};
  std::optional<PermissionRequest> permission;
  std::vector<TodoItem> todos;
  Notice notice;
  std::string error;
  bool help{false};
  std::vector<Attachment> attachments;
};

/// Apply one event. The only mutation point for the screen.
void apply(AppState& state, const Event& event);

/// The latest running tool title for the busy line, or "Working…" when none runs.
std::string active_activity(const AppState& state);

}  // namespace booth
