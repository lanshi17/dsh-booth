#pragma once

#include <filesystem>
#include <functional>
#include <mutex>
#include <string>
#include <utility>
#include <vector>

#include "contract/event.hpp"
#include "control/client.hpp"
#include "interaction/state.hpp"
#include "interaction/view.hpp"
#include "subagent/acp/messages.hpp"
#include "subagent/kernel.hpp"

namespace booth {

struct PromptContext {
  Kernel& kernel;
  AppState& state;
  std::mutex& mutex;
  std::string& session;
  std::string cwd;
  std::filesystem::path sessions_dir;
  std::filesystem::path mcp_file;
  std::function<void(Event)> publish;
};

/// Opens the ACP session when needed and publishes its model/effort options.
std::string ensure_session(PromptContext& context);

/// Resumes a restart marker when present, otherwise opens a fresh session.
void ensure_session_or_restart(PromptContext& context, const std::filesystem::path& marker);

/// Opens a session when needed, sends the prompt and resource links, then refreshes the title.
void run_prompt(PromptContext& context, std::string text,
                const std::vector<std::pair<std::string, std::string>>& resources = {},
                const std::vector<acp::ImageAttachment>& images = {});

/// Applies a model selection to the current session.
void apply_model(PromptContext& context, const ModelChoice& choice);

/// Applies a reasoning-effort selection to the current session.
void set_reasoning(PromptContext& context, const std::string& config_id,
                   const ModelChoice& choice);

/// Fetches dsh's persisted sessions and publishes them.
void list_sessions(PromptContext& context);

/// Resumes a persisted session and starts a fresh transcript view for it.
void resume_session(PromptContext& context, const SessionRow& selected);

/// Closes the active session; the next prompt opens a new one. The notice
/// replaces the transcript's default "session closed" line.
void close_session(PromptContext& context, std::string notice = "session closed");

/// Close the active session, resume the selected one, and replay it.
void switch_session(PromptContext& context, const SessionRow& selected);

/// Close and re-resume the same session, resetting process stream state while
/// replaying the persisted transcript.
void fresh_session(PromptContext& context);

/// Close the session, have the control plane relocate it to another directory,
/// and resume the relocated child.
void move_session(PromptContext& context, ControlClient& control, const std::string& id,
                  const std::string& path);

/// Close the session, create a git worktree through the control plane, and
/// resume the relocated child.
void worktree_session(PromptContext& context, ControlClient& control, const std::string& id,
                      const std::string& branch);

/// Close and delete the active session, then open a fresh one.
void delete_session(PromptContext& context, ControlClient& control, const std::string& id);

/// Cancels the current ACP turn and dismisses an active permission request.
void cancel_turn(PromptContext& context);

}  // namespace booth
