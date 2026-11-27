#include "interaction/commands_ui.hpp"

#include <string>

#include "interaction/context_gauge.hpp"
#include "subagent/acp/messages.hpp"

namespace booth {
namespace {

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

}  // namespace

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

std::string context_text(const AppState& state) {
  if (state.session.empty()) return "context: no active session";
  for (const auto& row : state.snapshot.sessions) {
    if (row.id.value != state.session) continue;
    if (row.context_size == 0) return "context: not reported yet";
    const auto percent = row.context_used * 100 / row.context_size;
    return "context: " + std::to_string(row.context_used) + " / " +
           std::to_string(row.context_size) + " (" + std::to_string(percent) + "%) " +
           context_bar(percent, 10) + " · " +
           std::to_string(row.context_size - row.context_used) + " left";
  }
  return "context: not reported yet";
}

std::string stats_text(const AppState& state, std::size_t queued) {
  std::size_t user = 0;
  std::size_t assistant = 0;
  std::size_t tools = 0;
  std::size_t thoughts = 0;
  std::size_t images = 0;
  std::size_t failed = 0;
  for (const auto& entry : state.entries) {
    switch (entry.kind) {
      case EntryKind::User: ++user; break;
      case EntryKind::Assistant: ++assistant; break;
      case EntryKind::Tool:
        ++tools;
        if (entry.status == "failed") ++failed;
        break;
      case EntryKind::Thought: ++thoughts; break;
      case EntryKind::Notice: break;
      case EntryKind::Command: break;
      case EntryKind::Image: ++images; break;
    }
  }
  std::string out = "entries " + std::to_string(state.entries.size()) + " · you " +
                    std::to_string(user) + " · assistant " + std::to_string(assistant) +
                    " · tools " + std::to_string(tools) + " · thoughts " +
                    std::to_string(thoughts);
  if (images > 0) out += " · images " + std::to_string(images);
  if (failed > 0) out += " · failed " + std::to_string(failed);
  return out + " · queued " + std::to_string(queued);
}

}  // namespace booth
