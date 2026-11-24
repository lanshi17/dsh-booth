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
  std::string notice;
  bool help{false};
  std::vector<Attachment> attachments;
};

/// Apply one event. The only mutation point for the screen.
void apply(AppState& state, const Event& event);

/// The latest running tool title for the busy line, or "Working…" when none runs.
std::string active_activity(const AppState& state);

}  // namespace booth
