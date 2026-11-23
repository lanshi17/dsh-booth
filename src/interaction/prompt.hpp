#pragma once

#include <filesystem>
#include <functional>
#include <mutex>
#include <string>
#include <utility>
#include <vector>

#include "contract/event.hpp"
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
  std::function<void(Event)> publish;
};

/// Opens the ACP session when needed and publishes its model/effort options.
std::string ensure_session(PromptContext& context);

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

/// Closes the active session; the next prompt opens a new one.
void close_session(PromptContext& context);

}  // namespace booth
