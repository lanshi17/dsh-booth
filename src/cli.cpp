#include "cli.hpp"

#include <CLI/CLI.hpp>
#include <nlohmann/json.hpp>

#include <unistd.h>

#include <atomic>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <mutex>
#include <optional>
#include <string>

#include "contract/error.hpp"
#include "contract/event.hpp"
#include "subagent/kernel.hpp"
#include "supervisor/config.hpp"
#include "supervisor/daemon.hpp"
#include "supervisor/logs.hpp"
#include "supervisor/run.hpp"
#include "interaction/tui.hpp"
#include "supervisor/supervisor.hpp"

namespace booth {
namespace {

using Json = nlohmann::json;

int fail(const Error& error) {
  std::cerr << "booth: " << to_string(error) << '\n';
  return 1;
}

std::string phase_name(DshPhase phase) {
  switch (phase) {
    case DshPhase::Down: return "down";
    case DshPhase::Starting: return "starting";
    case DshPhase::Ready: return "ready";
    case DshPhase::Degraded: return "degraded";
    case DshPhase::Paused: return "paused";
  }
  return "unknown";
}

std::string kind_name(EntryKind kind) {
  switch (kind) {
    case EntryKind::User: return "user";
    case EntryKind::Assistant: return "assistant";
    case EntryKind::Thought: return "thought";
    case EntryKind::Tool: return "tool";
    case EntryKind::Notice: return "notice";
  }
  return "unknown";
}

std::string event_json(const Event& event) {
  Json value;
  if (const auto* snapshot = std::get_if<Snapshot>(&event.value)) {
    value = {{"type", "snapshot"}, {"phase", phase_name(snapshot->phase)},
             {"detail", snapshot->detail}, {"stale", snapshot->stale}};
  } else if (const auto* entry = std::get_if<TranscriptEntry>(&event.value)) {
    value = {{"type", "entry"}, {"kind", kind_name(entry->kind)}, {"id", entry->id},
             {"tool", entry->tool}, {"status", entry->status}, {"text", entry->text}};
  } else if (const auto* delta = std::get_if<StreamDelta>(&event.value)) {
    value = {{"type", "delta"}, {"text", delta->text}};
  } else if (const auto* request = std::get_if<PermissionRequest>(&event.value)) {
    value = {{"type", "permission"}, {"id", request->id.value},
             {"session", request->session.value}, {"tool", request->tool},
             {"detail", request->detail}};
  } else if (const auto* problem = std::get_if<Problem>(&event.value)) {
    value = {{"type", "problem"}, {"layer", std::string(layer_name(problem->layer))},
             {"detail", problem->detail}};
  } else if (const auto* end = std::get_if<StreamEnd>(&event.value)) {
    value = {{"type", "end"}, {"stopReason", end->stop_reason}};
  } else if (const auto* config = std::get_if<SessionConfigChanged>(&event.value)) {
    value = {{"type", "config"}, {"session", config->session.value},
             {"id", config->config_id}, {"value", config->value}};
  } else if (const auto* usage = std::get_if<UsageChanged>(&event.value)) {
    value = {{"type", "usage"}, {"session", usage->session.value},
             {"used", usage->used}, {"size", usage->size}};
  } else if (const auto* progress = std::get_if<SetupProgress>(&event.value)) {
    value = {{"type", "progress"}, {"step", progress->step}};
  } else {
    value = {{"type", "unknown"}};
  }
  return value.dump();
}

struct RunOptions {
  std::string text;
  std::string cwd;
  std::string model;
  bool allow{false};
  bool json{false};
};

int do_run(const Config& config, const Env& env, const RunOptions& options) {
  std::atomic<Kernel*> active{nullptr};
  std::mutex output_mutex;
  const bool allow = options.allow;
  const bool json = options.json;

  auto sink = [&](Event event) {
    std::lock_guard<std::mutex> lock(output_mutex);
    if (json) {
      std::cout << event_json(event) << '\n';
      return;
    }
    if (const auto* delta = std::get_if<StreamDelta>(&event.value)) {
      std::cout << delta->text << std::flush;
      return;
    }
    if (const auto* entry = std::get_if<TranscriptEntry>(&event.value)) {
      if (entry->kind == EntryKind::Tool) {
        std::cerr << "\n[tool] " << entry->tool << ' ' << entry->status << '\n';
      }
      return;
    }
    if (const auto* request = std::get_if<PermissionRequest>(&event.value)) {
      std::string option;
      for (const auto& candidate : request->options) {
        if (allow ? candidate.kind == "allow_once" : candidate.kind == "reject_once") {
          option = candidate.id;
          break;
        }
      }
      std::cerr << "\n[permission] " << request->tool << " -> "
                << (option.empty() ? "cancelled" : option) << '\n';
      if (Kernel* kernel = active.load(); kernel != nullptr) {
        const std::optional<std::string> choice =
            option.empty() ? std::nullopt : std::optional<std::string>(option);
        const auto answered = kernel->answer_permission(request->id.value, choice);
        if (!answered) {
          std::cerr << "booth: " << to_string(answered.error()) << '\n';
        }
      }
      return;
    }
    if (const auto* problem = std::get_if<Problem>(&event.value)) {
      std::cerr << "booth: " << to_string(Error{problem->layer, problem->detail, std::nullopt})
                << '\n';
    }
  };

  RunRequest request;
  request.text = options.text;
  request.cwd = options.cwd;
  request.model = options.model;

  const auto finish = [&](Result<RunResult> result) -> int {
    active = nullptr;
    if (!result) return fail(result.error());
    if (json) {
      std::cout << Json{{"session", result->session}, {"stopReason", result->stop_reason}}.dump()
                << '\n';
    } else {
      std::cout << '\n' << result->stop_reason << '\n';
    }
    return 0;
  };

  if (const int fd = connect_daemon(config.socket_file); fd >= 0) {
    RemoteKernel kernel(fd, sink);
    active = &kernel;
    return finish(run_task(kernel, request));
  }

  Supervisor supervisor(config, env, sink, BOOTH_VERSION);
  active = &supervisor;
  auto up = supervisor.up();
  if (!up) {
    active = nullptr;
    return fail(up.error());
  }
  auto result = run_task(supervisor, request);
  auto down = supervisor.down();
  const int code = finish(std::move(result));
  return down ? code : fail(down.error());
}

int print_status(const Config& config, bool json) {
  const Status status = probe_status(config);
  if (json) {
    std::cout << status_json(status) << '\n';
    return 0;
  }
  std::cout << "phase      " << (status.connected ? "ready" : "down") << '\n';
  std::cout << "detail     " << status.detail << '\n';
  if (status.pid > 0) std::cout << "pid        " << status.pid << '\n';
  std::cout << "profile    " << status.profile << " (base " << status.base_profile << ")\n";
  std::cout << "state      " << status.state_dir.string() << '\n';
  return 0;
}

int print_sessions(const Config& config, const Env& env, bool json) {
  const auto sink = [](Event) {};
  const auto list = [](Kernel& kernel) {
    return kernel.request("session/list", acp::Json::object(), std::chrono::seconds(10));
  };

  Result<acp::Json> result =
      std::unexpected(Error{Layer::Kernel, "no kernel", std::nullopt});
  if (const int fd = connect_daemon(config.socket_file); fd >= 0) {
    RemoteKernel kernel(fd, sink);
    result = list(kernel);
  } else {
    Supervisor supervisor(config, env, sink, BOOTH_VERSION);
    auto up = supervisor.up();
    if (!up) return fail(up.error());
    result = list(supervisor);
    auto down = supervisor.down();
    if (!down) std::cerr << "booth: " << to_string(down.error()) << '\n';
  }

  if (!result) return fail(result.error());
  if (json) {
    std::cout << result->dump(2) << '\n';
    return 0;
  }
  const auto sessions = result->value("sessions", acp::Json::array());
  if (sessions.empty()) {
    std::cout << "no sessions\n";
    return 0;
  }
  for (const auto& session : sessions) {
    std::cout << session.value("sessionId", std::string{"?"}) << '\t'
              << session.value("title", std::string{"-"}) << '\t'
              << session.value("cwd", std::string{"-"}) << '\n';
  }
  return 0;
}

int print_logs(const Config& config, std::size_t lines) {
  Log log(config.log_file);
  auto tail = log.tail(lines);
  if (!tail) return fail(tail.error());
  for (const auto& line : *tail) std::cout << line << '\n';
  return 0;
}

}  // namespace

int run_cli(int argc, char** argv) {
  CLI::App app{"A terminal console for a dsh kernel"};
  app.name("booth");
  app.description("Runs a dsh kernel with the booth profile and drives it over ACP.");
  app.set_version_flag("-V,--version", BOOTH_VERSION);

  auto* run = app.add_subcommand("run", "Run one task through the kernel");
  std::string text;
  run->add_option("text", text, "The task")->required();
  std::string cwd;
  run->add_option("--cwd", cwd, "Working directory (default: the current one)");
  std::string model{"auto"};
  run->add_option("--model", model, "Model id, or auto")->default_val("auto");
  bool allow = false;
  run->add_flag("--allow", allow, "Allow once permissions; the default denies them");
  bool run_json = false;
  run->add_flag("--json", run_json, "Print protocol events as JSON lines");

  auto* up = app.add_subcommand("up", "Start the supervisor daemon");
  auto* down = app.add_subcommand("down", "Stop the supervisor daemon");
  auto* restart = app.add_subcommand("restart", "Restart the supervisor daemon");
  auto* daemon = app.add_subcommand("supervisor", "Run the supervisor daemon in the foreground");

  auto* sessions = app.add_subcommand("sessions", "List sessions the kernel knows");
  bool sessions_json = false;
  sessions->add_flag("--json", sessions_json, "Print the served view as JSON");

  auto* status = app.add_subcommand("status", "Report the supervisor process and profile");
  bool status_json = false;
  status->add_flag("--json", status_json, "Print the status as JSON");

  auto* logs = app.add_subcommand("logs", "Print the supervisor log");
  std::size_t lines = 40;
  logs->add_option("--lines", lines, "How many trailing lines to print")->default_val(40);


  try {
    app.parse(argc, argv);
  } catch (const CLI::ParseError& error) {
    return app.exit(error);
  }

  auto config = Config::from_env(system_env());
  if (!config) return fail(config.error());
  const Env env = system_env();

  if (run->parsed()) {
    RunOptions options;
    options.text = text;
    options.cwd = cwd;
    options.model = model;
    options.allow = allow;
    options.json = run_json;
    if (options.cwd.empty()) options.cwd = std::filesystem::current_path().string();
    return do_run(*config, env, options);
  }
  if (up->parsed()) {
    auto started = start_daemon(*config, env);
    if (!started) return fail(started.error());
    return print_status(*config, status_json);
  }
  if (down->parsed()) {
    auto stopped = stop_daemon(*config);
    if (!stopped) return fail(stopped.error());
    std::cout << "stopped\n";
    return 0;
  }
  if (restart->parsed()) {
    auto stopped = stop_daemon(*config);
    if (!stopped) return fail(stopped.error());
    auto started = start_daemon(*config, env);
    if (!started) return fail(started.error());
    return print_status(*config, status_json);
  }
  if (daemon->parsed()) return run_daemon(*config, env, BOOTH_VERSION);
  if (sessions->parsed()) return print_sessions(*config, env, sessions_json);
  if (status->parsed()) return print_status(*config, status_json);
  if (logs->parsed()) return print_logs(*config, lines);

  if (isatty(STDIN_FILENO) != 0) return run_tui(*config, env);
  std::cout << app.help();
  return 0;
}

}  // namespace booth
