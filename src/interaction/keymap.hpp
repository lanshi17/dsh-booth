#pragma once

#include <filesystem>
#include <optional>
#include <string>
#include <string_view>

#include "contract/error.hpp"

#include <ftxui/component/event.hpp>

namespace booth {

struct Chord {
  enum class Kind { None, Ctrl, Alt, Escape, Enter } kind{Kind::None};
  char key{0};
};

/// Parses `Ctrl+C`, `Alt+D`, `Esc`, `Escape`, `Enter`. Case-insensitive.
std::optional<Chord> parse_chord(std::string_view text);

/// Whether a terminal event carries this chord.
bool matches_chord(const Chord& chord, const ftxui::Event& event);

struct KeyBindings {
  Chord cancel{Chord::Kind::Ctrl, 'C'};
  Chord quit{Chord::Kind::Ctrl, 'D'};
  Chord editor{Chord::Kind::Ctrl, 'G'};
};

/// Reads `action: chord` lines. A missing file keeps the defaults; one bad line is an error.
Result<KeyBindings> load_keybindings(const std::filesystem::path& path);

}  // namespace booth
