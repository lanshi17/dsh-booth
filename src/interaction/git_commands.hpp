#pragma once

#include <string>
#include <string_view>

#include "interaction/control_commands.hpp"

namespace booth {

/// `/commit` and `/git ...`: local git operations with captured output.
bool handle_git_command(const ControlHooks& hooks, std::string_view line);

/// Whether a line belongs to the git command family.
bool is_git_command(std::string_view line);

}  // namespace booth
