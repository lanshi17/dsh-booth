#pragma once

#include <filesystem>
#include <optional>
#include <string>
#include <string_view>

#include "contract/error.hpp"

#include <ftxui/component/event.hpp>

namespace booth {

struct Chord {
  enum class Kind { None, Ctrl, Alt, Shift, Escape, Enter, BackTab, Special };
  enum class Special { None, Up, Down, PageUp, PageDown, Home, End };
  Kind kind{Kind::None};
  char key{0};
  Special special{Special::None};
};

/// Parses `Ctrl+C`, `Alt+D`, `Esc`, `Escape`, `Enter`. Case-insensitive.
std::optional<Chord> parse_chord(std::string_view text);

/// Whether a terminal event carries this chord.
bool matches_chord(const Chord& chord, const ftxui::Event& event);

/// Human-readable chord label (`Ctrl+C`, `Alt+Up`, ...).
std::string chord_label(const Chord& chord);

struct KeyBindings {
  Chord cancel{Chord::Kind::Ctrl, 'C'};
  Chord quit{Chord::Kind::Ctrl, 'D'};
  Chord editor{Chord::Kind::Ctrl, 'G'};
  Chord history{Chord::Kind::Ctrl, 'R'};
  Chord tools{Chord::Kind::Ctrl, 'O'};
  Chord thinking{Chord::Kind::Ctrl, 'T'};
  Chord hide_tools{Chord::Kind::Alt, 'O'};
  Chord paste_image{Chord::Kind::Ctrl, 'V'};
  Chord reasoning{Chord::Kind::BackTab};
  Chord page_up{Chord::Kind::Special, 0, Chord::Special::PageUp};
  Chord page_down{Chord::Kind::Special, 0, Chord::Special::PageDown};
  Chord dequeue{Chord::Kind::Alt, 0, Chord::Special::Up};
  Chord help{Chord::Kind::Alt, 'H'};
  Chord model{Chord::Kind::Alt, 'M'};
  Chord sessions{Chord::Kind::Alt, 'S'};
  Chord copy{Chord::Kind::Alt, 'C'};
  Chord links{Chord::Kind::Alt, 'L'};
  Chord model_next{Chord::Kind::Ctrl, 'P'};
  Chord retry{Chord::Kind::Alt, 'R'};
};

/// Reads `action: chord` lines. A missing file keeps the defaults; one bad line is an error.
Result<KeyBindings> load_keybindings(const std::filesystem::path& path);

}  // namespace booth
