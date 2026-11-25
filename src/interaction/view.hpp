#pragma once

#include "contract/options.hpp"
#include "interaction/commands.hpp"
#include "interaction/copy_picker.hpp"
#include "interaction/file_menu.hpp"
#include "interaction/keymap.hpp"
#include "interaction/model_picker.hpp"
#include "interaction/mouse_zone.hpp"
#include "interaction/vim.hpp"

#include <deque>
#include <filesystem>
#include <optional>
#include <set>
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
                             std::size_t cursor_column, const std::string& reasoning,
                             std::size_t queued, const std::string& mode, std::size_t images,
                             MouseRouter* mouse = nullptr,
                             const std::optional<vim::Selection>& selection = std::nullopt,
                             bool block_cursor = false);

/// The rows of an oversized editor that fit around the cursor.
struct RowWindow {
  std::size_t begin{0};
  std::size_t end{0};
  std::size_t hidden_above{0};
  std::size_t hidden_below{0};
};

RowWindow row_window(std::size_t count, std::size_t cursor, std::size_t visible);

/// A tool-call frame; `expanded` reveals full output and diffs are rendered when present.
ftxui::Element tool_view(const TranscriptEntry& entry, bool expanded);

/// The count and text of prompts waiting for the active turn to finish.
ftxui::Element queued_view(const std::deque<std::string>& queued);

/// A bounded checklist above the composer; nullopt when no open todos remain.
std::optional<ftxui::Element> todo_hud(const std::vector<TodoItem>& todos);

/// Flatten a dsh read envelope into path, numbered rows and footer text.
std::string read_output_text(const std::string& output);

/// One transcript row. `expand_tools` reveals full tool output.
ftxui::Element entry_element(const TranscriptEntry& entry, bool expand_tools = false);

/// Markdown for streamed assistant text.
ftxui::Element markdown_view(const std::string& text, bool dim);

/// Inline emphasis/code spans for one line (used by the user band).
ftxui::Element inline_words_view(const std::string& text, bool dim, int width = 0);

/// One scrollable row of the help card.
struct HelpLine {
  std::string text;
  bool dim{false};
};

/// The scrollable help body: key hints, command reference, aliases.
std::vector<HelpLine> help_lines(const KeyBindings& keys);
std::vector<HelpLine> help_lines();

/// How many help body rows fit above the composer at the current terminal height.
int help_visible_lines();

/// Largest valid scroll offset for the help body.
int help_max_offset();

/// The help card; `offset` is clamped to the scrollable range for this frame.
ftxui::Element help_view(int offset, const KeyBindings& keys, MouseRouter* mouse = nullptr);

/// The full-screen permission card.
ftxui::Element permission_view(const PermissionRequest& request, MouseRouter* mouse = nullptr,
                               bool expanded = false);

/// One line of phase, session and model, for `/status`.
std::string status_text(const AppState& state);

/// Prints the omp-style splash once, before the inline app starts.
void print_splash();

/// Flattens dsh model config options into display-name/wire-value pairs.
void collect_model_options(const nlohmann::json& option, std::vector<ModelChoice>& out,
                           const std::string& provider = {});

/// Reads the model option, fills the choices and returns the display name (empty if absent).
std::string read_model_options(const nlohmann::json& config_options,
                               std::vector<ModelChoice>& choices);

/// Reads the reasoning-effort option (category `thought_level`).
ReasoningOptions read_reasoning_options(const nlohmann::json& config_options);

/// The searchable `/model` selection list.
ftxui::Element model_picker_view(const std::vector<ModelChoice>& choices,
                                 const std::vector<std::size_t>& matches, std::size_t index,
                                 const std::string& current, const std::string& query,
                                 const std::string& provider, bool model_focus,
                                 MouseRouter* mouse = nullptr);

/// The inline `@path` completion rows.
ftxui::Element file_menu_view(const std::string& query,
                              const std::vector<std::string>& matches, std::size_t index,
                              MouseRouter* mouse = nullptr);

/// The `/copy` picker rows.
ftxui::Element copy_picker_view(const std::vector<CopyItem>& items,
                                const std::vector<std::size_t>& matches, std::size_t index,
                                const std::string& query, MouseRouter* mouse = nullptr);

/// The Ctrl+R prompt-history search overlay.
ftxui::Element history_search_view(const std::string& query,
                                   const std::vector<std::string>& matches,
                                   std::size_t index, MouseRouter* mouse = nullptr);

/// The searchable `/sessions` list; an empty list renders as a loading card.
ftxui::Element session_picker_view(const std::vector<SessionRow>& sessions,
                                   const std::vector<std::size_t>& matches, std::size_t index,
                                   bool loading, const std::string& query,
                                   MouseRouter* mouse = nullptr);

enum class SlashAction { None, Handled, Submit };

/// The `/sessions` picker: navigation, refresh/close commands and load state.
class SessionPicker {
 public:
  enum class Action { None, Refresh, Close, Resume };

  void open(const std::filesystem::path& pins_file = {});
  void close();
  void loaded(std::vector<SessionRow>& sessions);

  [[nodiscard]] bool visible() const { return visible_; }
  [[nodiscard]] bool loading() const { return loading_; }
  [[nodiscard]] std::size_t index() const { return index_; }
  [[nodiscard]] const std::string& query() const { return query_; }
  [[nodiscard]] const std::vector<std::size_t>& matches() const { return matches_; }

  Action key(ftxui::Event& event, std::vector<SessionRow>& sessions);

  /// Selects and resumes one rendered row, exactly like keyboard selection + Enter.
  Action click(std::size_t row, const std::vector<SessionRow>& sessions);

 private:
  void filter(const std::vector<SessionRow>& sessions);

  bool visible_{false};
  bool loading_{false};
  std::size_t index_{0};
  std::string query_;
  std::vector<std::size_t> matches_;
  std::filesystem::path pins_file_;
  std::set<std::string> pins_;
};

/// The inline slash-command menu: filter, selection, completion and dismissal state.
class SlashMenu {
 public:
  /// Reopens the menu and resets the selection when the composer text changes.
  void sync(const std::string& composer);

  /// Whether the dropdown should be drawn for this composer text.
  [[nodiscard]] bool visible(const std::string& composer, const AppState& state) const;

  /// Handles navigation, Tab/Enter completion and Esc dismissal.
  SlashAction key(ftxui::Event& event, AppState& state);

  /// Selects and activates one rendered row, exactly like keyboard selection + Enter.
  SlashAction click(std::size_t row, AppState& state);

  [[nodiscard]] std::size_t index() const { return index_; }

 private:
  bool hidden_{false};
  bool moved_{false};
  std::string seen_;
  std::size_t index_{0};
};

/// The slash-command menu rows.
ftxui::Element command_menu_view(const AppState& state, const std::string& composer,
                                 std::size_t index, MouseRouter* mouse = nullptr);

}  // namespace booth
