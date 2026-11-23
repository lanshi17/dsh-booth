#include "interaction/export.hpp"
#include <nlohmann/json.hpp>

#include "interaction/view.hpp"
#include <catch2/catch_test_macros.hpp>

#include <string>

#include "interaction/state.hpp"

using namespace booth;

TEST_CASE("streamed text becomes one assistant entry at the turn end") {
  AppState state;
  apply(state, Event{StreamDelta{"hel"}});
  apply(state, Event{StreamDelta{"lo"}});
  REQUIRE(state.streaming == "hello");
  REQUIRE(state.entries.empty());

  state.busy = true;
  apply(state, Event{StreamEnd{"end_turn"}});
  REQUIRE(state.streaming.empty());
  REQUIRE(state.entries.size() == 1);
  REQUIRE(state.entries[0].kind == EntryKind::Assistant);
  REQUIRE(state.entries[0].text == "hello");
  REQUIRE_FALSE(state.busy);
}

TEST_CASE("a tool entry is updated in place by id") {
  AppState state;
  TranscriptEntry tool;
  tool.kind = EntryKind::Tool;
  tool.id = "t-1";
  tool.tool = "bash";
  tool.status = "pending";
  apply(state, Event{tool});
  REQUIRE(state.entries.size() == 1);

  tool.status = "completed";
  apply(state, Event{tool});
  REQUIRE(state.entries.size() == 1);
  REQUIRE(state.entries[0].status == "completed");
}

TEST_CASE("permission requests open a modal and a problem is a notice") {
  AppState state;
  PermissionRequest request;
  request.id = RequestId{"p-1"};
  request.session = SessionId{"s-1"};
  request.tool = "bash";
  request.options = {PermissionOption{"allow", "Allow", "allow_once"}};
  apply(state, Event{request});
  REQUIRE(state.permission.has_value());
  REQUIRE(state.permission->tool == "bash");

  apply(state, Event{Problem{Layer::Kernel, "connection closed"}});
  REQUIRE(state.notice == "connection closed");
}

TEST_CASE("sessions keep their model and usage") {
  AppState state;
  SessionRow row;
  row.id = SessionId{"s-1"};
  state.snapshot.sessions.push_back(row);

  apply(state, Event{SessionConfigChanged{SessionId{"s-1"}, "model", "deepseek-v4-pro"}});
  REQUIRE(state.model == "deepseek-v4-pro");
  REQUIRE(state.snapshot.sessions[0].model == "deepseek-v4-pro");

  apply(state, Event{UsageChanged{SessionId{"s-1"}, 100, 1000}});
  REQUIRE(state.snapshot.sessions[0].context_used == 100);
  REQUIRE(state.snapshot.sessions[0].context_size == 1000);
}

TEST_CASE("session listing and switching reset the visible transcript") {
  AppState state;
  TranscriptEntry entry;
  entry.kind = EntryKind::Assistant;
  entry.text = "old answer";
  apply(state, Event{entry});
  state.streaming = "partial";

  SessionRow row;
  row.id = SessionId{"s-old"};
  row.title = "old session";
  row.cwd = "/tmp";
  apply(state, Event{SessionsListed{{row}}});
  REQUIRE(state.history.size() == 1);
  REQUIRE(state.history[0].title == "old session");

  apply(state, Event{SessionSwitched{SessionId{"s-old"}, "old session"}});
  REQUIRE(state.session == "s-old");
  REQUIRE(state.title == "old session");
  REQUIRE(state.entries.empty());
  REQUIRE(state.streaming.empty());
  REQUIRE_FALSE(state.busy);
}

TEST_CASE("closing the session clears the transcript and title") {
  AppState state;
  state.session = "s-1";
  state.title = "demo";
  TranscriptEntry entry;
  entry.kind = EntryKind::Assistant;
  entry.text = "answer";
  apply(state, Event{entry});
  apply(state, Event{SessionClosed{}});
  REQUIRE(state.session.empty());
  REQUIRE(state.title.empty());
  REQUIRE(state.entries.empty());
  REQUIRE_FALSE(state.busy);
}

TEST_CASE("model options keep provider groups") {
  const auto options = nlohmann::json::parse(R"([
    {"id":"model","category":"model","currentValue":"[\"provider\",\"model-one\"]",
     "options":[{"group":"provider","name":"Provider","options":[
       {"value":"[\"provider\",\"model-one\"]","name":"model-one"},
       {"value":"[\"provider\",\"model-two\"]","name":"model-two"}]},
       {"group":"other","name":"Other","options":[
       {"value":"[\"other\",\"model-three\"]","name":"model-three"}]}]}
  ])");
  std::vector<booth::ModelChoice> choices;
  const std::string display = booth::read_model_options(options, choices);
  REQUIRE(display == "model-one");
  REQUIRE(choices.size() == 3);
  REQUIRE(choices[0].provider == "Provider");
  REQUIRE(choices[1].provider == "Provider");
  REQUIRE(choices[2].provider == "Other");
}

TEST_CASE("transcripts export as markdown") {
  booth::AppState state;
  booth::TranscriptEntry user;
  user.kind = booth::EntryKind::User;
  user.text = "question";
  state.entries.push_back(user);
  booth::TranscriptEntry assistant;
  assistant.kind = booth::EntryKind::Assistant;
  assistant.text = "answer";
  state.entries.push_back(assistant);
  booth::TranscriptEntry tool;
  tool.kind = booth::EntryKind::Tool;
  tool.tool = "bash";
  tool.status = "completed";
  tool.input = "ls";
  tool.output = "alpha";
  state.entries.push_back(tool);

  const std::string markdown = booth::transcript_markdown(state);
  REQUIRE(markdown.find("## You") != std::string::npos);
  REQUIRE(markdown.find("question") != std::string::npos);
  REQUIRE(markdown.find("### Tool: bash") != std::string::npos);
  REQUIRE(markdown.find("alpha") != std::string::npos);
}

TEST_CASE("session options and reasoning changes land in state") {
  AppState state;
  SessionOptionsChanged options;
  options.models = {ModelChoice{"provider", "one", "[\"provider\",\"one\"]"}};
  options.reasoning.id = "reasoning_effort";
  options.reasoning.current = "high";
  options.reasoning.current_name = "High";
  apply(state, Event{std::move(options)});
  REQUIRE(state.models.size() == 1);
  REQUIRE(state.models[0].name == "one");
  REQUIRE(state.reasoning.current_name == "High");

  apply(state, Event{ReasoningChanged{"low", "Low"}});
  REQUIRE(state.reasoning.current == "low");
  REQUIRE(state.reasoning.current_name == "Low");
  REQUIRE(state.reasoning.id == "reasoning_effort");

  apply(state, Event{SessionClosed{}});
  REQUIRE(state.models.empty());
  REQUIRE(state.reasoning.choices.empty());
}

TEST_CASE("an oversized composer keeps the cursor in an eight-row window") {
  auto window = row_window(12, 11, 8);
  REQUIRE(window.begin == 4);
  REQUIRE(window.end == 12);
  REQUIRE(window.hidden_above == 4);
  REQUIRE(window.hidden_below == 0);

  window = row_window(12, 0, 8);
  REQUIRE(window.begin == 0);
  REQUIRE(window.end == 8);
  REQUIRE(window.hidden_below == 4);

  window = row_window(3, 2, 8);
  REQUIRE(window.begin == 0);
  REQUIRE(window.end == 3);
  REQUIRE(window.hidden_above == 0);
  REQUIRE(window.hidden_below == 0);
}
