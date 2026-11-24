#include "interaction/export.hpp"
#include <nlohmann/json.hpp>

#include "interaction/view.hpp"
#include <catch2/catch_test_macros.hpp>

#include <string>

#include <ftxui/dom/elements.hpp>
#include <ftxui/screen/screen.hpp>

#include "interaction/context_gauge.hpp"
#include "interaction/commands_ui.hpp"
#include "interaction/thought_view.hpp"
#include "interaction/tui_internal.hpp"
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

TEST_CASE("config option updates refresh models and effort") {
  AppState state;
  nlohmann::json options = nlohmann::json::array({
      {{"id", "model"},
       {"name", "Model"},
       {"category", "model"},
       {"currentValue", "[\"provider\",\"model-two\"]"},
       {"options",
        nlohmann::json::array({{{"group", "provider"},
                                {"name", "Provider"},
                                {"options", nlohmann::json::array({
                                    {{"value", "[\"provider\",\"model-two\"]"},
                                     {"name", "model-two"}},
                                    {{"value", "[\"provider\",\"model-three\"]"},
                                     {"name", "model-three"}},
                                })}}})}},
      {{"id", "reasoning_effort"},
       {"name", "Reasoning effort"},
       {"category", "thought_level"},
       {"currentValue", "low"},
       {"options", nlohmann::json::array({{{"value", "low"}, {"name", "Low"}},
                                           {{"value", "high"}, {"name", "High"}}})}},
  });
  apply(state, Event{ConfigOptionsChanged{options.dump()}});
  REQUIRE(state.model == "model-two");
  REQUIRE(state.models.size() == 2);
  REQUIRE(state.reasoning.current == "low");
  REQUIRE(state.reasoning.current_name == "Low");
  REQUIRE(state.reasoning.choices.size() == 2);
}

TEST_CASE("reopening a session keeps one row for usage") {
  AppState state;
  apply(state, Event{SessionOpened{SessionId{"s-1"}, "/old", "auto"}});
  apply(state, Event{SessionOpened{SessionId{"s-1"}, "/new", "model-two"}});
  REQUIRE(state.snapshot.sessions.size() == 1);
  REQUIRE(state.snapshot.sessions[0].cwd == "/new");
  REQUIRE(state.snapshot.sessions[0].model == "model-two");
  apply(state, Event{UsageChanged{SessionId{"s-1"}, 1200, 4800}});
  REQUIRE(context_text(state).find("1200 / 4800") != std::string::npos);
}

TEST_CASE("entry clicks toggle tools and finished thoughts") {
  AppState state;
  TranscriptEntry tool;
  tool.kind = EntryKind::Tool;
  state.entries.push_back(tool);
  TranscriptEntry thought;
  thought.kind = EntryKind::Thought;
  thought.started = std::chrono::system_clock::now();
  thought.finished = thought.started + std::chrono::seconds(1);
  state.entries.push_back(thought);
  TranscriptEntry live = thought;
  live.finished = {};
  state.entries.push_back(live);

  std::set<std::size_t> open;
  toggle_entry(state, open, 0);
  REQUIRE(open.count(0) == 1);
  toggle_entry(state, open, 0);
  REQUIRE(open.empty());
  toggle_entry(state, open, 1);
  REQUIRE(state.entries[1].expanded);
  toggle_entry(state, open, 1);
  REQUIRE_FALSE(state.entries[1].expanded);
  toggle_entry(state, open, 2);
  REQUIRE_FALSE(state.entries[2].expanded);
  toggle_entry(state, open, 99);
}

TEST_CASE("finished thoughts collapse until expanded") {
  TranscriptEntry entry;
  entry.kind = EntryKind::Thought;
  entry.text = "hidden reasoning";
  entry.started = std::chrono::system_clock::now() - std::chrono::seconds(3);
  entry.finished = std::chrono::system_clock::now();

  auto collapsed = thought_view(entry, false);
  ftxui::Screen screen(80, 8);
  ftxui::Render(screen, std::move(collapsed));
  const std::string text = screen.ToString();
  REQUIRE(text.find("Thought for") != std::string::npos);
  REQUIRE(text.find("click to expand") != std::string::npos);
  REQUIRE(text.find("hidden reasoning") == std::string::npos);

  auto expanded = thought_view(entry, true);
  ftxui::Screen full(80, 8);
  ftxui::Render(full, std::move(expanded));
  REQUIRE(full.ToString().find("hidden reasoning") != std::string::npos);

  entry.finished = {};
  auto live = thought_view(entry, false);
  ftxui::Screen running(80, 8);
  ftxui::Render(running, std::move(live));
  const std::string live_text = running.ToString();
  REQUIRE(live_text.find("hidden reasoning") != std::string::npos);
  REQUIRE(live_text.find("click to expand") == std::string::npos);
}

TEST_CASE("permission requests inherit the matching tool card") {
  AppState state;
  TranscriptEntry tool;
  tool.kind = EntryKind::Tool;
  tool.id = "t-1";
  tool.tool = "edit";
  tool.text = "src/a.cpp";
  tool.input = "src/a.cpp";
  tool.raw_input = R"({"file_path":"src/a.cpp","old_string":"old","new_string":"new"})";
  state.entries.push_back(tool);

  PermissionRequest request;
  request.session = SessionId{"s-1"};
  request.tool_call_id = "t-1";
  request.tool = "tool";
  apply(state, Event{request});
  REQUIRE(state.permission.has_value());
  REQUIRE(state.permission->tool == "edit");
  REQUIRE(state.permission->title == "src/a.cpp");
  REQUIRE(state.permission->raw_input == tool.raw_input);
  REQUIRE(permission_kind(ftxui::Event::Character('y'), *state.permission) == nullptr);
}

TEST_CASE("permission card lists only the advertised choices") {
  PermissionRequest request;
  request.tool = "bash";
  request.raw_input = R"({"command":"ls"})";
  request.options = {PermissionOption{"allow", "Allow once", "allow_once"},
                     PermissionOption{"deny", "Reject", "reject_once"}};
  auto view = permission_view(request);
  ftxui::Screen screen(70, 12);
  ftxui::Render(screen, std::move(view));
  const std::string text = screen.ToString();
  REQUIRE(text.find("Allow once") != std::string::npos);
  REQUIRE(text.find("Reject") != std::string::npos);
  REQUIRE(text.find("Allow always") == std::string::npos);
  REQUIRE(text.find("Deny always") == std::string::npos);
  REQUIRE(permission_kind(ftxui::Event::Character('a'), request) == nullptr);
  REQUIRE(permission_kind(ftxui::Event::Character('y'), request) != nullptr);
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

TEST_CASE("grep cards group matches by file") {
  ftxui::Terminal::SetFallbackSize({80, 24});
  TranscriptEntry entry;
  entry.kind = EntryKind::Tool;
  entry.tool = "grep";
  entry.status = "completed";
  entry.raw_input = R"({"pattern":"TODO","path":"src"})";
  entry.output = "Found 12 matches\n\nsrc/a.cpp\n";
  for (int index = 1; index <= 6; ++index) {
    entry.output += "Line " + std::to_string(index) + ": TODO a" + std::to_string(index) + "\n";
  }
  entry.output += "\nsrc/b.cpp\n";
  for (int index = 7; index <= 12; ++index) {
    entry.output += "Line " + std::to_string(index) + ": TODO b" + std::to_string(index) + "\n";
  }
  entry.output += "\n(Full grep result stored at: /tmp/grep.txt.)";

  auto collapsed = tool_view(entry, false);
  ftxui::Screen screen(80, 24);
  ftxui::Render(screen, std::move(collapsed));
  const std::string text = screen.ToString();
  REQUIRE(text.find("Grep \"TODO\" · 12 matches") != std::string::npos);
  REQUIRE(text.find("src/a.cpp") != std::string::npos);
  REQUIRE(text.find("TODO a1") != std::string::npos);
  REQUIRE(text.find("src/b.cpp") != std::string::npos);
  REQUIRE(text.find("… +2 matches · Ctrl+O") != std::string::npos);
  REQUIRE(text.find("TODO b12") == std::string::npos);

  auto expanded = tool_view(entry, true);
  ftxui::Screen full(80, 34);
  ftxui::Render(full, std::move(expanded));
  REQUIRE(full.ToString().find("TODO b12") != std::string::npos);

  entry.output = "No matches found";
  auto empty = tool_view(entry, false);
  ftxui::Screen empty_screen(80, 12);
  ftxui::Render(empty_screen, std::move(empty));
  const std::string empty_text = empty_screen.ToString();
  REQUIRE(empty_text.find("Grep \"TODO\"") != std::string::npos);
  REQUIRE(empty_text.find("No matches") != std::string::npos);
  ftxui::Terminal::SetFallbackSize({80, 24});
}

TEST_CASE("queued prompts show their text and overflow count") {
  std::deque<std::string> queued{"first\nline", "second", "third", "fourth"};
  auto view = queued_view(queued);
  ftxui::Screen screen(80, 10);
  ftxui::Render(screen, std::move(view));
  const std::string text = screen.ToString();
  REQUIRE(text.find("4 queued · Alt+Up edit") != std::string::npos);
  REQUIRE(text.find("↳ first line") != std::string::npos);
  REQUIRE(text.find("↳ second") != std::string::npos);
  REQUIRE(text.find("↳ third") != std::string::npos);
  REQUIRE(text.find("↳ +1 more") != std::string::npos);
  REQUIRE(text.find("fourth") == std::string::npos);
}

TEST_CASE("shell cards tail output and report the exit status") {
  ftxui::Terminal::SetFallbackSize({60, 24});
  TranscriptEntry entry;
  entry.kind = EntryKind::Tool;
  entry.tool = "bash";
  entry.status = "completed";
  for (int index = 1; index <= 20; ++index) {
    const std::string number = (index < 10 ? "0" : "") + std::to_string(index);
    entry.output += "out " + number + "\n";
  }
  entry.output += "[exit code: 3]";

  auto collapsed = tool_view(entry, false);
  ftxui::Screen screen(60, 24);
  ftxui::Render(screen, std::move(collapsed));
  const std::string text = screen.ToString();
  REQUIRE(text.find("… 10 earlier lines · Ctrl+O") != std::string::npos);
  REQUIRE(text.find("out 11") != std::string::npos);
  REQUIRE(text.find("out 20") != std::string::npos);
  REQUIRE(text.find("out 01") == std::string::npos);
  REQUIRE(text.find("exit 3") != std::string::npos);
  REQUIRE(text.find("[exit code: 3]") == std::string::npos);

  auto expanded = tool_view(entry, true);
  ftxui::Screen full(60, 30);
  ftxui::Render(full, std::move(expanded));
  REQUIRE(full.ToString().find("out 01") != std::string::npos);

  entry.output = "gone\n[killed by signal: SIGKILL]";
  auto killed = tool_view(entry, false);
  ftxui::Screen killed_screen(60, 12);
  ftxui::Render(killed_screen, std::move(killed));
  const std::string killed_text = killed_screen.ToString();
  REQUIRE(killed_text.find("signal SIGKILL") != std::string::npos);
  REQUIRE(killed_text.find("[killed by signal") == std::string::npos);

  entry.output = "ok";
  auto clean = tool_view(entry, false);
  ftxui::Screen clean_screen(60, 12);
  ftxui::Render(clean_screen, std::move(clean));
  REQUIRE(clean_screen.ToString().find("exit 0") != std::string::npos);
  ftxui::Terminal::SetFallbackSize({80, 24});
}

TEST_CASE("large diffs page until expanded") {
  ftxui::Terminal::SetFallbackSize({100, 60});
  TranscriptEntry entry;
  entry.kind = EntryKind::Tool;
  entry.tool = "write";
  entry.status = "completed";
  nlohmann::json raw;
  raw["file_path"] = "big.txt";
  std::string content;
  for (int index = 1; index <= 60; ++index) {
    if (!content.empty()) content += "\n";
    content += "line " + std::to_string(index);
  }
  raw["content"] = content;
  entry.raw_input = raw.dump();

  auto collapsed = tool_view(entry, false);
  ftxui::Screen screen(100, 60);
  ftxui::Render(screen, std::move(collapsed));
  const std::string text = screen.ToString();
  REQUIRE(text.find("Diff +60 −0") != std::string::npos);
  REQUIRE(text.find("line 1") != std::string::npos);
  REQUIRE(text.find("line 60") == std::string::npos);
  REQUIRE(text.find("… +21 lines · Ctrl+O") != std::string::npos);

  auto expanded = tool_view(entry, true);
  ftxui::Screen full(100, 80);
  ftxui::Render(full, std::move(expanded));
  REQUIRE(full.ToString().find("line 60") != std::string::npos);
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

TEST_CASE("todo tool renders a checklist") {
  ftxui::Terminal::SetFallbackSize({60, 16});
  TranscriptEntry entry;
  entry.kind = EntryKind::Tool;
  entry.tool = "todo_write";
  entry.text = "todo_write";
  entry.status = "completed";
  entry.raw_input =
      R"({"todos":[{"content":"Write code","status":"completed"},)"
      R"({"content":"Run tests","status":"in_progress"},)"
      R"({"content":"Ship it","status":"pending"}]})";
  entry.output = "Updated todo list: 1 pending, 1 in progress, 1 completed.";
  auto collapsed = tool_view(entry, false);
  ftxui::Screen screen(60, 16);
  ftxui::Render(screen, std::move(collapsed));
  const std::string text = screen.ToString();
  REQUIRE(text.find("Todo · 1/3") != std::string::npos);
  REQUIRE(text.find("☑") != std::string::npos);
  REQUIRE(text.find("Write code") != std::string::npos);
  REQUIRE(text.find("Run tests") != std::string::npos);
  REQUIRE(text.find("Ship it") != std::string::npos);
  REQUIRE(text.find("Updated todo list") == std::string::npos);
  ftxui::Terminal::SetFallbackSize({80, 24});
}

TEST_CASE("todo tool pages a long checklist until expanded") {
  ftxui::Terminal::SetFallbackSize({60, 20});
  TranscriptEntry entry;
  entry.kind = EntryKind::Tool;
  entry.tool = "todo_write";
  entry.status = "running";
  nlohmann::json args;
  args["todos"] = nlohmann::json::array();
  for (int index = 1; index <= 10; ++index) {
    args["todos"].push_back(
        {{"content", "Task " + std::to_string(index)}, {"status", "pending"}});
  }
  entry.raw_input = args.dump();
  auto collapsed = tool_view(entry, false);
  ftxui::Screen screen(60, 20);
  ftxui::Render(screen, std::move(collapsed));
  const std::string text = screen.ToString();
  REQUIRE(text.find("Task 8") != std::string::npos);
  REQUIRE(text.find("Task 9") == std::string::npos);
  REQUIRE(text.find("… +2 todos") != std::string::npos);

  auto expanded = tool_view(entry, true);
  ftxui::Screen full(60, 24);
  ftxui::Render(full, std::move(expanded));
  REQUIRE(full.ToString().find("Task 10") != std::string::npos);
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
  state.title = "demo";
  SessionRow row;
  row.id = SessionId{"s-1"};
  row.context_used = 50;
  row.context_size = 100;
  state.snapshot.sessions.push_back(row);
  const std::string text = status_text(state);
  REQUIRE(text.find("demo") != std::string::npos);
  REQUIRE(text.find("context 50%") != std::string::npos);
}

TEST_CASE("composer band truncates long labels") {
  AppState state;
  state.model = "a-very-long-model-name-that-keeps-going-on";
  GitStatus git;
  std::vector<std::string> lines = {"hi"};
  auto view = composer_view(state, "/very/long/path/with/many/segments/project", git, "title",
                             -1, "", lines, 0, 0, "", 0, "", 0, nullptr);
  ftxui::Screen screen(100, 12);
  ftxui::Render(screen, std::move(view));
  REQUIRE(screen.ToString().find("…") != std::string::npos);
}

TEST_CASE("export strips read envelopes") {
  AppState state;
  TranscriptEntry read;
  read.kind = EntryKind::Tool;
  read.tool = "read";
  read.output =
      "<path>src/a.cpp</path>\n<type>file</type>\n<content>\n1: one\n(End of file)\n</content>";
  state.entries.push_back(read);
  const std::string markdown = transcript_markdown(state);
  REQUIRE(markdown.find("src/a.cpp") != std::string::npos);
  REQUIRE(markdown.find("1: one") != std::string::npos);
  REQUIRE(markdown.find("<content>") == std::string::npos);
}

TEST_CASE("permission card shows a shell prompt") {
  PermissionRequest request;
  request.tool = "bash";
  request.detail = R"({"command":"rm -rf build"})";
  auto view = permission_view(request);
  ftxui::Screen screen(70, 12);
  ftxui::Render(screen, std::move(view));
  REQUIRE(screen.ToString().find("$ rm -rf build") != std::string::npos);
}

TEST_CASE("composer hides the context gauge when size is unknown") {
  AppState state;
  GitStatus git;
  std::vector<std::string> lines = {"hi"};
  auto view = composer_view(state, "/tmp", git, "title", -1, "", lines, 0, 0, "", 0, "", 0,
                             nullptr);
  ftxui::Screen screen(100, 12);
  ftxui::Render(screen, std::move(view));
  REQUIRE(screen.ToString().find("0%") == std::string::npos);
}

TEST_CASE("export includes session metadata") {
  AppState state;
  state.session = "s-1";
  state.title = "demo";
  state.model = "model-one";
  TranscriptEntry user;
  user.kind = EntryKind::User;
  user.text = "hi";
  state.entries.push_back(user);
  const std::string markdown = transcript_markdown(state);
  REQUIRE(markdown.find("- session: s-1") != std::string::npos);
  REQUIRE(markdown.find("- title: demo") != std::string::npos);
  REQUIRE(markdown.find("- model: model-one") != std::string::npos);
}

TEST_CASE("session picker shows a running status dot") {
  SessionRow row;
  row.id = SessionId{"s-1"};
  row.title = "demo";
  row.status = "running";
  auto view = session_picker_view({row}, {0}, 0, false, "");
  ftxui::Screen screen(70, 10);
  ftxui::Render(screen, std::move(view));
  bool dot = false;
  for (int y = 0; y < 10; ++y) {
    for (int x = 0; x < 70; ++x) {
      const auto& cell = screen.CellAt(x, y);
      if (cell.character == "●" && cell.foreground_color == ftxui::Color::RGB(0, 255, 135)) dot = true;
    }
  }
  REQUIRE(dot);
}
