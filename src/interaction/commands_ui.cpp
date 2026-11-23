#include "interaction/commands_ui.hpp"

#include <algorithm>
#include <cctype>
#include <filesystem>
#include <string_view>
#include <utility>

#include "interaction/commands.hpp"
#include "interaction/view.hpp"
#include "subagent/acp/messages.hpp"
#include "subagent/kernel.hpp"

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

std::string phase_name(DshPhase phase) {
  switch (phase) {
    case DshPhase::Down: return "down";
    case DshPhase::Starting: return "starting";
    case DshPhase::Ready: return "ready";
    case DshPhase::Degraded: return "degraded";
    case DshPhase::Paused: return "paused";
  }
  return "unknown";
}

std::string first_word(const std::string& line) {
  const auto end = line.find_first_of(" \t\r\n");
  return line.substr(0, end);
}

}  // namespace

const char* permission_kind(const ftxui::Event& event) {
  if (event == ftxui::Event::Character('y') || event == ftxui::Event::Return) return "allow_once";
  if (event == ftxui::Event::Character('a')) return "allow_always";
  if (event == ftxui::Event::Character('n') || event == ftxui::Event::Escape) return "reject_once";
  if (event == ftxui::Event::Character('d')) return "reject_always";
  return nullptr;
}

std::optional<ModelChoice> next_reasoning(const ReasoningOptions& options) {
  if (options.choices.empty()) return std::nullopt;
  std::size_t current = 0;
  for (std::size_t index = 0; index < options.choices.size(); ++index) {
    if (options.choices[index].value == options.current) current = index;
  }
  return options.choices[(current + 1) % options.choices.size()];
}

bool view_toggle(const ftxui::Event& event, const KeyBindings& keys, bool& expand_tools,
                 bool& hide_tools, bool& show_thoughts, AppState& state) {
  if (matches_chord(keys.tools, event)) {
    expand_tools = !expand_tools;
    state.notice = expand_tools ? "tools expanded" : "tools collapsed";
    return true;
  }
  if (matches_chord(keys.thinking, event)) {
    show_thoughts = !show_thoughts;
    state.notice = show_thoughts ? "thinking shown" : "thinking hidden";
    return true;
  }
  if (matches_chord(keys.hide_tools, event) || event.input() == "\x1b[111;6u") {
    hide_tools = !hide_tools;
    state.notice = hide_tools ? "tools hidden" : "tools shown";
    return true;
  }
  return false;
}

void answer_permission_choice(Kernel& kernel, AppState& state, const char* kind) {
  if (!state.permission || kind == nullptr) return;
  const auto& request = *state.permission;
  const auto pick = [&](std::string_view wanted) -> std::string {
    for (const auto& candidate : request.options) {
      if (candidate.kind == wanted) return candidate.id;
    }
    return {};
  };
  std::string option = pick(kind);
  if (option.empty()) {
    option = pick(std::string_view(kind).rfind("allow", 0) == 0 ? "allow_once" : "reject_once");
  }
  const std::optional<std::string> choice =
      option.empty() ? std::nullopt : std::optional<std::string>(option);
  const auto answered = kernel.answer_permission(request.id.value, choice);
  if (!answered) state.notice = answered.error().detail;
  state.permission.reset();
}

bool local_command(const std::string& line) {
  const std::string token = first_word(normalize_command(line));
  return token == "/help" || token == "/status" || token == "/copy" || token == "/clear" ||
         token == "/export" || token == "/quit" || token == "/context" || token == "/stats" ||
         token == "/effort" || token == "/queue" || token == "/session" || token == "/acp" ||
         token == "/cancel" || token == "/attach" || token == "/detach" || token == "/find";
}

std::string session_text(const AppState& state, std::size_t queued) {
  std::string out = state.session.empty() ? "session: none" : "session " + state.session;
  if (!state.title.empty()) out += " · " + state.title;
  out += " · model " + state.model;
  if (!state.reasoning.current_name.empty()) out += " · effort " + state.reasoning.current_name;
  out += " · " + context_text(state);
  if (state.busy) out += " · busy";
  if (queued > 0) out += " · " + std::to_string(queued) + " queued";
  return out;
}

std::string acp_text(const AppState& state, bool connected, const std::string& socket) {
  return "acp v" + std::to_string(acp::kProtocolVersion) + " · phase " +
         phase_name(state.snapshot.phase) + (connected ? " · connected" : " · disconnected") +
         (socket.empty() ? "" : " · socket " + socket) +
         (state.session.empty() ? "" : " · session " + state.session);
}

bool handle_local_command(AppState& state, TranscriptScroll& scroll, TranscriptSearch& search,
                          std::size_t queued, const std::string& cwd, const std::string& line) {
  (void)cwd;
  if (line == "/help") {
    state.help = true;
    return true;
  }
  if (line == "/status") {
    state.notice = status_text(state);
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
  if (line == "/session") {
    state.notice = session_text(state, queued);
    return true;
  }
  if (line == "/find" || line.rfind("/find ", 0) == 0) {
    state.notice = search.find(state, scroll, line.size() > 5 ? line.substr(5) : "");
    return true;
  }
  if (line == "/attach" || line.rfind("/attach ", 0) == 0) {
    const std::string path = argument_after(line, "/attach");
    if (path.empty()) {
      std::string names;
      for (const auto& attachment : state.attachments) {
        if (!names.empty()) names += ", ";
        names += attachment.name;
      }
      state.notice = names.empty() ? "usage: /attach <path>" : "attached: " + names;
      return true;
    }
    std::error_code error;
    const auto absolute = std::filesystem::absolute(path, error).lexically_normal();
    if (error || !std::filesystem::exists(absolute, error)) {
      state.notice = "no such file: " + path;
      return true;
    }
    const std::string uri = "file://" + absolute.string();
    for (const auto& attachment : state.attachments) {
      if (attachment.uri == uri) {
        state.notice = "already attached: " + attachment.name;
        return true;
      }
    }
    std::string name = absolute.filename().string();
    if (name.empty()) name = absolute.string();
    state.attachments.push_back({name, uri});
    state.notice = "attached " + name + " · " + std::to_string(state.attachments.size()) + " file(s)";
    return true;
  }
  if (line == "/detach" || line.rfind("/detach ", 0) == 0) {
    const std::string target = argument_after(line, "/detach");
    if (target.empty() || target == "all") {
      state.attachments.clear();
      state.notice = "attachments cleared";
    } else {
      const auto before = state.attachments.size();
      std::erase_if(state.attachments, [&](const Attachment& attachment) {
        return attachment.name == target || attachment.uri == target;
      });
      state.notice = state.attachments.size() == before ? "not attached: " + target : "detached";
    }
    return true;
  }
  return false;
}

bool dequeue_prompt(AppState& state, std::deque<std::string>& queued) {
  if (queued.empty()) return false;
  std::string text = std::move(queued.back());
  queued.pop_back();
  if (!state.composer.empty()) text += "\n" + state.composer;
  state.composer = std::move(text);
  state.notice = queued.empty() ? "" : std::to_string(queued.size()) + " queued";
  return true;
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
