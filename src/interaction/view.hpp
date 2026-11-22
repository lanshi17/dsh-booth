#pragma once

#include "contract/options.hpp"

#include <string>
#include <utility>
#include <vector>

#include <nlohmann/json.hpp>

#include <ftxui/component/event.hpp>
#include <ftxui/dom/elements.hpp>

#include "interaction/state.hpp"

namespace booth {

struct GitStatus {
  bool valid{false};
  std::string branch;
  int modified{0};
  int staged{0};
  int untracked{0};
};

/// Reads the branch and porcelain counts once; empty when the cwd is not a repository.
GitStatus read_git(const std::string& cwd);

/// The two-row composer: status embedded in the top border, the input merged into the bottom.
ftxui::Element composer_view(const AppState& state, const std::string& cwd,
                             const GitStatus& git, const std::string& session_title,
                             long busy_seconds, const std::string& spinner,
                             const std::vector<std::string>& lines, std::size_t cursor_line,
                             std::size_t cursor_column, const std::string& reasoning);

/// One transcript row.
ftxui::Element entry_element(const TranscriptEntry& entry);

/// Markdown for streamed assistant text.
ftxui::Element markdown_view(const std::string& text, bool dim);

/// Inline emphasis/code spans for one line (used by the user band).
ftxui::Element inline_words_view(const std::string& text, bool dim);

/// The full-screen help card.
ftxui::Element help_view();

/// The full-screen permission card.
ftxui::Element permission_view(const PermissionRequest& request);

/// One line of phase, session and model, for `/status`.
std::string status_text(const AppState& state);

/// Prints the omp-style splash once, before the inline app starts.
void print_splash();

struct CommandHint {
  const char* name;
  const char* description;
};

/// Commands matching a `/` prefix, in display order.
std::vector<const CommandHint*> matching_commands(const std::string& composer);

/// Flattens dsh model config options into display-name/wire-value pairs.
void collect_model_options(const nlohmann::json& option, std::vector<ModelChoice>& out,
                           const std::string& provider = {});

/// Reads the model option, fills the choices and returns the display name (empty if absent).
std::string read_model_options(const nlohmann::json& config_options,
                               std::vector<ModelChoice>& choices);

/// Reads the reasoning-effort option (category `thought_level`).
ReasoningOptions read_reasoning_options(const nlohmann::json& config_options);

/// The `/model` selection list.
ftxui::Element model_picker_view(const std::vector<ModelChoice>& choices, std::size_t index,
                                 const std::string& current);

/// The `/sessions` list; an empty list renders as a loading card.
ftxui::Element session_picker_view(const std::vector<SessionRow>& sessions, std::size_t index,
                                   bool loading);

enum class SlashAction { None, Handled, Submit };

/// Handles ↑/↓/Tab/Enter while the slash menu is open; updates the composer for Tab/Enter.
SlashAction slash_menu_key(const ftxui::Event& event, AppState& state, std::size_t& index);

/// The slash-command menu rows.
ftxui::Element command_menu_view(const std::string& composer, std::size_t index);

}  // namespace booth
