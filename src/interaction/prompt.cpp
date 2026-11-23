#include "interaction/prompt.hpp"

#include <chrono>
#include <mutex>
#include <utility>

#include "interaction/replay.hpp"
#include "subagent/acp/messages.hpp"

namespace booth {
namespace {

constexpr auto kSessionTimeout = std::chrono::seconds(20);

bool send_config(PromptContext& context, const std::string& id, const std::string& value) {
  std::string session;
  {
    std::lock_guard<std::mutex> lock(context.mutex);
    session = context.session;
  }
  if (session.empty()) return false;
  auto selected = context.kernel.request(
      "session/set_config_option", acp::set_config_params(session, id, value),
      std::chrono::seconds(10));
  if (!selected) {
    context.publish(Event{Problem{selected.error().layer, selected.error().detail}});
    return false;
  }
  return true;
}

}  // namespace

std::string ensure_session(PromptContext& context) {
  std::string session;
  {
    std::lock_guard<std::mutex> lock(context.mutex);
    session = context.session;
  }
  if (!session.empty()) return session;
  auto created =
      context.kernel.request("session/new", acp::new_session_params(context.cwd), kSessionTimeout);
  if (!created) {
    context.publish(Event{Problem{created.error().layer, created.error().detail}});
    return {};
  }
  session = created->value("sessionId", std::string{});
  if (session.empty()) {
    context.publish(Event{Problem{Layer::Protocol, "session/new returned no sessionId"}});
    return {};
  }
  {
    std::lock_guard<std::mutex> lock(context.mutex);
    context.session = session;
  }
  context.publish(Event{SessionOpened{SessionId{session}, context.cwd, "auto"}});
  std::vector<ModelChoice> choices;
  const std::string display =
      read_model_options(created->value("configOptions", acp::Json::array()), choices);
  context.publish(Event{SessionOptionsChanged{
      std::move(choices),
      read_reasoning_options(created->value("configOptions", acp::Json::array()))}});
  if (!display.empty()) {
    context.publish(Event{SessionConfigChanged{SessionId{session}, "model", display}});
  }
  return session;
}

void run_prompt(PromptContext& context, std::string text,
                const std::vector<std::pair<std::string, std::string>>& resources,
                const std::vector<acp::ImageAttachment>& images) {
  const std::string session = ensure_session(context);
  if (session.empty()) {
    context.publish(Event{StreamEnd{}});
    return;
  }

  auto prompted = context.kernel.request(
      "session/prompt", acp::prompt_params(session, text, resources, images), std::chrono::hours(24));
  if (!prompted) {
    context.publish(Event{Problem{prompted.error().layer, prompted.error().detail}});
    context.publish(Event{StreamEnd{}});
    return;
  }
  context.publish(Event{StreamEnd{prompted->value("stopReason", std::string{})}});

  auto listed = context.kernel.request("session/list", acp::Json::object(), std::chrono::seconds(5));
  if (listed) {
    for (const auto& row : listed->value("sessions", acp::Json::array())) {
      if (row.value("sessionId", std::string{}) != session) continue;
      const std::string title = row.value("title", std::string{});
      if (!title.empty()) {
        context.publish(Event{SessionConfigChanged{SessionId{session}, "title", title}});
      }
      break;
    }
  }
}

void apply_model(PromptContext& context, const ModelChoice& choice) {
  std::string session;
  {
    std::lock_guard<std::mutex> lock(context.mutex);
    session = context.session;
  }
  if (send_config(context, "model", choice.value)) {
    context.publish(Event{SessionConfigChanged{SessionId{session}, "model", choice.name}});
  }
}

void list_sessions(PromptContext& context) {
  constexpr std::size_t kMaxSessions = 30;
  constexpr int kMaxPages = 5;
  std::vector<SessionRow> sessions;
  std::string cursor;
  for (int page = 0; page < kMaxPages && sessions.size() < kMaxSessions; ++page) {
    acp::Json params = acp::Json::object();
    if (!cursor.empty()) params["cursor"] = cursor;
    auto listed = context.kernel.request("session/list", params, std::chrono::seconds(10));
    if (!listed) {
      context.publish(Event{Problem{listed.error().layer, listed.error().detail}});
      break;
    }
    for (const auto& row : listed->value("sessions", acp::Json::array())) {
      if (sessions.size() >= kMaxSessions) break;
      SessionRow session;
      session.id = SessionId{row.value("sessionId", std::string{})};
      session.cwd = row.value("cwd", std::string{});
      session.title = row.value("title", std::string{});
      if (session.title.empty()) {
        session.title = session_title(context.sessions_dir, session.id.value);
      }
      if (session.title.empty()) session.title = "untitled";
      session.status = "idle";
      sessions.push_back(std::move(session));
    }
    cursor = listed->value("nextCursor", std::string{});
    if (cursor.empty()) break;
  }
  context.publish(Event{SessionsListed{std::move(sessions)}});
}

void resume_session(PromptContext& context, const SessionRow& selected) {
  acp::Json params = {{"sessionId", selected.id.value},
                      {"cwd", selected.cwd.empty() ? context.cwd : selected.cwd},
                      {"mcpServers", acp::Json::array()}};
  auto resumed = context.kernel.request("session/resume", params, std::chrono::seconds(20));
  if (!resumed) {
    context.publish(Event{Problem{resumed.error().layer, resumed.error().detail}});
    return;
  }
  {
    std::lock_guard<std::mutex> lock(context.mutex);
    context.session = selected.id.value;
  }
  std::vector<ModelChoice> choices;
  const std::string display =
      read_model_options(resumed->value("configOptions", acp::Json::array()), choices);
  context.publish(Event{SessionOptionsChanged{
      std::move(choices),
      read_reasoning_options(resumed->value("configOptions", acp::Json::array()))}});
  context.publish(Event{SessionSwitched{selected.id, selected.title}});
  auto entries = replay_session(context.sessions_dir, selected.id.value);
  if (!entries) {
    context.publish(Event{Problem{entries.error().layer, entries.error().detail}});
  } else if (!entries->empty()) {
    context.publish(Event{TranscriptReplayed{std::move(*entries)}});
  }
  if (!display.empty()) {
    context.publish(
        Event{SessionConfigChanged{selected.id, "model", display}});
  }
}

void cancel_turn(PromptContext& context) {
  if (context.state.permission) {
    const auto id = context.state.permission->id.value;
    context.state.permission.reset();
    context.kernel.answer_permission(id, std::nullopt);
  }
  std::string session;
  {
    std::lock_guard<std::mutex> lock(context.mutex);
    session = context.session;
  }
  if (!session.empty()) {
    context.kernel.notify("session/cancel", acp::cancel_params(session));
    context.state.notice = "cancelled";
  }
}

void close_session(PromptContext& context) {
  std::string session;
  {
    std::lock_guard<std::mutex> lock(context.mutex);
    session = context.session;
  }
  if (session.empty()) {
    context.publish(Event{Problem{Layer::State, "no active session"}});
    return;
  }
  auto closed = context.kernel.request("session/close", acp::Json{{"sessionId", session}},
                                       std::chrono::seconds(10));
  if (!closed) {
    context.publish(Event{Problem{closed.error().layer, closed.error().detail}});
    return;
  }
  {
    std::lock_guard<std::mutex> lock(context.mutex);
    context.session.clear();
  }
  context.publish(Event{SessionClosed{}});
}

void set_reasoning(PromptContext& context, const std::string& config_id,
                   const ModelChoice& choice) {
  if (config_id.empty()) return;
  if (send_config(context, config_id, choice.value)) {
    context.publish(Event{ReasoningChanged{choice.value, choice.name}});
  }
}

}  // namespace booth
