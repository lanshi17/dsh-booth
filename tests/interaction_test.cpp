#include "interaction/export.hpp"
#include <nlohmann/json.hpp>

#include "interaction/view.hpp"
#include <catch2/catch_test_macros.hpp>

#include <string>

#include <ftxui/dom/elements.hpp>
#include <ftxui/screen/screen.hpp>

#include "interaction/context_gauge.hpp"
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

TEST_CASE("thought chunks coalesce and finish on the next output") {
  AppState state;
  TranscriptEntry thought;
  thought.kind = EntryKind::Thought;
  thought.text = "first ";
  apply(state, Event{thought});
  thought.text = "second";
  apply(state, Event{thought});

  REQUIRE(state.entries.size() == 1);
  REQUIRE(state.entries[0].kind == EntryKind::Thought);
  REQUIRE(state.entries[0].text == "first second");
  REQUIRE(state.entries[0].started != std::chrono::system_clock::time_point{});
  REQUIRE(state.entries[0].finished == std::chrono::system_clock::time_point{});

  apply(state, Event{StreamDelta{"answer"}});
  REQUIRE(state.entries[0].finished != std::chrono::system_clock::time_point{});

  thought.text = "later";
  apply(state, Event{thought});
  REQUIRE(state.entries.size() == 2);
  REQUIRE(state.entries[1].text == "later");
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

TEST_CASE("consecutive read results merge into one card") {
  AppState state;
  const auto read_event = [](const std::string& id, const std::string& path,
                             const std::string& body, const std::string& status) {
    TranscriptEntry entry;
    entry.kind = EntryKind::Tool;
    entry.id = id;
    entry.tool = "read";
    entry.text = path;
    entry.status = status;
    if (status == "completed") {
      entry.output = "<path>" + path + "</path>\n<type>file</type>\n<content>\n" + body +
                     "\n(End of file)\n</content>";
    }
    return entry;
  };
  apply(state, Event{read_event("r-1", "src/a.cpp", "1: one", "pending")});
  apply(state, Event{read_event("r-1", "src/a.cpp", "1: one", "completed")});
  apply(state, Event{read_event("r-2", "src/b.cpp", "1: two", "pending")});
  apply(state, Event{read_event("r-2", "src/b.cpp", "1: two", "completed")});
  REQUIRE(state.entries.size() == 1);
  REQUIRE(state.entries[0].text == "Read 2 files");
  REQUIRE(state.entries[0].output.find("src/a.cpp") != std::string::npos);
  REQUIRE(state.entries[0].output.find("src/b.cpp") != std::string::npos);
}

TEST_CASE("grouped read cards collapse to one line per file") {
  TranscriptEntry entry;
  entry.kind = EntryKind::Tool;
  entry.tool = "read";
  entry.text = "Read 2 files";
  entry.output =
      "<path>src/a.cpp</path>\n<type>file</type>\n<content>\n1: one\n2: two\n"
      "(End of file - total 2 lines)\n</content>\n"
      "<path>src/b.cpp</path>\n<type>file</type>\n<content>\n1: only\n"
      "(End of file - total 1 lines)\n</content>";
  auto collapsed = tool_view(entry, false);
  ftxui::Screen screen(80, 14);
  ftxui::Render(screen, std::move(collapsed));
  const std::string text = screen.ToString();
  REQUIRE(text.find("Read 2 files") != std::string::npos);
  REQUIRE(text.find("src/a.cpp · 2 lines") != std::string::npos);
  REQUIRE(text.find("src/b.cpp · 1 line") != std::string::npos);
  REQUIRE(text.find("1: one") == std::string::npos);

  auto expanded = tool_view(entry, true);
  ftxui::Screen full(80, 20);
  ftxui::Render(full, std::move(expanded));
  const std::string all = full.ToString();
  REQUIRE(all.find("1: one") != std::string::npos);
  REQUIRE(all.find("1: only") != std::string::npos);
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

TEST_CASE("active activity follows the latest running tool") {
  AppState state;
  TranscriptEntry tool;
  tool.kind = EntryKind::Tool;
  tool.id = "t-1";
  tool.tool = "read";
  tool.text = "Reading the config";
  tool.status = "pending";
  state.entries.push_back(tool);
  REQUIRE(active_activity(state) == "Reading the config");

  tool.status = "completed";
  state.entries[0] = tool;
  REQUIRE(active_activity(state) == "Working…");

  tool.status = "in_progress";
  tool.text.clear();
  state.entries[0] = tool;
  REQUIRE(active_activity(state) == "read");
}

TEST_CASE("context gauge fills and changes color at thresholds") {
  REQUIRE(context_bar(0, 6) == "░░░░░░");
  REQUIRE(context_bar(50, 6) == "███░░░");
  REQUIRE(context_bar(100, 6) == "██████");
  REQUIRE(context_color(69) == ftxui::Color::RGB(0, 255, 135));
  REQUIRE(context_color(70) == ftxui::Color::RGB(255, 215, 0));
  REQUIRE(context_color(90) == ftxui::Color::RGB(255, 95, 95));
}

TEST_CASE("permission card previews an edit diff") {
  PermissionRequest request;
  request.tool = "edit";
  request.detail =
      R"({"file_path":"src/a.cpp","old_string":"old","new_string":"new"})";
  auto view = permission_view(request);
  ftxui::Screen screen(70, 20);
  ftxui::Render(screen, std::move(view));
  const std::string text = screen.ToString();
  REQUIRE(text.find("src/a.cpp") != std::string::npos);
  REQUIRE(text.find("- old") != std::string::npos);
  REQUIRE(text.find("+ new") != std::string::npos);
}

TEST_CASE("read tool cards collapse long output") {
  TranscriptEntry entry;
  entry.kind = EntryKind::Tool;
  entry.tool = "read";
  entry.text = "src/a.cpp";
  entry.status = "completed";
  entry.output = "one\ntwo\nthree\nfour\nfive\nsix";

  auto collapsed = tool_view(entry, false);
  ftxui::Screen compact(70, 20);
  ftxui::Render(compact, std::move(collapsed));
  const std::string compact_text = compact.ToString();
  REQUIRE(compact_text.find("one") != std::string::npos);
  REQUIRE(compact_text.find("three") != std::string::npos);
  REQUIRE(compact_text.find("four") == std::string::npos);
  REQUIRE(compact_text.find("… +3 lines") != std::string::npos);

  auto expanded = tool_view(entry, true);
  ftxui::Screen full(70, 20);
  ftxui::Render(full, std::move(expanded));
  REQUIRE(full.ToString().find("six") != std::string::npos);
}

TEST_CASE("read envelopes render clean numbered cards") {
  TranscriptEntry entry;
  entry.kind = EntryKind::Tool;
  entry.tool = "read";
  entry.text = "read";
  entry.status = "completed";
  entry.output =
      "<path>src/b.cpp</path>\n<type>file</type>\n<content>\n"
      "1: one\n2: two\n3: three\n4: four\n\n(End of file - total 4 lines)\n</content>";

  auto collapsed = tool_view(entry, false);
  ftxui::Screen compact(70, 20);
  ftxui::Render(compact, std::move(collapsed));
  const std::string compact_text = compact.ToString();
  REQUIRE(compact_text.find("src/b.cpp") != std::string::npos);
  REQUIRE(compact_text.find("1: one") != std::string::npos);
  REQUIRE(compact_text.find("3: three") != std::string::npos);
  REQUIRE(compact_text.find("4: four") == std::string::npos);
  REQUIRE(compact_text.find("… +1 lines") != std::string::npos);
  REQUIRE(compact_text.find("(End of file") != std::string::npos);
  REQUIRE(compact_text.find("<content>") == std::string::npos);

  auto expanded = tool_view(entry, true);
  ftxui::Screen full(70, 20);
  ftxui::Render(full, std::move(expanded));
  REQUIRE(full.ToString().find("4: four") != std::string::npos);
}

TEST_CASE("assistant images render as placeholders") {
  TranscriptEntry entry;
  entry.kind = EntryKind::Image;
  entry.text = "image/png";
  entry.tool = "4B";
  auto view = entry_element(entry);
  ftxui::Screen screen(60, 4);
  ftxui::Render(screen, std::move(view));
  const std::string text = screen.ToString();
  REQUIRE(text.find("[image: image/png · 4B]") != std::string::npos);
}

TEST_CASE("tool titles truncate on terminal cells") {
  ftxui::Terminal::SetFallbackSize({24, 10});
  TranscriptEntry entry;
  entry.kind = EntryKind::Tool;
  entry.tool = "read";
  entry.output =
      "<path>中文路径很长很长.cpp</path>\n<type>file</type>\n<content>\n1: one\n"
      "(End of file)\n</content>";
  auto view = tool_view(entry, false);
  ftxui::Screen screen(24, 10);
  ftxui::Render(screen, std::move(view));
  const std::string text = screen.ToString();
  REQUIRE(text.find("…") != std::string::npos);
  REQUIRE(text.find("中文路径很长很长.cpp") == std::string::npos);
  ftxui::Terminal::SetFallbackSize({80, 24});
}

TEST_CASE("shell tool cards show a command prompt") {
  ftxui::Terminal::SetFallbackSize({60, 8});
  TranscriptEntry entry;
  entry.kind = EntryKind::Tool;
  entry.tool = "bash";
  entry.input = "echo hi";
  entry.status = "completed";
  auto view = tool_view(entry, false);
  ftxui::Screen screen(60, 8);
  ftxui::Render(screen, std::move(view));
  const std::string text = screen.ToString();
  REQUIRE(text.find("│ $") != std::string::npos);
  REQUIRE(text.find("echo hi") != std::string::npos);
  ftxui::Terminal::SetFallbackSize({80, 24});
}

TEST_CASE("session picker truncates CJK titles by cells") {
  SessionRow row;
  row.id = SessionId{"s-1"};
  row.title = "中文会话标题很长很长很长很长很长很长很长很长";
  row.cwd = "/very/long/path/with/many/segments/project";
  auto view = session_picker_view({row}, {0}, 0, false, "");
  ftxui::Screen screen(70, 12);
  ftxui::Render(screen, std::move(view));
  REQUIRE(screen.ToString().find("…") != std::string::npos);
}

TEST_CASE("tool footer keeps failure status with duration") {
  ftxui::Terminal::SetFallbackSize({60, 8});
  TranscriptEntry entry;
  entry.kind = EntryKind::Tool;
  entry.tool = "bash";
  entry.status = "failed";
  entry.started = std::chrono::system_clock::now();
  entry.finished = entry.started + std::chrono::milliseconds(12);
  auto view = tool_view(entry, false);
  ftxui::Screen screen(60, 8);
  ftxui::Render(screen, std::move(view));
  const std::string text = screen.ToString();
  REQUIRE(text.find("failed") != std::string::npos);
  REQUIRE(text.find("12ms") != std::string::npos);
  ftxui::Terminal::SetFallbackSize({80, 24});
}

TEST_CASE("model picker truncates long labels by cells") {
  std::vector<ModelChoice> choices{{"provider", "a-very-long-model-name-that-keeps-going-on", "v"}};
  auto view = model_picker_view(choices, {0}, 0, "", "");
  ftxui::Screen screen(60, 10);
  ftxui::Render(screen, std::move(view));
  REQUIRE(screen.ToString().find("…") != std::string::npos);
}

TEST_CASE("command menu truncates long descriptions") {
  ftxui::Terminal::SetFallbackSize({40, 10});
  auto view = command_menu_view("/queue", 0, nullptr);
  ftxui::Screen screen(40, 10);
  ftxui::Render(screen, std::move(view));
  REQUIRE(screen.ToString().find("…") != std::string::npos);
  ftxui::Terminal::SetFallbackSize({80, 24});
}

TEST_CASE("status text reports context percentage") {
  AppState state;
  state.snapshot.phase = DshPhase::Ready;
  state.session = "s-1";
  SessionRow row;
  row.id = SessionId{"s-1"};
  row.context_used = 50;
  row.context_size = 100;
  state.snapshot.sessions.push_back(row);
  const std::string text = status_text(state);
  REQUIRE(text.find("context 50%") != std::string::npos);
}
