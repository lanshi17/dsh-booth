#include "interaction/commands_ui.hpp"

#include <algorithm>
#include <cctype>
#include <string_view>
#include <utility>

#include "interaction/clipboard.hpp"
#include "interaction/commands.hpp"
#include "interaction/view.hpp"

namespace booth {
namespace {

std::string lower(std::string_view text) {
  std::string out(text);
  for (auto& ch : out) ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
  return out;
}

std::string trim(std::string_view text) {
  const auto first = text.find_first_not_of(" \t\r\n");
  if (first == std::string_view::npos) return {};
  const auto last = text.find_last_not_of(" \t\r\n");
  return std::string(text.substr(first, last - first + 1));
}

std::string argument_after(const std::string& line, std::string_view command) {
  if (line.size() <= command.size()) return {};
  return trim(std::string_view(line).substr(command.size()));
}

std::string first_word(const std::string& line) {
  const auto end = line.find_first_of(" \t\r\n");
  return line.substr(0, end);
}

}  // namespace

bool local_command(const std::string& line) {
  const std::string token = first_word(normalize_command(line));
  return token == "/help" || token == "/status" || token == "/copy" || token == "/clear" ||
         token == "/export" || token == "/quit" || token == "/context" || token == "/stats" ||
         token == "/effort" || token == "/queue";
}

bool handle_local_command(AppState& state, TranscriptScroll& scroll, std::size_t queued,
                          const std::string& line) {
  if (line == "/help") {
    state.help = true;
    return true;
  }
  if (line == "/status") {
    state.notice = status_text(state);
    return true;
  }
  if (line == "/copy") {
    copy_answer(state);
    return true;
  }
  if (line == "/clear") {
    state.entries.clear();
    state.streaming.clear();
    scroll.follow();
    state.notice = "cleared";
    return true;
  }
  if (line == "/context") {
    state.notice = context_text(state);
    return true;
  }
  if (line == "/stats") {
    state.notice = stats_text(state, queued);
    return true;
  }
  return false;
}

bool handle_queue_command(AppState& state, std::deque<std::string>& queued,
                          const std::string& line) {
  std::string text = argument_after(line, "/queue");
  if (text.empty()) {
    state.notice = "usage: /queue <prompt>";
    return true;
  }
  if (!state.busy) {
    state.notice = "not running; send it as a prompt";
    return true;
  }
  queued.push_back(std::move(text));
  state.notice = std::to_string(queued.size()) + " queued";
  return true;
}

bool handle_effort_command(AppState& state, const std::string& line,
                           const std::function<void(const std::string&, const ModelChoice&)>& apply) {
  if (state.reasoning.id.empty() || state.reasoning.choices.empty()) {
    state.notice = "no reasoning options";
    return true;
  }
  const std::string argument = argument_after(line, "/effort");
  if (argument.empty()) {
    std::string names;
    for (const auto& choice : state.reasoning.choices) {
      if (!names.empty()) names += ", ";
      names += choice.name;
    }
    state.notice = "effort " + state.reasoning.current_name + " · " + names;
    return true;
  }
  const std::string wanted = lower(argument);
  const ModelChoice* selected = nullptr;
  if (wanted == "next" || wanted == "cycle") {
    std::size_t current = 0;
    for (std::size_t index = 0; index < state.reasoning.choices.size(); ++index) {
      if (state.reasoning.choices[index].value == state.reasoning.current) current = index;
    }
    selected = &state.reasoning.choices[(current + 1) % state.reasoning.choices.size()];
  } else {
    for (const auto& choice : state.reasoning.choices) {
      if (lower(choice.name) == wanted || lower(choice.value) == wanted) {
        selected = &choice;
        break;
      }
    }
  }
  if (selected == nullptr) {
    state.notice = "usage: /effort <" +
                   [] (const ReasoningOptions& options) {
                     std::string names;
                     for (const auto& choice : options.choices) {
                       if (!names.empty()) names += "|";
                       names += choice.name;
                     }
                     return names;
                   }(state.reasoning) +
                   "|next>";
    return true;
  }
  state.reasoning.current = selected->value;
  state.reasoning.current_name = selected->name;
  state.notice = "reasoning " + selected->name;
  apply(state.reasoning.id, *selected);
  return true;
}

bool start_new_session(AppState& state, TranscriptScroll& scroll, bool& close_after) {
  close_after = false;
  if (!state.session.empty()) {
    close_after = true;
    state.notice = "starting a new session";
    return true;
  }
  state.entries.clear();
  state.streaming.clear();
  state.title.clear();
  scroll.follow();
  state.notice = "new session";
  return false;
}

std::string context_text(const AppState& state) {
  if (state.session.empty()) return "context: no active session";
  for (const auto& row : state.snapshot.sessions) {
    if (row.id.value != state.session) continue;
    if (row.context_size == 0) return "context: not reported yet";
    const auto percent = row.context_used * 100 / row.context_size;
    return "context: " + std::to_string(row.context_used) + " / " +
           std::to_string(row.context_size) + " (" + std::to_string(percent) + "%)";
  }
  return "context: not reported yet";
}

std::string stats_text(const AppState& state, std::size_t queued) {
  std::size_t user = 0;
  std::size_t assistant = 0;
  std::size_t tools = 0;
  std::size_t thoughts = 0;
  for (const auto& entry : state.entries) {
    switch (entry.kind) {
      case EntryKind::User: ++user; break;
      case EntryKind::Assistant: ++assistant; break;
      case EntryKind::Tool: ++tools; break;
      case EntryKind::Thought: ++thoughts; break;
      case EntryKind::Notice: break;
    }
  }
  return "entries " + std::to_string(state.entries.size()) + " · you " + std::to_string(user) +
         " · assistant " + std::to_string(assistant) + " · tools " + std::to_string(tools) +
         " · thoughts " + std::to_string(thoughts) + " · queued " + std::to_string(queued);
}

}  // namespace booth
