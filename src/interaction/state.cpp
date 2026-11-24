#include "interaction/state.hpp"

#include <algorithm>
#include <set>
#include <string>

#include "interaction/tool_parse.hpp"
#include "interaction/tui_internal.hpp"
#include "interaction/view.hpp"

namespace booth {
namespace {

SessionRow* find_session(std::vector<SessionRow>& sessions, const SessionId& id) {
  for (auto& session : sessions) {
    if (session.id == id) return &session;
  }
  return nullptr;
}

bool is_read_tool(const TranscriptEntry& entry) {
  return entry.kind == EntryKind::Tool &&
         (entry.tool == "read" || entry.tool == "read_file");
}

std::size_t count_reads(const std::string& output) {
  std::size_t count = 0;
  for (std::size_t pos = output.find("<path>"); pos != std::string::npos;
       pos = output.find("<path>", pos + 6)) {
    ++count;
  }
  return count;
}

}  // namespace

void remember_todos(AppState& state, const TranscriptEntry& entry) {
  if (entry.kind != EntryKind::Tool || entry.tool != "todo_write") return;
  if (auto todos = parse_todos(entry)) state.todos = std::move(*todos);
}

void replay_todos(AppState& state) {
  state.todos.clear();
  for (const auto& entry : state.entries) remember_todos(state, entry);
}

void toggle_entry(AppState& state, std::set<std::size_t>& open_tools, std::size_t index) {
  if (index >= state.entries.size()) return;
  TranscriptEntry& entry = state.entries[index];
  if (entry.kind == EntryKind::Tool) {
    if (open_tools.erase(index) == 0) open_tools.insert(index);
  } else if (entry.kind == EntryKind::Thought &&
             entry.finished != std::chrono::system_clock::time_point{}) {
    entry.expanded = !entry.expanded;
  }
}

std::string active_activity(const AppState& state) {
  for (auto entry = state.entries.rbegin(); entry != state.entries.rend(); ++entry) {
    if (entry->kind != EntryKind::Tool) continue;
    if (entry->status == "pending" || entry->status == "running" || entry->status == "in_progress") {
      return tool_activity(*entry);
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
      for (std::size_t index = 0; index < state.entries.size(); ++index) {
        TranscriptEntry& existing = state.entries[index];
        if (existing.id != entry->id || existing.kind != entry->kind) continue;
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
        if (index > 0 && existing.status == "completed") {
          TranscriptEntry& previous = state.entries[index - 1];
          if (is_read_tool(previous) && previous.status == "completed" &&
              is_read_tool(existing) && !existing.output.empty()) {
            previous.output += "\n" + existing.output;
            previous.finished = existing.finished;
            const std::size_t reads = count_reads(previous.output);
            previous.text = "Read " + std::to_string(reads) +
                            (reads == 1 ? " file" : " files");
            state.entries.erase(state.entries.begin() + static_cast<std::ptrdiff_t>(index));
          }
        }
        if (index < state.entries.size()) remember_todos(state, state.entries[index]);
        return;
      }
    }
    state.entries.push_back(*entry);
    remember_todos(state, state.entries.back());
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
    state.todos.clear();
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
    replay_todos(state);
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
    state.todos.clear();
    state.streaming.clear();
    state.busy = false;
    state.notice = "session closed";
    return;
  }

  if (const auto* opened = std::get_if<SessionOpened>(&event.value)) {
    state.session = opened->id.value;
    state.todos.clear();
    if (SessionRow* row = find_session(state.snapshot.sessions, opened->id)) {
      if (!opened->cwd.empty()) row->cwd = opened->cwd;
      if (!opened->model.empty()) row->model = opened->model;
      return;
    }
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
    PermissionRequest pending = *request;
    if (!pending.tool_call_id.empty()) {
      for (auto entry = state.entries.rbegin(); entry != state.entries.rend(); ++entry) {
        if (entry->kind != EntryKind::Tool || entry->id != pending.tool_call_id) continue;
        if (!entry->tool.empty()) pending.tool = entry->tool;
        if (pending.title.empty()) pending.title = entry->text;
        if (pending.raw_input.empty()) pending.raw_input = entry->raw_input;
        if (pending.detail.empty()) pending.detail = entry->input;
        break;
      }
    }
    state.permission = std::move(pending);
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

  if (const auto* options = std::get_if<ConfigOptionsChanged>(&event.value)) {
    const auto parsed = nlohmann::json::parse(options->options, nullptr, false);
    if (parsed.is_discarded() || !parsed.is_array()) return;
    std::vector<ModelChoice> choices;
    const std::string display = read_model_options(parsed, choices);
    state.models = std::move(choices);
    state.reasoning = read_reasoning_options(parsed);
    if (!display.empty()) state.model = display;
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
