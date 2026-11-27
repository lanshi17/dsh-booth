#include "interaction/todo_commands.hpp"

#include <chrono>
#include <cstddef>
#include <filesystem>
#include <fstream>
#include <optional>
#include <string>
#include <string_view>

#include <nlohmann/json.hpp>

#include <algorithm>
#include <cctype>

#include "interaction/clipboard.hpp"
#include "interaction/state.hpp"

namespace booth {
namespace {

using json = nlohmann::json;
using namespace std::chrono_literals;

std::string trim(std::string text) {
  const auto first = text.find_first_not_of(" \t\r\n");
  if (first == std::string::npos) return {};
  const auto last = text.find_last_not_of(" \t\r\n");
  return text.substr(first, last - first + 1);
}

std::string argument_after(std::string_view line) {
  const auto end = line.find_first_of(" \t");
  if (end == std::string_view::npos) return {};
  const auto first = line.find_first_not_of(" \t", end);
  if (first == std::string_view::npos) return {};
  return std::string(line.substr(first));
}

TranscriptEntry command_entry(std::string label, std::string text, bool failed = false) {
  TranscriptEntry entry;
  entry.kind = EntryKind::Command;
  entry.tool = std::move(label);
  entry.text = std::move(text);
  if (failed) entry.status = "failed";
  return entry;
}

std::string error_text(const Error& error) {
  return error.layer == Layer::Protocol ? error.detail : to_string(error);
}

std::string lower_text(std::string_view text) {
  std::string out(text);
  for (auto& ch : out) ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
  return out;
}

json todos_json(const std::vector<TodoItem>& todos) {
  json rows = json::array();
  for (const auto& item : todos) rows.push_back({{"content", item.content}, {"status", item.status}});
  return rows;
}

std::optional<json> parse_todos(const std::string& text, std::string& error) {
  json todos = json::array();
  std::size_t start = 0;
  while (start <= text.size()) {
    const auto end = text.find('\n', start);
    std::string line = trim(text.substr(start, end == std::string::npos ? std::string::npos : end - start));
    start = end == std::string::npos ? text.size() + 1 : end + 1;
    if (line.size() < 6) continue;
    const char bullet = line[0];
    if ((bullet != '-' && bullet != '*') || line[1] != ' ' || line[2] != '[' || line[4] != ']' ||
        line[5] != ' ') continue;
    const char mark = line[3];
    if (mark != ' ' && mark != 'x' && mark != 'X') continue;
    std::string content = trim(line.substr(6));
    std::string status = mark == ' ' ? "pending" : "completed";
    constexpr std::string_view progress = " (in progress)";
    if (content.size() > progress.size() &&
        content.compare(content.size() - progress.size(), progress.size(), progress) == 0) {
      content = trim(content.substr(0, content.size() - progress.size()));
      status = "in_progress";
    }
    if (content.empty()) continue;
    for (const auto& todo : todos) {
      if (todo.value("content", std::string{}) == content) {
        error = "duplicate todo: " + content;
        return std::nullopt;
      }
    }
    todos.push_back({{"content", content}, {"status", status}});
  }
  if (todos.empty()) {
    error = "no todo checkbox lines found";
    return std::nullopt;
  }
  return todos;
}

void commit_todos(const ControlHooks& hooks, const std::string& session, json todos,
                  std::string label) {
  if (hooks.control == nullptr) {
    hooks.publish(command_entry(std::move(label), "control channel unavailable", true));
    return;
  }
  const std::size_t count = todos.size();
  hooks.post([hooks, session, todos = std::move(todos), label = std::move(label), count]() mutable {
    auto result = hooks.control->request("todos.commit", {{"sessionId", session}, {"todos", std::move(todos)}}, 30s);
    if (!result) {
      hooks.publish(command_entry(label, error_text(result.error()), true));
      return;
    }
    hooks.publish(command_entry(label, "committed " + std::to_string(count) + " todos"));
  });
}

}  // namespace

std::string todos_markdown(const std::vector<TodoItem>& todos) {
  std::string out = "# Todos\n\n";
  for (const auto& item : todos) {
    out += item.status == "completed" ? "- [x] " : "- [ ] ";
    out += item.content;
    if (item.status == "in_progress") out += " (in progress)";
    out += '\n';
  }
  return out;
}

std::size_t apply_todo_action(std::vector<TodoItem>& todos, std::string_view action,
                              const std::string& argument, std::string& error) {
  const std::string task = trim(argument);
  if (action == "append") {
    if (task.empty()) {
      error = "usage: /todo append <task>";
      return 0;
    }
    todos.push_back({task, "pending"});
    return 1;
  }
  if (action != "start" && action != "done" && action != "drop" && action != "rm") {
    error = "usage: /todo [append|start|done|drop|rm] <task>";
    return 0;
  }
  if (task.empty() && action == "start") {
    error = "usage: /todo start <task>";
    return 0;
  }
  const bool all = task.empty() || lower_text(task) == "all";
  const std::string needle = lower_text(task);
  const auto matches = [&](const TodoItem& item) {
    return all || lower_text(item.content).find(needle) != std::string::npos;
  };
  std::size_t changed = 0;
  if (action == "drop" || action == "rm") {
    const auto before = todos.size();
    std::erase_if(todos, matches);
    changed = before - todos.size();
  } else {
    for (auto& item : todos) {
      if (!matches(item)) continue;
      item.status = action == "start" ? "in_progress" : "completed";
      ++changed;
    }
  }
  if (changed == 0) error = "no todo matches \"" + task + "\"";
  return changed;
}

bool handle_todo_local(AppState& state, const std::string& line, const std::string& cwd,
                       Notice& notice) {
  const std::string argument = trim(argument_after(line));
  const auto split = argument.find_first_of(" \t");
  const std::string action = argument.substr(0, split);
  const std::string rest = split == std::string::npos ? std::string{} : trim(argument.substr(split + 1));
  if (action == "expand" || action == "collapse") {
    if (!rest.empty()) {
      notice = "usage: /todo [expand|collapse]";
      return true;
    }
    state.todo_expanded = action == "expand";
    notice = state.todo_expanded ? "todo HUD expanded" : "todo HUD collapsed";
    return true;
  }
  if (action == "copy") {
    const std::string text = todos_markdown(state.todos);
    notice = copy_to_clipboard(text) ? "todos copied" : "nothing to copy";
    return true;
  }
  if (action == "export") {
    const std::string target = rest.empty() ? "TODO.md" : rest;
    const auto path = std::filesystem::path(target).is_absolute()
                          ? std::filesystem::path(target)
                          : std::filesystem::path(cwd) / target;
    std::ofstream output(path, std::ios::trunc);
    if (!output) {
      notice = "could not write " + path.string();
    } else {
      output << todos_markdown(state.todos);
      notice = "todos exported to " + path.string();
    }
    return true;
  }
  if (action == "append" || action == "start" || action == "done" || action == "drop" || action == "rm") {
    std::string error;
    const std::size_t changed = apply_todo_action(state.todos, action, rest, error);
    if (!error.empty()) {
      notice = error;
      return true;
    }
    notice = std::string(action == "append" ? "added" : action == "start" ? "started"
                        : action == "done" ? "completed" : action == "drop" ? "dropped" : "removed") +
             " " + std::to_string(changed) + " todo(s)";
    return true;
  }
  if (!action.empty()) {
    notice = "usage: /todo [edit|append|start|done|drop|rm|expand|collapse|copy|export [path]|import [path]]";
    return true;
  }
  std::size_t done = 0;
  std::string next;
  for (const auto& item : state.todos) {
    if (item.status == "completed") ++done;
    else if (next.empty()) next = item.content;
  }
  notice = state.todos.empty()
               ? "no todo list"
               : "todo " + std::to_string(done) + "/" + std::to_string(state.todos.size()) +
                     (next.empty() ? "" : " · next " + next);
  return true;
}

bool handle_todo_control(const ControlHooks& hooks, std::string_view line,
                         const std::string& session) {
  const std::string argument = trim(argument_after(line));
  const auto split = argument.find_first_of(" \t");
  const std::string action = argument.substr(0, split);
  const std::string rest = split == std::string::npos ? std::string{} : trim(argument.substr(split + 1));
  if (action == "expand" || action == "collapse") {
    if (hooks.state == nullptr) {
      hooks.publish(command_entry("todo", "no application state", true));
      return true;
    }
    hooks.state->todo_expanded = action == "expand";
    hooks.publish(command_entry("todo", hooks.state->todo_expanded ? "todo HUD expanded" : "todo HUD collapsed"));
    return true;
  }
  if (action == "append" || action == "start" || action == "done" || action == "drop" || action == "rm") {
    if (hooks.state == nullptr) {
      hooks.publish(command_entry("todo", "no application state", true));
      return true;
    }
    std::string error;
    apply_todo_action(hooks.state->todos, action, rest, error);
    if (!error.empty()) {
      hooks.publish(command_entry("todo", error, true));
      return true;
    }
    commit_todos(hooks, session, todos_json(hooks.state->todos), "todo");
    return true;
  }
  if (action != "edit" && action != "import") return false;
  std::string target = rest;
  if (target.empty()) target = "TODO.md";
  const auto path = std::filesystem::path(target).is_absolute()
                        ? std::filesystem::path(target)
                        : std::filesystem::path(hooks.cwd) / target;
  if (action == "edit") {
    if (!hooks.edit_text || hooks.state == nullptr) {
      hooks.publish(command_entry("todo", "external editor is unavailable", true));
      return true;
    }
    auto edited = hooks.edit_text(todos_markdown(hooks.state->todos));
    if (!edited) {
      hooks.publish(command_entry("todo", "external editor cancelled or failed", true));
      return true;
    }
    std::string error;
    auto todos = parse_todos(*edited, error);
    if (!todos) {
      hooks.publish(command_entry("todo", error, true));
      return true;
    }
    commit_todos(hooks, session, std::move(*todos), "todo");
    return true;
  }
  if (hooks.control == nullptr) {
    hooks.publish(command_entry("todo", "control channel unavailable", true));
    return true;
  }
  hooks.post([hooks, session, path]() mutable {
    std::ifstream input(path, std::ios::binary);
    if (!input) {
      hooks.publish(command_entry("todo", "cannot read " + path.string(), true));
      return;
    }
    const std::string text{std::istreambuf_iterator<char>(input), std::istreambuf_iterator<char>()};
    std::string error;
    auto todos = parse_todos(text, error);
    if (!todos) {
      hooks.publish(command_entry("todo", error, true));
      return;
    }
    commit_todos(hooks, session, std::move(*todos), "todo");
  });
  return true;
}

}  // namespace booth
