#pragma once

#include <filesystem>
#include <string>

#include "interaction/state.hpp"

namespace booth {

/// Local commands outside the control plane: open the last link, dump the
/// transcript, and list the state directory.
bool handle_extra_command(AppState& state, const std::string& line, const std::string& cwd,
                          const std::filesystem::path& state_dir);

}  // namespace booth
