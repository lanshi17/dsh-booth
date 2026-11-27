#pragma once

#include <filesystem>
#include <functional>
#include <string>
#include <string_view>

#include "contract/event.hpp"
#include "control/client.hpp"

namespace booth {

struct AppState;

/// The TUI-owned sinks a control command needs: one serialized request worker,
/// one transcript append, and the daemon-side socket.
struct ControlHooks {
  ControlClient* control{nullptr};
  std::string cwd;
  std::filesystem::path state_dir;
  AppState* state{nullptr};
  std::function<void(TranscriptEntry)> publish;
  std::function<void(std::function<void()>)> post;
  std::function<void(const SessionRow&)> switch_session;
  std::function<void(const std::string&)> delete_session;
  std::function<void()> fresh_session;
  std::function<void(const std::string& id, const std::string& path)> move_session;
  std::function<void(const std::string& id, const std::string& branch)> worktree_session;
  std::function<void(const std::string&)> title_changed;
  std::function<void(const std::string& value, const std::string& name)> apply_model;
  std::function<void(const std::string& id, const ModelChoice& choice)> apply_reasoning;
  std::function<std::optional<std::string>(const std::string&)> edit_text;
};

/// Run a control-backed slash command. Returns false when the line is not one,
/// so the caller can continue its local dispatch.
bool handle_control_command(const ControlHooks& hooks, std::string_view line,
                            const std::string& session);

/// Whether a line names a control-backed command; used by the busy gate.
bool is_control_command(std::string_view line);

}  // namespace booth
