#pragma once

#include <chrono>
#include <cstddef>
#include <functional>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

#include "contract/event.hpp"

namespace booth {

struct ShellCommandResult {
  std::string output;
  int exit_code{-1};
  bool timed_out{false};
  bool truncated{false};
  std::string error;
};

/// Run a command through /bin/sh in cwd, merging stderr into stdout.
/// Captured output never exceeds output_limit bytes; timed_out is true when
/// the process group has to be killed after timeout.
ShellCommandResult run_shell_command(
    const std::string& command, const std::string& cwd,
    std::chrono::milliseconds timeout = std::chrono::seconds(60),
    std::size_t output_limit = 256 * 1024);

struct ShellCommandCompletion {
  std::string command;
  ShellCommandResult result;
  std::chrono::system_clock::time_point started;
  std::chrono::system_clock::time_point finished;
};

/// Runs one local command off-thread and delivers its result to the caller.
TranscriptEntry shell_tool_entry(const ShellCommandCompletion& completion, std::string id);

class ShellCommandJob {
 public:
  ~ShellCommandJob();
  ShellCommandJob() = default;
  ShellCommandJob(const ShellCommandJob&) = delete;
  ShellCommandJob& operator=(const ShellCommandJob&) = delete;

  bool start(std::string command, std::string cwd, std::function<void()> completed);
  bool busy() const;
  std::vector<ShellCommandCompletion> take_completed();
  void join();

 private:
  mutable std::mutex mutex_;
  std::thread thread_;
  std::vector<ShellCommandCompletion> completed_;
  bool busy_{false};
};

}  // namespace booth
