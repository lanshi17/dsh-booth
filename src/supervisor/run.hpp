#pragma once

#include <string>

#include "contract/error.hpp"
#include "subagent/kernel.hpp"

namespace booth {

struct RunRequest {
  std::string text;
  std::string cwd;
  std::string model{"auto"};
  std::chrono::milliseconds timeout{std::chrono::minutes(30)};
};

struct RunResult {
  std::string session;
  std::string stop_reason;
};

/// Open a session, optionally select a model, and run one prompt to completion.
Result<RunResult> run_task(Kernel& kernel, const RunRequest& request);

}  // namespace booth
