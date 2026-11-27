#include "interaction/prompt.hpp"

#include <chrono>
#include <mutex>
#include <optional>
#include <utility>

#include "interaction/mcp_config.hpp"
#include "interaction/replay.hpp"
#include "interaction/restart.hpp"
#include "subagent/acp/messages.hpp"

namespace booth {
namespace {

constexpr auto kSessionTimeout = std::chrono::seconds(20);

std::optional<acp::Json> send_config(PromptContext& context, const std::string& id,
                                      const std::string& value) {
  std::string session;
  {
    std::lock_guard<std::mutex> lock(context.mutex);
    session = context.session;
  }
  if (session.empty()) return std::nullopt;
  auto selected = context.kernel.request(
      "session/set_config_option", acp::set_config_params(session, id, value),
      std::chrono::seconds(10));
  if (!selected) {
    context.publish(Event{Problem{selected.error().layer, selected.error().detail}});
    return std::nullopt;
  }
  return *selected;
}

std::string publish_config_options(PromptContext& context, const acp::Json& response) {
  const auto options = response.value("configOptions", acp::Json::array());
  std::vector<ModelChoice> choices;
  const std::string display = read_model_options(options, choices);
  context.publish(Event{SessionOptionsChanged{
      std::move(choices), read_reasoning_options(options)}});
  return display;
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
      context.kernel.request("session/new", acp::new_session_params(context.cwd, load_mcp_servers(context.mcp_file)), kSessionTimeout);
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
  const std::string display = publish_config_options(context, *created);
  if (!display.empty()) {
    context.publish(Event{SessionConfigChanged{SessionId{session}, "model", display}});
  }
  return session;
}

void ensure_session_or_restart(PromptContext& context, const std::filesystem::path& marker) {
  if (const auto restart = take_restart_marker(marker)) {
    SessionRow row;
    row.id = SessionId{restart->session};
    row.cwd = restart->cwd.empty() ? context.cwd : restart->cwd;
    row.title = restart->title;
    resume_session(context, row);
    std::lock_guard<std::mutex> lock(context.mutex);
    if (!context.session.empty()) return;
  }
  ensure_session(context);
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
  const auto selected = send_config(context, "model", choice.value);
  if (!selected) return;
  std::string display = publish_config_options(context, *selected);
  if (display.empty()) display = choice.name;
  context.publish(Event{SessionConfigChanged{SessionId{session}, "model", display}});
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
      const SessionSummary summary =
          session_summary(context.sessions_dir, session.id.value);
      if (session.title.empty()) session.title = summary.title;
      if (session.title.empty()) session.title = "untitled";
      session.preview = summary.preview;
      session.age_seconds = summary.age_seconds;
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
                      {"mcpServers", load_mcp_servers(context.mcp_file)}};
  auto resumed = context.kernel.request("session/resume", params, std::chrono::seconds(20));
  if (!resumed) {
    context.publish(Event{Problem{resumed.error().layer, resumed.error().detail}});
    return;
  }
  {
    std::lock_guard<std::mutex> lock(context.mutex);
    context.session = selected.id.value;
  }
  const std::string display = publish_config_options(context, *resumed);
  context.publish(Event{SessionSwitched{selected.id, selected.title}});
  if (!display.empty()) {
    context.publish(Event{SessionConfigChanged{selected.id, "model", display}});
  }
  context.publish(Event{SessionOpened{selected.id,
                                      selected.cwd.empty() ? context.cwd : selected.cwd,
                                      display}});
  auto entries = replay_session(context.sessions_dir, selected.id.value);
  if (!entries) {
    context.publish(Event{Problem{entries.error().layer, entries.error().detail}});
  } else if (!entries->empty()) {
    context.publish(Event{TranscriptReplayed{std::move(*entries)}});
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

void close_session(PromptContext& context, std::string notice) {
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
  context.publish(Event{SessionClosed{std::move(notice)}});
}

void switch_session(PromptContext& context, const SessionRow& selected) {
  std::string previous;
  {
    std::lock_guard<std::mutex> lock(context.mutex);
    previous = context.session;
  }
  resume_session(context, selected);
  std::string current;
  {
    std::lock_guard<std::mutex> lock(context.mutex);
    current = context.session;
  }
  // A failed resume keeps the old session live and selected; a successful one
  // leaves the old ACP session idle, so close it by its captured id.
  if (!previous.empty() && current != previous) {
    (void)context.kernel.request("session/close", acp::Json{{"sessionId", previous}},
                                 std::chrono::seconds(10));
  }
}

void fresh_session(PromptContext& context) {
  std::string id;
  {
    std::lock_guard<std::mutex> lock(context.mutex);
    id = context.session;
  }
  if (id.empty()) return;
  SessionRow row;
  row.id = SessionId{id};
  row.cwd = context.cwd;
  row.title = context.state.title;
  close_session(context);
  {
    std::lock_guard<std::mutex> lock(context.mutex);
    if (!context.session.empty()) return;
  }
  resume_session(context, row);
}

void move_session(PromptContext& context, ControlClient& control, const std::string& id,
                  const std::string& path) {
  close_session(context);
  {
    std::lock_guard<std::mutex> lock(context.mutex);
    if (!context.session.empty()) return;
  }
  auto moved = control.request("session.move", {{"sessionId", id}, {"path", path}},
                               std::chrono::seconds(120));
  if (!moved) {
    context.publish(Event{Problem{moved.error().layer, moved.error().detail}});
    return;
  }
  SessionRow row;
  row.id = SessionId{moved->value("sessionId", std::string{})};
  row.cwd = moved->value("cwd", path);
  row.title = moved->value("title", std::string{});
  if (row.id.value.empty()) {
    context.publish(Event{Problem{Layer::Protocol, "session.move returned no sessionId"}});
    return;
  }
  switch_session(context, row);
}

void worktree_session(PromptContext& context, ControlClient& control, const std::string& id,
                      const std::string& branch) {
  close_session(context);
  {
    std::lock_guard<std::mutex> lock(context.mutex);
    if (!context.session.empty()) return;
  }
  auto moved = control.request("session.worktree", {{"sessionId", id}, {"branch", branch}},
                               std::chrono::seconds(120));
  if (!moved) {
    context.publish(Event{Problem{moved.error().layer, moved.error().detail}});
    return;
  }
  SessionRow row;
  row.id = SessionId{moved->value("sessionId", std::string{})};
  row.cwd = moved->value("cwd", std::string{});
  row.title = moved->value("title", std::string{});
  if (row.id.value.empty()) {
    context.publish(Event{Problem{Layer::Protocol, "session.worktree returned no sessionId"}});
    return;
  }
  switch_session(context, row);
}

void delete_session(PromptContext& context, ControlClient& control, const std::string& id) {
  close_session(context);
  {
    std::lock_guard<std::mutex> lock(context.mutex);
    if (!context.session.empty()) return;
  }
  auto deleted = control.request("session.delete", {{"sessionId", id}}, std::chrono::seconds(30));
  if (!deleted) {
    context.publish(Event{Problem{deleted.error().layer, deleted.error().detail}});
    return;
  }
  ensure_session(context);
}

void set_reasoning(PromptContext& context, const std::string& config_id,
                   const ModelChoice& choice) {
  if (config_id.empty()) return;
  const auto selected = send_config(context, config_id, choice.value);
  if (!selected) return;
  publish_config_options(context, *selected);
  context.publish(Event{ReasoningChanged{choice.value, choice.name}});
}

}  // namespace booth
