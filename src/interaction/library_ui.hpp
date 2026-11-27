#pragma once

#include <cstddef>
#include <filesystem>
#include <optional>
#include <string>
#include <vector>

#include <ftxui/component/event.hpp>
#include <ftxui/component/screen_interactive.hpp>
#include <ftxui/dom/elements.hpp>

#include "interaction/state.hpp"

namespace booth {

class Editor;
enum class LibraryKind { None, Skills, Settings };

struct LibraryRow {
  std::string value;
  std::string label;
  std::string detail;
};

/// Live booth toggles the settings overlay reads and writes.
struct LibrarySettings {
  bool* show_thoughts{nullptr};
  bool* tools_hidden{nullptr};
  bool* tools_expanded{nullptr};
  bool* mouse_enabled{nullptr};
  ftxui::ScreenInteractive* screen{nullptr};
  std::filesystem::path state_dir;
};

/// Full-screen `/skills` and `/settings` list with filtering and keyboard selection.
class LibraryOverlay {
 public:
  void open(LibraryKind kind, std::string title, std::vector<LibraryRow> rows);
  void close();
  [[nodiscard]] bool visible() const { return visible_; }
  [[nodiscard]] LibraryKind kind() const { return kind_; }
  /// Returns the selected value on Enter; leaves the overlay open.
  std::optional<std::string> key(ftxui::Event& event);
  [[nodiscard]] ftxui::Element view() const;

 private:
  void filter();

  bool visible_{false};
  LibraryKind kind_{LibraryKind::None};
  std::string title_;
  std::string query_;
  std::size_t index_{0};
  std::vector<LibraryRow> rows_;
  std::vector<std::size_t> matches_;
};

/// Handles `/skills` and `/providers`; true when the line was consumed.
bool handle_library_command(const std::string& line, AppState& state, LibraryOverlay& overlay,
                            const std::filesystem::path& cwd, const LibrarySettings& settings,
                            const Editor& editor);

/// Handles overlay navigation and applies the selection; true while the overlay is visible.
bool handle_library_event(ftxui::Event& event, AppState& state, LibraryOverlay& overlay,
                          Editor& editor, const LibrarySettings& settings);

}  // namespace booth
