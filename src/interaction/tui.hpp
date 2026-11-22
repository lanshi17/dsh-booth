#pragma once

#include "supervisor/config.hpp"

namespace booth {

/// Runs the interactive console. Returns the process exit code.
int run_tui(const Config& config, const Env& env);

}  // namespace booth
