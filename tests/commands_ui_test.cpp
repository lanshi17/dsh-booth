#include <catch2/catch_test_macros.hpp>

#include <deque>
#include <string>

#include "interaction/commands.hpp"
#include "interaction/commands_ui.hpp"
#include "interaction/scroll.hpp"
#include "interaction/state.hpp"

using namespace booth;

TEST_CASE("command aliases normalize to canonical names") {
  REQUIRE(normalize_command("/hotkeys") == "/help");
  REQUIRE(normalize_command("/keys") == "/help");
  REQUIRE(normalize_command("/models") == "/model");
  REQUIRE(normalize_command("/resume") == "/sessions");
  REQUIRE(normalize_command("/reasoning high") == "/effort high");
  REQUIRE(normalize_command("/exit") == "/quit");
  REQUIRE(normalize_command("/q") == "/quit");
  REQUIRE(normalize_command("/unknown") == "/unknown");
}

TEST_CASE("local commands include aliases and stay available during a run") {
  REQUIRE(local_command("/status"));
  REQUIRE(local_command("/hotkeys"));
  REQUIRE(local_command("/q"));
  REQUIRE(local_command("/export out.md"));
  REQUIRE(local_command("/queue later"));
  REQUIRE_FALSE(local_command("/new"));
  REQUIRE_FALSE(local_command("/sessions"));
  REQUIRE_FALSE(local_command("/retry"));
}

TEST_CASE("context and stats describe the state") {
  AppState state;
  REQUIRE(context_text(state) == "context: no active session");

  state.session = "s-1";
  SessionRow row;
  row.id = SessionId{"s-1"};
  row.context_used = 1200;
  row.context_size = 4800;
  state.snapshot.sessions.push_back(row);
  REQUIRE(context_text(state) == "context: 1200 / 4800 (25%)");

  TranscriptEntry user;
  user.kind = EntryKind::User;
  state.entries.push_back(user);
  TranscriptEntry assistant;
  assistant.kind = EntryKind::Assistant;
  state.entries.push_back(assistant);
  TranscriptEntry tool;
  tool.kind = EntryKind::Tool;
  state.entries.push_back(tool);
  REQUIRE(stats_text(state, 2) ==
          "entries 3 · you 1 · assistant 1 · tools 1 · thoughts 0 · queued 2");
}

TEST_CASE("queue accepts text only while a turn is running") {
  AppState state;
  std::deque<std::string> queued;
  REQUIRE(handle_queue_command(state, queued, "/queue later"));
  REQUIRE(queued.empty());
  REQUIRE(state.notice == "not running; send it as a prompt");

  state.busy = true;
  REQUIRE(handle_queue_command(state, queued, "/queue later"));
  REQUIRE(queued.size() == 1);
  REQUIRE(queued.front() == "later");
  REQUIRE(state.notice == "1 queued");

  REQUIRE(handle_queue_command(state, queued, "/queue"));
  REQUIRE(state.notice == "usage: /queue <prompt>");
}

TEST_CASE("effort reads, matches by name and cycles") {
  AppState state;
  state.reasoning.id = "reasoning_effort";
  state.reasoning.current = "high";
  state.reasoning.current_name = "High";
  state.reasoning.choices = {ModelChoice{"", "Low", "low"}, ModelChoice{"", "High", "high"}};
  ModelChoice applied;
  std::string applied_id;
  const auto apply = [&](const std::string& id, const ModelChoice& choice) {
    applied_id = id;
    applied = choice;
  };

  REQUIRE(handle_effort_command(state, "/effort", apply));
  REQUIRE(state.notice == "effort High · Low, High");

  REQUIRE(handle_effort_command(state, "/effort low", apply));
  REQUIRE(applied_id == "reasoning_effort");
  REQUIRE(applied.value == "low");

  REQUIRE(handle_effort_command(state, "/effort next", apply));
  REQUIRE(applied.value == "high");
}

TEST_CASE("new drops the local view or asks to close the session") {
  TranscriptScroll scroll;
  AppState state;
  state.entries.push_back(TranscriptEntry{});
  bool close_after = false;
  REQUIRE_FALSE(start_new_session(state, scroll, close_after));
  REQUIRE_FALSE(close_after);
  REQUIRE(state.entries.empty());
  REQUIRE(state.notice == "new session");

  state.session = "s-1";
  REQUIRE(start_new_session(state, scroll, close_after));
  REQUIRE(close_after);
  REQUIRE(state.notice == "starting a new session");
}
