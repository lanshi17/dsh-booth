#include "interaction/git_commands.hpp"

#include <fcntl.h>
#include <spawn.h>
#include <sys/wait.h>
#include <unistd.h>

#include <cerrno>
#include <cstdlib>
#include <cstring>
#include <string>
#include <utility>
#include <vector>

extern char** environ;

namespace booth {
namespace {

std::string_view first_word(std::string_view line) {
  const auto end = line.find_first_of(" \t\r\n");
  return line.substr(0, end);
}

std::string argument_after(std::string_view line) {
  const auto end = line.find_first_of(" \t");
  if (end == std::string_view::npos) return {};
  const auto first = line.find_first_not_of(" \t", end);
  if (first == std::string_view::npos) return {};
  return std::string(line.substr(first));
}

std::string trim(std::string text) {
  const auto first = text.find_first_not_of(" \t\r\n");
  if (first == std::string::npos) return {};
  const auto last = text.find_last_not_of(" \t\r\n");
  return text.substr(first, last - first + 1);
}

TranscriptEntry command_entry(std::string label, std::string text, bool failed = false) {
  TranscriptEntry value;
  value.kind = EntryKind::Command;
  value.tool = std::move(label);
  value.text = std::move(text);
  if (failed) value.status = "failed";
  return value;
}

struct GitRun {
  int code{127};
  std::string output;
};

/// Run git without a shell: argv is explicit, stdout and stderr are merged.
GitRun run_git(const std::string& cwd, const std::vector<std::string>& args) {
  GitRun result;
  int pipe_fds[2] = {-1, -1};
  if (::pipe2(pipe_fds, O_CLOEXEC) != 0) {
    result.output = std::string("cannot create a git pipe: ") + std::strerror(errno);
    return result;
  }
  posix_spawn_file_actions_t actions;
  posix_spawn_file_actions_init(&actions);
  posix_spawn_file_actions_adddup2(&actions, pipe_fds[1], STDOUT_FILENO);
  posix_spawn_file_actions_adddup2(&actions, pipe_fds[1], STDERR_FILENO);
  posix_spawn_file_actions_addclose(&actions, pipe_fds[0]);
  posix_spawn_file_actions_addclose(&actions, pipe_fds[1]);
  if (!cwd.empty()) posix_spawn_file_actions_addchdir_np(&actions, cwd.c_str());

  std::vector<std::string> storage{"git"};
  storage.insert(storage.end(), args.begin(), args.end());
  std::vector<char*> argv;
  argv.reserve(storage.size() + 1);
  for (auto& value : storage) argv.push_back(value.data());
  argv.push_back(nullptr);

  pid_t pid = -1;
  const int spawned = ::posix_spawnp(&pid, "git", &actions, nullptr, argv.data(), environ);
  posix_spawn_file_actions_destroy(&actions);
  ::close(pipe_fds[1]);
  if (spawned != 0) {
    ::close(pipe_fds[0]);
    result.output = std::string("cannot run git: ") + std::strerror(spawned);
    return result;
  }

  char chunk[4096];
  for (;;) {
    const ssize_t size = ::read(pipe_fds[0], chunk, sizeof(chunk));
    if (size > 0) {
      result.output.append(chunk, static_cast<std::size_t>(size));
      continue;
    }
    if (size < 0 && errno == EINTR) continue;
    break;
  }
  ::close(pipe_fds[0]);
  int status = 0;
  while (::waitpid(pid, &status, 0) < 0) {
    if (errno != EINTR) {
      result.output = "cannot wait for git";
      return result;
    }
  }
  result.code = WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status);
  if (!result.output.empty() && result.output.back() == '\n') result.output.pop_back();
  if (result.output.size() > 200000) {
    result.output.resize(200000);
    result.output += "\n… output truncated";
  }
  return result;
}

void post_git(const ControlHooks& hooks, std::string label, std::string cwd,
              std::vector<std::string> args,
              std::function<std::string(const GitRun&)> format) {
  hooks.post([hooks, label = std::move(label), cwd = std::move(cwd), args = std::move(args),
              format = std::move(format)]() mutable {
    const GitRun run = run_git(cwd, args);
    hooks.publish(command_entry(std::move(label), format(run), run.code != 0));
  });
}

std::string commit(const ControlHooks& hooks, const std::string& message, bool stage_all) {
  const std::string body = message.empty() ? "booth: commit changes" : message;
  if (stage_all) {
    const GitRun staged = run_git(hooks.cwd, {"add", "-A"});
    if (staged.code != 0) return staged.output.empty() ? "git add failed" : staged.output;
  }
  const GitRun committed = run_git(hooks.cwd, {"commit", "-m", body});
  if (committed.code != 0) return committed.output.empty() ? "git commit failed" : committed.output;
  return committed.output.empty() ? "committed" : committed.output;
}

}  // namespace

bool is_git_command(std::string_view line) {
  const auto word = first_word(line);
  return word == "/git" || word == "/commit";
}

bool handle_git_command(const ControlHooks& hooks, std::string_view line) {
  const auto word = first_word(line);
  if (word == "/commit") {
    std::string argument = trim(argument_after(line));
    const bool stage_all = argument == "--all" || argument.rfind("--all ", 0) == 0;
    if (stage_all) argument = trim(argument.size() > 5 ? argument.substr(5) : "");
    hooks.post([hooks, argument, stage_all] {
      hooks.publish(command_entry("commit", commit(hooks, argument, stage_all)));
    });
    return true;
  }
  if (word != "/git") return false;
  const std::string argument = trim(argument_after(line));
  if (argument.empty() || argument == "status") {
    if (argument.empty()) {
      post_git(hooks, "git", hooks.cwd, {"status", "--short", "--branch"}, [](const GitRun& run) {
        return run.output.empty() ? "clean working tree" : run.output;
      });
    } else {
      post_git(hooks, "git", hooks.cwd, {"status", "--short"}, [](const GitRun& run) {
        return run.output.empty() ? "clean working tree" : run.output;
      });
    }
    return true;
  }
  if (argument == "diff" || argument == "diff --staged" || argument == "diff staged") {
    const bool staged = argument != "diff";
    post_git(hooks, "git", hooks.cwd, staged ? std::vector<std::string>{"diff", "--staged"}
                                             : std::vector<std::string>{"diff"},
             [](const GitRun& run) { return run.output.empty() ? "no diff" : run.output; });
    return true;
  }
  if (argument.rfind("stage ", 0) == 0 || argument.rfind("add ", 0) == 0) {
    const std::string path = trim(argument.substr(argument.find(' ') + 1));
    post_git(hooks, "git", hooks.cwd, {"add", "--", path}, [path](const GitRun& run) {
      return run.code == 0 ? "staged " + path : run.output;
    });
    return true;
  }
  if (argument.rfind("unstage ", 0) == 0) {
    const std::string path = trim(argument.substr(8));
    post_git(hooks, "git", hooks.cwd, {"restore", "--staged", "--", path}, [path](const GitRun& run) {
      return run.code == 0 ? "unstaged " + path : run.output;
    });
    return true;
  }
  if (argument == "commit" || argument.rfind("commit ", 0) == 0) {
    std::string message = argument.size() > 7 ? trim(argument.substr(7)) : "";
    const bool stage_all = message == "--all" || message.rfind("--all ", 0) == 0;
    if (stage_all) message = trim(message.size() > 5 ? message.substr(5) : "");
    hooks.post([hooks, message, stage_all] {
      hooks.publish(command_entry("commit", commit(hooks, message, stage_all)));
    });
    return true;
  }
  if (argument.rfind("-", 0) != 0) {
    post_git(hooks, "git", hooks.cwd, {"show", "--format=", "--patch", argument},
             [argument](const GitRun& run) {
               if (run.code != 0) return run.output.empty() ? "unknown revision " + argument : run.output;
               return run.output.empty() ? "no changes in " + argument : run.output;
             });
    return true;
  }
  hooks.publish(command_entry("git", "usage: /git [revision|status|diff [staged]|stage <path>|unstage <path>|commit [message]]", true));
  return true;
}

}  // namespace booth
