#include <catch2/catch_test_macros.hpp>

#include <unistd.h>

#include <cstdlib>
#include <deque>
#include <filesystem>
#include <fstream>
#include <string>
#include <string_view>

#include "interaction/commands.hpp"
#include "interaction/commands_ui.hpp"
#include "interaction/scroll.hpp"
#include "interaction/state.hpp"
#include "interaction/theme.hpp"
#include "interaction/view.hpp"

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

TEST_CASE("help documents every command and alias") {
  std::string text;
  for (const auto& line : help_lines()) text += line.text + "\n";
  for (const auto& hint : command_hints()) {
    REQUIRE(text.find(hint.name) != std::string::npos);
    std::string_view rest = hint.aliases;
    while (!rest.empty()) {
      const auto space = rest.find(' ');
      const std::string_view alias = rest.substr(0, space);
      if (!alias.empty()) REQUIRE(text.find("/" + std::string(alias)) != std::string::npos);
      if (space == std::string_view::npos) break;
      rest.remove_prefix(space + 1);
    }
  }
  REQUIRE(help_visible_lines() >= 4);
  REQUIRE(help_max_offset() >= 0);
}

TEST_CASE("local commands include aliases and stay available during a run") {
  REQUIRE(local_command("/status"));
  REQUIRE(local_command("/theme accent=#123456"));
  REQUIRE(local_command("/hotkeys"));
  REQUIRE(local_command("/q"));
  REQUIRE(local_command("/export out.md"));
  REQUIRE(local_command("/queue later"));
  REQUIRE(local_command("/session"));
  REQUIRE(local_command("/acp"));
  REQUIRE(local_command("/cancel"));
  REQUIRE(local_command("/notify off"));
  REQUIRE(local_command("/attach README.md"));
  REQUIRE(local_command("/detach"));
  REQUIRE_FALSE(local_command("/new"));
  REQUIRE_FALSE(local_command("/sessions"));
  REQUIRE_FALSE(local_command("/retry"));
}

TEST_CASE("notify command reads and persists notification preference") {
  const auto dir = std::filesystem::temp_directory_path() /
                   ("booth-notify-command-" + std::to_string(::getpid()));
  std::filesystem::remove_all(dir);
  AppState state;
  TranscriptScroll scroll;
  TranscriptSearch search;

  REQUIRE(handle_local_command(state, scroll, search, 0, dir.string(), dir, "/notify"));
  REQUIRE(state.notice == "notifications on");
  REQUIRE(handle_local_command(state, scroll, search, 0, dir.string(), dir, "/notify off"));
  REQUIRE(state.notice == "notifications off");
  REQUIRE(handle_local_command(state, scroll, search, 0, dir.string(), dir, "/notify on"));
  REQUIRE(state.notice == "notifications on");
  REQUIRE(handle_local_command(state, scroll, search, 0, dir.string(), dir, "/notify maybe"));
  REQUIRE(state.notice == "usage: /notify [on|off]");
  std::filesystem::remove_all(dir);
}

TEST_CASE("display toggles flip tools, thinking and visibility") {
  AppState state;
  bool expand = false;
  bool hide = false;
  bool thoughts = true;
  KeyBindings keys;
  REQUIRE(view_toggle(ftxui::Event::Special("\x0f"), keys, expand, hide, thoughts, state));
  REQUIRE(expand);
  REQUIRE(state.notice == "tools expanded");
  REQUIRE(view_toggle(ftxui::Event::Special("\x14"), keys, expand, hide, thoughts, state));
  REQUIRE_FALSE(thoughts);
  REQUIRE(state.notice == "thinking hidden");
  REQUIRE(view_toggle(ftxui::Event::Special("\x1bo"), keys, expand, hide, thoughts, state));
  REQUIRE(hide);
  REQUIRE(state.notice == "tools hidden");
  REQUIRE_FALSE(view_toggle(ftxui::Event::Character('x'), keys, expand, hide, thoughts, state));
}

TEST_CASE("session and ACP lines describe the connection") {
  AppState state;
  REQUIRE(session_text(state, 0) == "session: none · model auto · context: no active session");
  state.session = "s-1";
  state.title = "work";
  state.model = "model-one";
  state.reasoning.current_name = "High";
  state.busy = true;
  REQUIRE(session_text(state, 2) ==
          "session s-1 · work · model model-one · effort High · context: not reported yet · busy · 2 queued");
  const std::string acp = acp_text(state, true, "/tmp/booth.sock");
  REQUIRE(acp.find("acp v1") != std::string::npos);
  REQUIRE(acp.find("connected") != std::string::npos);
  REQUIRE(acp.find("session s-1") != std::string::npos);
}

TEST_CASE("attachments resolve paths and detach") {
  const auto dir = std::filesystem::temp_directory_path() /
                   ("booth-attach-" + std::to_string(::getpid()));
  std::filesystem::remove_all(dir);
  std::filesystem::create_directories(dir);
  const auto file = dir / "notes.txt";
  { std::ofstream out(file); out << "notes"; }
  AppState state;
  TranscriptScroll scroll;
  TranscriptSearch search;
  REQUIRE(handle_local_command(state, scroll, search, 0, dir.string(), dir,
                               "/attach " + file.string()));
  REQUIRE(state.attachments.size() == 1);
  REQUIRE(state.attachments[0].name == "notes.txt");
  REQUIRE(state.attachments[0].uri.rfind("file://", 0) == 0);
  REQUIRE(handle_local_command(state, scroll, search, 0, dir.string(), dir, "/attach"));
  REQUIRE(state.notice.find("notes.txt") != std::string::npos);
  REQUIRE(handle_local_command(state, scroll, search, 0, dir.string(), dir, "/detach notes.txt"));
  REQUIRE(state.attachments.empty());
  std::filesystem::remove_all(dir);
}

TEST_CASE("theme command reloads and persists valid overrides") {
  const auto dir = std::filesystem::temp_directory_path() /
                   ("booth-command-theme-" + std::to_string(::getpid()));
  std::filesystem::create_directories(dir);
  const auto path = dir / "theme.conf";
  {
    std::ofstream out(path);
    out << "# saved palette\ntext=#abcdef\n";
  }
  AppState state;
  TranscriptScroll scroll;
  TranscriptSearch search;

  REQUIRE(handle_local_command(state, scroll, search, 0, dir.string(), dir, "/theme"));
  REQUIRE(state.notice == "theme reloaded: 1 overrides");
  REQUIRE(theme_color("text", parse_color("#000000").value()) ==
          parse_color("#abcdef").value());

  REQUIRE(handle_local_command(state, scroll, search, 0, dir.string(), dir,
                               "/theme accent=#123456"));
  REQUIRE(state.notice == "theme accent=#123456");
  REQUIRE(load_theme(path).at("accent") == "#123456");

  const auto before = load_theme(path);
  REQUIRE(handle_local_command(state, scroll, search, 0, dir.string(), dir,
                               "/theme accent=bad"));
  REQUIRE(state.notice.find("invalid theme color") != std::string::npos);
  REQUIRE(load_theme(path) == before);
  unsetenv("BOOTH_THEME_TEXT");
  unsetenv("BOOTH_THEME_ACCENT");
  std::filesystem::remove_all(dir);
}

TEST_CASE("transcript search focuses matches and cycles") {
  AppState state;
  TranscriptScroll scroll;
  TranscriptSearch search;
  for (const char* text : {"alpha", "beta", "alpha again"}) {
    TranscriptEntry entry;
    entry.kind = EntryKind::Assistant;
    entry.text = text;
    state.entries.push_back(entry);
  }
  const std::string first = search.find(state, scroll, "alpha");
  REQUIRE(first.find("match 2/2") != std::string::npos);
  REQUIRE(scroll.window(state.entries.size()).scroll == 0);
  const std::string next = search.find(state, scroll, "");
  REQUIRE(next.find("match 1/2") != std::string::npos);
  REQUIRE(search.find(state, scroll, "missing").find("no match") == 0);
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

TEST_CASE("dequeue moves the newest prompt back into the composer") {
  AppState state;
  std::deque<std::string> queued = {"first", "second"};
  state.composer = "tail";
  REQUIRE(dequeue_prompt(state, queued));
  REQUIRE(state.composer == "second\ntail");
  REQUIRE(queued.size() == 1);
  REQUIRE(dequeue_prompt(state, queued));
  REQUIRE(state.composer == "first\nsecond\ntail");
  REQUIRE_FALSE(dequeue_prompt(state, queued));
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
