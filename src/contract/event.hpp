#pragma once

#include <chrono>
#include <cstddef>
#include <optional>
#include <string>
#include <variant>
#include <vector>

#include "contract/error.hpp"
#include "contract/ids.hpp"
#include "contract/options.hpp"

namespace booth {

enum class DshPhase { Down, Starting, Ready, Degraded, Paused };

struct Problem {
  Layer layer{Layer::State};
  std::string detail;
};

struct SessionRow {
  SessionId id;
  std::string title;
  std::string status;  // new | running | idle | closed
  std::string cwd;
  std::string model;   // resolved id, or "auto"
  std::size_t turns{0};
  std::size_t context_used{0};
  std::size_t context_size{0};
  std::string preview;
  long long age_seconds{0};
  bool pinned{false};
};


struct Snapshot {
  DshPhase phase{DshPhase::Down};
  std::string detail;
  std::vector<SessionRow> sessions;
  std::optional<Problem> problem;
  std::chrono::system_clock::time_point at{};
  bool stale{false};
};

enum class EntryKind { User, Assistant, Thought, Tool, Notice, Image, Command };

struct TodoItem {
  std::string content;
  std::string status;  // pending | in_progress | completed
};

struct TranscriptEntry {
  EntryKind kind{EntryKind::Assistant};
  std::string id;    // tool call id, when the entry is a tool
  std::string text;
  std::string tool;
  std::string status;
  std::string input;      // tool call arguments, one line
  std::string raw_input;  // original arguments JSON, for diff rendering
  std::string output;     // tool result text
  bool expanded{false};   // per-entry disclosure for finished thinking
  std::chrono::system_clock::time_point started{};
  std::chrono::system_clock::time_point finished{};
};

struct StreamDelta {
  std::string text;
};

struct PermissionOption {
  std::string id;
  std::string name;
  std::string kind;  // allow_once | reject_once | allow_always | reject_always
};

struct PermissionRequest {
  RequestId id;
  SessionId session;
  std::string tool_call_id;
  std::string tool;
  std::string title;
  std::string detail;
  std::string raw_input;
  std::vector<PermissionOption> options;
};


struct SetupProgress {
  std::string step;
};

struct StreamEnd {
  std::string stop_reason;
};

struct SessionOpened {
  SessionId id;
  std::string cwd;
  std::string model;
};

struct SessionsListed {
  std::vector<SessionRow> sessions;
};

struct SessionSwitched {
  SessionId id;
  std::string title;
};

struct TranscriptReplayed {
  std::vector<TranscriptEntry> entries;
};

struct SessionClosed { std::string notice{"session closed"}; };

struct SessionConfigChanged {
  SessionId session;
  std::string config_id;
  std::string value;
};

struct SessionOptionsChanged {
  std::vector<ModelChoice> models;
  ReasoningOptions reasoning;
};

struct ConfigOptionsChanged {
  std::string options;  // configOptions JSON from a config_option_update notification
};

struct ReasoningChanged {
  std::string value;
  std::string name;
};

struct UsageChanged {
  SessionId session;
  std::size_t used{0};
  std::size_t size{0};
};

/// One shell condition finished for loop mode; `code` is its exit status.
struct LoopConditionDone {
  int code{0};
};

using EventValue = std::variant<Snapshot, TranscriptEntry, StreamDelta, PermissionRequest,
                                SetupProgress, Problem, StreamEnd, SessionOpened,
                                SessionConfigChanged, SessionOptionsChanged, ConfigOptionsChanged,
                                ReasoningChanged,
                                UsageChanged, SessionsListed, SessionSwitched, TranscriptReplayed,
                                SessionClosed, LoopConditionDone>;

struct Event {
  EventValue value;
};

}  // namespace booth
