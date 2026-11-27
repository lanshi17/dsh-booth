#pragma once

#include <cstddef>
#include <filesystem>
#include <string>
#include <string_view>
#include <vector>

#include "interaction/control_commands.hpp"

namespace booth {

struct AppState;
struct Notice;

/// Markdown round-trip text for the local todo list.
std::string todos_markdown(const std::vector<TodoItem>& todos);

/// Applies one `/todo` mutation (`append`, `start`, `done`, `drop`, `rm`).
/// Returns the number of changed tasks; `error` is set when nothing matched.
std::size_t apply_todo_action(std::vector<TodoItem>& todos, std::string_view action,
                              const std::string& argument, std::string& error);

/// Handles `/todo` view, copy, export and local mutations. Commits for
/// `edit`, `import`, `append`, `start`, `done`, `drop` and `rm` go through the
/// control plane first; this local path is the non-control fallback.
bool handle_todo_local(AppState& state, const std::string& line, const std::string& cwd,
                       Notice& notice);

/// Handles `/todo` mutations and external-editor round trips through the control plane.
/// Returns false for display-only forms so the local handler runs.
bool handle_todo_control(const ControlHooks& hooks, std::string_view line,
                         const std::string& session);

}  // namespace booth
