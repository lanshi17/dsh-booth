#include "interaction/state.hpp"

#include <algorithm>
#include <string>

namespace booth {
namespace {

SessionRow* find_session(std::vector<SessionRow>& sessions, const SessionId& id) {
  for (auto& session : sessions) {
    if (session.id == id) return &session;
  }
  return nullptr;
}

}  // namespace

std::string active_activity(const AppState& state) {
  for (auto entry = state.entries.rbegin(); entry != state.entries.rend(); ++entry) {
    if (entry->kind != EntryKind::Tool) continue;
    if (entry->status == "pending" || entry->status == "running" || entry->status == "in_progress") {
      if (!entry->text.empty()) return entry->text;
      if (!entry->tool.empty()) return entry->tool;
    }
    break;
  }
  return "Working…";
}

void apply(AppState& state, const Event& event) {
  const auto now = std::chrono::system_clock::now();
  const auto finish_thought = [&] {
    if (state.entries.empty()) return;
    TranscriptEntry& last = state.entries.back();
    if (last.kind == EntryKind::Thought && last.finished == std::chrono::system_clock::time_point{}) {
      last.finished = now;
    }
  };

  if (const auto* snapshot = std::get_if<Snapshot>(&event.value)) {
    state.snapshot = *snapshot;
    return;
  }

  if (const auto* entry = std::get_if<TranscriptEntry>(&event.value)) {
    if (entry->kind == EntryKind::Thought) {
      if (!state.entries.empty() && state.entries.back().kind == EntryKind::Thought &&
          state.entries.back().finished == std::chrono::system_clock::time_point{}) {
        TranscriptEntry& thought = state.entries.back();
        thought.text += entry->text;
        if (thought.started == std::chrono::system_clock::time_point{}) thought.started = now;
        return;
      }
      TranscriptEntry thought = *entry;
      if (thought.started == std::chrono::system_clock::time_point{}) thought.started = now;
      state.entries.push_back(std::move(thought));
      return;
    }
    finish_thought();
    if (!entry->id.empty()) {
      for (auto& existing : state.entries) {
        if (existing.id == entry->id && existing.kind == entry->kind) {
          const auto started = existing.started;
          const std::string input = existing.input;
          const std::string raw_input = existing.raw_input;
          const std::string text = existing.text;
          const std::string tool = existing.tool;
          existing = *entry;
          if (existing.started == std::chrono::system_clock::time_point{}) existing.started = started;
          if (existing.input.empty()) existing.input = input;
          if (existing.raw_input.empty()) existing.raw_input = raw_input;
          if (existing.text.empty()) existing.text = text;
          if (existing.tool.empty()) existing.tool = tool;
          return;
        }
      }
    }
    state.entries.push_back(*entry);
    return;
  }

  if (const auto* delta = std::get_if<StreamDelta>(&event.value)) {
    finish_thought();
    state.streaming += delta->text;
    return;
  }

  if (const auto* listed = std::get_if<SessionsListed>(&event.value)) {
    state.history = listed->sessions;
    return;
  }

  if (const auto* switched = std::get_if<SessionSwitched>(&event.value)) {
    state.entries.clear();
    state.attachments.clear();
    state.streaming.clear();
    state.busy = false;
    state.session = switched->id.value;
    state.title = switched->title;
    state.notice = switched->title.empty() ? "session resumed"
                                           : "resumed " + switched->title;
    return;
  }

  if (const auto* replayed = std::get_if<TranscriptReplayed>(&event.value)) {
    state.entries = replayed->entries;
    state.streaming.clear();
    state.busy = false;
    return;
  }

  if (std::holds_alternative<SessionClosed>(event.value)) {
    state.session.clear();
    state.title.clear();
    state.models.clear();
    state.reasoning = {};
    state.attachments.clear();
    state.entries.clear();
    state.streaming.clear();
    state.busy = false;
    state.notice = "session closed";
    return;
  }

  if (const auto* opened = std::get_if<SessionOpened>(&event.value)) {
    state.session = opened->id.value;
    SessionRow row;
    row.id = opened->id;
    row.status = "idle";
    row.cwd = opened->cwd;
    row.model = opened->model;
    state.snapshot.sessions.push_back(row);
    return;
  }

  if (std::holds_alternative<StreamEnd>(event.value)) {
    finish_thought();
    if (!state.streaming.empty()) {
      TranscriptEntry entry;
      entry.kind = EntryKind::Assistant;
      entry.text = state.streaming;
      state.entries.push_back(std::move(entry));
      state.streaming.clear();
    }
    state.busy = false;
    return;
  }

  if (const auto* request = std::get_if<PermissionRequest>(&event.value)) {
    finish_thought();
    state.permission = *request;
    return;
  }


  if (const auto* config = std::get_if<SessionConfigChanged>(&event.value)) {
    if (config->config_id == "title") {
      state.title = config->value;
      return;
    }
    if (config->config_id == "model") {
      state.model = config->value;
      if (SessionRow* session = find_session(state.snapshot.sessions, config->session)) {
        session->model = config->value;
      }
    }
    return;
  }

  if (const auto* options = std::get_if<SessionOptionsChanged>(&event.value)) {
    state.models = options->models;
    state.reasoning = options->reasoning;
    return;
  }

  if (const auto* reasoning = std::get_if<ReasoningChanged>(&event.value)) {
    state.reasoning.current = reasoning->value;
    state.reasoning.current_name = reasoning->name;
    return;
  }

  if (const auto* usage = std::get_if<UsageChanged>(&event.value)) {
    if (SessionRow* session = find_session(state.snapshot.sessions, usage->session)) {
      session->context_used = usage->used;
      session->context_size = usage->size;
    }
    return;
  }

  if (const auto* problem = std::get_if<Problem>(&event.value)) {
    finish_thought();
    state.notice = problem->detail;
  }
}

}  // namespace booth
