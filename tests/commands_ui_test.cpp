#include <catch2/catch_test_macros.hpp>

#include <unistd.h>

#include <cstdlib>
#include <deque>
#include <filesystem>
#include <set>
#include <fstream>
#include <iostream>
#include <sstream>
#include <string>
#include <string_view>

#include "interaction/commands.hpp"
#include "interaction/editor.hpp"
#include "interaction/commands_ui.hpp"
#include "interaction/scroll.hpp"
#include "interaction/state.hpp"
#include "interaction/theme.hpp"
#include "interaction/todo_commands.hpp"
#include "interaction/view.hpp"

using namespace booth;

TEST_CASE("command aliases normalize to canonical names") {
  REQUIRE(normalize_command("/hotkeys") == "/help");
  REQUIRE(normalize_command("/keys") == "/help");
  REQUIRE(normalize_command("/models") == "/model");
  REQUIRE(normalize_command("/resume") == "/sessions");
  REQUIRE(normalize_command("/reasoning high") == "/effort high");
  REQUIRE(normalize_command("/exit") == "/quit");
  REQUIRE(normalize_command("/setup") == "/providers");
  REQUIRE(normalize_command("/q") == "/quit");
  REQUIRE(normalize_command("/hub") == "/agents");
  REQUIRE(normalize_command("/plugin") == "/plugins");
  REQUIRE(normalize_command("/switch") == "/model");
  REQUIRE(normalize_command("/switch opus") == "/model opus");
  REQUIRE(normalize_command("/subagents") == "/agents");
  REQUIRE(normalize_command("/rewind") == "/branch");
  REQUIRE(normalize_command("/worktree") == "/wt");
  REQUIRE(normalize_command("/unknown") == "/unknown");
}

TEST_CASE("help reflects remapped keybindings") {
  KeyBindings keys;
  keys.help = Chord{Chord::Kind::Alt, 'X'};
  keys.copy = Chord{Chord::Kind::Alt, 'Y'};
  std::string text;
  for (const auto& line : help_lines(keys)) text += line.text + "\n";
  REQUIRE(text.find("Alt+X help") != std::string::npos);
  REQUIRE(text.find("Alt+Y copy") != std::string::npos);
  REQUIRE(text.find("Alt+H help") == std::string::npos);
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

TEST_CASE("transcript search skips cards the view hides") {
  AppState state;
  TranscriptScroll scroll;
  TranscriptEntry thought;
  thought.kind = EntryKind::Thought;
  thought.text = "alpha hidden reasoning";
  state.entries.push_back(thought);
  TranscriptEntry tool;
  tool.kind = EntryKind::Tool;
  tool.tool = "bash";
  tool.input = "alpha";
  state.entries.push_back(tool);
  TranscriptEntry answer;
  answer.kind = EntryKind::Assistant;
  answer.text = "alpha answer";
  state.entries.push_back(answer);

  const auto only_visible = [](const TranscriptEntry& entry) {
    return entry.kind == EntryKind::Assistant || entry.kind == EntryKind::User;
  };
  TranscriptSearch hidden_search;
  const std::string visible_match = hidden_search.find(state, scroll, "alpha", only_visible);
  REQUIRE(visible_match.find("match 1/1") != std::string::npos);
  REQUIRE(visible_match.find("entry 3") != std::string::npos);

  TranscriptSearch all;
  const std::string all_match = all.find(state, scroll, "alpha");
  REQUIRE(all_match.find("match 3/3") != std::string::npos);
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
  REQUIRE(context_text(state) == "context: 1200 / 4800 (25%) ███░░░░░░░ · 3600 left");

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

TEST_CASE("todo, usage and session controls report local state") {
  AppState state;
  TranscriptScroll scroll;
  TranscriptSearch search;
  const auto cwd = std::filesystem::temp_directory_path();
  state.todos = {{"first", "completed"}, {"second", "in_progress"}};
  TranscriptEntry bash;
  bash.kind = EntryKind::Tool;
  bash.tool = "bash";
  state.entries.push_back(bash);
  TranscriptEntry read;
  read.kind = EntryKind::Tool;
  read.tool = "read";
  state.entries.push_back(read);
  state.entries.push_back(bash);

  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd, "/todo", false, false));
  REQUIRE(state.notice == "todo 1/2 · next second");
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd, "/todo export todos.md", false, false));
  std::ifstream exported(cwd / "todos.md");
  std::string markdown((std::istreambuf_iterator<char>(exported)), std::istreambuf_iterator<char>());
  REQUIRE(markdown.find("# Todos") != std::string::npos);
  REQUIRE(markdown.find("- [x] first") != std::string::npos);
  REQUIRE(markdown.find("- [ ] second (in progress)") != std::string::npos);
  std::filesystem::remove(cwd / "todos.md");
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd, "/usage", false, false));
  REQUIRE(state.notice.find("entries 3") != std::string::npos);
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd, "/stats --port 9090", false, false));
  REQUIRE(state.notice.find("dashboard unavailable") != std::string::npos);
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd, "/stats bogus", false, false));
  REQUIRE(state.notice.text.rfind("usage: /stats", 0) == 0);
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd,
                               "/stats --port 9090 --host 127.0.0.1", false, false));
  REQUIRE(state.notice.find("dashboard unavailable") != std::string::npos);
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd, "/stats --port", false, false));
  REQUIRE(state.notice.text.rfind("usage: /stats", 0) == 0);
  state.composer.clear();
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd, "/copy prompt", false, false));
  REQUIRE(state.notice == "nothing to copy");
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd, "/copy answer", false, false));
  REQUIRE(state.notice == "nothing to copy");
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd, "/session info", false, false));
  REQUIRE(state.notice.find("session: none") == 0);
  state.session = "s-9";
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd, "/pin", false, false));
  REQUIRE(state.notice == "session pinned");
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd, "/unpin", false, false));
  REQUIRE(state.notice == "session unpinned");
  std::filesystem::remove(cwd / "session-pins");
}

TEST_CASE("clear inside a session requests a fresh context") {
  AppState state;
  TranscriptScroll scroll;
  TranscriptSearch search;
  const auto cwd = std::filesystem::temp_directory_path();
  state.session = "s-1";
  TranscriptEntry entry;
  entry.kind = EntryKind::Assistant;
  entry.text = "old answer";
  state.entries.push_back(entry);
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd, "/clear", false, false));
  REQUIRE(state.context_reset_requested);
  REQUIRE(state.entries.empty());
  REQUIRE(state.notice.find("fresh session") != std::string::npos);
  state.session = "s-2";
  state.context_reset_requested = false;
  state.busy = true;
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd, "/clear", false, false));
  REQUIRE_FALSE(state.context_reset_requested);
  REQUIRE(state.notice.find("wait for the current turn") != std::string::npos);
}

TEST_CASE("todo subcommands mutate the local list") {
  AppState state;
  TranscriptScroll scroll;
  TranscriptSearch search;
  const auto cwd = std::filesystem::temp_directory_path();
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd,
                               "/todo append write docs", false, false));
  REQUIRE(state.todos.size() == 1);
  REQUIRE(state.todos[0].status == "pending");
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd,
                               "/todo start write", false, false));
  REQUIRE(state.todos[0].status == "in_progress");
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd,
                               "/todo done write", false, false));
  REQUIRE(state.todos[0].status == "completed");
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd,
                               "/todo append ship", false, false));
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd,
                               "/todo rm ship", false, false));
  REQUIRE(state.todos.size() == 1);
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd,
                               "/todo expand", false, false));
  REQUIRE(state.todo_expanded);
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd,
                               "/todo collapse", false, false));
  REQUIRE_FALSE(state.todo_expanded);
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd,
                               "/todo rm missing", false, false));
  REQUIRE(state.notice == "no todo matches \"missing\"");
}

TEST_CASE("todo mutation helper reports usage and all-target actions") {
  std::vector<TodoItem> todos{{"one", "pending"}, {"two", "completed"}};
  std::string error;
  REQUIRE(apply_todo_action(todos, "start", "", error) == 0);
  REQUIRE(error == "usage: /todo start <task>");
  error.clear();
  REQUIRE(apply_todo_action(todos, "done", "", error) == 2);
  REQUIRE(error.empty());
  REQUIRE(todos[0].status == "completed");
  REQUIRE(apply_todo_action(todos, "drop", "all", error) == 2);
  REQUIRE(todos.empty());
}

TEST_CASE("copy subcommands use the last transcript content") {
  AppState state;
  TranscriptEntry answer;
  answer.kind = EntryKind::Assistant;
  answer.text = "```cpp\nint x = 1;\n```\nhttps://example.com";
  state.entries.push_back(answer);
  TranscriptEntry tool;
  tool.kind = EntryKind::Tool;
  tool.tool = "bash";
  tool.input = "make test";
  state.entries.push_back(tool);
  TranscriptScroll scroll;
  TranscriptSearch search;
  const auto cwd = std::filesystem::temp_directory_path();
  std::ostringstream captured;
  auto* previous = std::cout.rdbuf(captured.rdbuf());
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd, "/copy code", false, false));
  REQUIRE(state.notice == "copied code block");
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd, "/copy cmd", false, false));
  REQUIRE(state.notice == "copied command");
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd, "/copy link", false, false));
  REQUIRE(state.notice == "copied link");
  std::cout.rdbuf(previous);
  REQUIRE(captured.str().find("\x1b]52;c;") != std::string::npos);
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd, "/copy missing", false, false));
  REQUIRE(state.notice == "usage: /copy [code|cmd|link|local|answer|prompt|line]");
}

TEST_CASE("paste raw inserts clipboard text without collapsing") {
  const auto dir = std::filesystem::temp_directory_path() /
                   ("booth-paste-raw-" + std::to_string(::getpid()));
  std::filesystem::create_directories(dir);
  const auto tool = dir / "wl-paste";
  {
    std::ofstream out(tool);
    out << "#!/bin/sh\nprintf 'raw\\ntext'\n";
  }
  std::filesystem::permissions(tool, std::filesystem::perms::owner_exec |
                                         std::filesystem::perms::group_exec |
                                         std::filesystem::perms::others_exec,
                               std::filesystem::perm_options::add);
  const std::string old_path = ::getenv("PATH") == nullptr ? "" : ::getenv("PATH");
  const char* old_wayland = ::getenv("WAYLAND_DISPLAY");
  const std::string wayland = old_wayland == nullptr ? "" : old_wayland;
  ::setenv("PATH", (dir.string() + ":" + old_path).c_str(), 1);
  ::setenv("WAYLAND_DISPLAY", "test", 1);

  AppState state;
  TranscriptScroll scroll;
  TranscriptSearch search;
  const auto cwd = std::filesystem::temp_directory_path();
  std::string content;
  Editor editor(content);
  REQUIRE(handle_local_command(state, scroll, search, 0, cwd.string(), cwd, "/paste raw",
                               false, false, &editor));
  ::setenv("PATH", old_path.c_str(), 1);
  if (old_wayland != nullptr) ::setenv("WAYLAND_DISPLAY", wayland.c_str(), 1); else ::unsetenv("WAYLAND_DISPLAY");
  std::filesystem::remove_all(dir);
  REQUIRE(content == "raw\ntext");
  REQUIRE(state.notice == "raw text pasted");
}

TEST_CASE("copy line reads the composer cursor line") {
  AppState state;
  TranscriptScroll scroll;
  TranscriptSearch search;
  const auto cwd = std::filesystem::temp_directory_path();
  std::string content = "one\ntwo";
  Editor editor(content);
  std::ostringstream captured;
  auto* previous = std::cout.rdbuf(captured.rdbuf());
  const bool handled = handle_local_command(state, scroll, search, 0, cwd.string(), cwd,
                                            "/copy line", false, false, &editor);
  std::cout.rdbuf(previous);
  REQUIRE(handled);
  REQUIRE(state.notice == "copied line");
  REQUIRE(captured.str().find("dHdv") != std::string::npos);
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

TEST_CASE("models cycle with wraparound") {
  AppState state;
  state.models = {ModelChoice{"p", "one", "1"}, ModelChoice{"p", "two", "2"},
                  ModelChoice{"p", "three", "3"}};
  state.model = "two";
  auto next = cycle_model(state, 1);
  REQUIRE(next.has_value());
  REQUIRE(next->name == "three");
  auto previous = cycle_model(state, -1);
  REQUIRE(previous.has_value());
  REQUIRE(previous->name == "one");
  state.models.clear();
  REQUIRE_FALSE(cycle_model(state, 1).has_value());
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

TEST_CASE("stats report image entries when present") {
  AppState state;
  TranscriptEntry image;
  image.kind = EntryKind::Image;
  image.text = "image/png";
  state.entries.push_back(image);
  const std::string text = stats_text(state, 0);
  REQUIRE(text.find("images 1") != std::string::npos);
  REQUIRE(text.find("queued 0") != std::string::npos);
}

TEST_CASE("stats report failed tools") {
  AppState state;
  TranscriptEntry tool;
  tool.kind = EntryKind::Tool;
  tool.status = "failed";
  state.entries.push_back(tool);
  REQUIRE(stats_text(state, 0).find("failed 1") != std::string::npos);
}


TEST_CASE("every OMP slash command is recognized") {
  const char* omp[] = {"add-dir", "advisor", "agents", "branch", "browser", "btw", "changelog", "cleanse", "clear", "collab", "commit", "compact", "computer", "context", "copy", "debug", "delete", "dirs", "dump", "effort", "exit", "export", "extended-context", "extensions", "fast", "force", "forget-approvals", "fork", "fresh", "git", "goal", "guided-goal", "handoff", "hotkeys", "hub", "jobs", "join", "leave", "live", "local", "login", "logout", "loop", "marketplace", "mcp", "memory", "model", "modelpreset", "move", "new", "omfg", "open", "pause", "pin", "plan", "plan-review", "plugins", "prewalk", "queue", "quit", "ratchet", "record", "reload-plugins", "remove-dir", "rename", "restart", "resume", "retry", "security", "session", "settings", "setup", "shake", "share", "skillful", "skills", "slow", "ssh", "stats", "switch", "tan", "todo", "tools", "trace", "tree", "unpin", "usage", "vibe", "wt"};
  std::set<std::string> canonical;
  for (const auto& hint : command_hints()) canonical.insert(hint.name);
  for (const char* name : omp) {
    const auto normalized = normalize_command(std::string("/") + name);
    REQUIRE(canonical.count(normalized) == 1);
  }
}
